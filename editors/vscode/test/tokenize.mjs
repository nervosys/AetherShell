// Run the shipped TextMate grammar the way VS Code runs it.
//
// `vscode-textmate` + `vscode-oniguruma` are the exact libraries the editor
// uses, driving the same Oniguruma regex engine. So these tests exercise the
// grammar as published rather than a reimplementation of it: if a pattern
// relies on Oniguruma behaviour that JavaScript's RegExp lacks, it is caught
// here and not by a user.

import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const vsctm = require("vscode-textmate");
const oniguruma = require("vscode-oniguruma");

const here = dirname(fileURLToPath(import.meta.url));
export const extRoot = join(here, "..");

const SCOPE = "source.aethershell";
const MARKDOWN_INJECTION = "markdown.aethershell.codeblock";
/// A stand-in for VS Code's markdown grammar. The injection is declared with
/// `injectionSelector: L:text.html.markdown`, so it cannot be tokenized on its
/// own — it needs a host to inject into. Shipping the real markdown grammar
/// here would test Microsoft's grammar; this stub is the smallest host that
/// lets the *injection* be exercised: it tokenizes nothing itself, so every
/// scope in the result came from the grammar under test.
export const MARKDOWN_HOST = "text.html.markdown";
const HOST_STUB = {
  scopeName: MARKDOWN_HOST,
  patterns: [],
};

let registryPromise = null;

async function makeRegistry() {
  const wasm = await readFile(
    require.resolve("vscode-oniguruma/release/onig.wasm"),
  );
  await oniguruma.loadWASM(wasm.buffer);

  const grammars = {
    [SCOPE]: join(extRoot, "syntaxes", "aethershell.tmLanguage.json"),
    [MARKDOWN_INJECTION]: join(
      extRoot,
      "syntaxes",
      "aethershell.markdown.tmLanguage.json",
    ),
  };

  return new vsctm.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (sources) => new oniguruma.OnigScanner(sources),
      createOnigString: (s) => new oniguruma.OnigString(s),
    }),
    loadGrammar: async (scopeName) => {
      if (scopeName === MARKDOWN_HOST) {
        return vsctm.parseRawGrammar(JSON.stringify(HOST_STUB), "host.json");
      }
      const path = grammars[scopeName];
      if (!path) return null;
      return vsctm.parseRawGrammar(await readFile(path, "utf8"), path);
    },
    getInjections: (scopeName) =>
      scopeName === MARKDOWN_HOST ? [MARKDOWN_INJECTION] : undefined,
  });
}

function registry() {
  if (!registryPromise) registryPromise = makeRegistry();
  return registryPromise;
}

/**
 * Tokenize source and return `[{ text, scopes }]`, dropping whitespace-only
 * tokens so assertions read as the code does.
 */
export async function tokenize(source, scopeName = SCOPE) {
  const grammar = await (await registry()).loadGrammar(scopeName);
  if (!grammar) throw new Error(`no grammar registered for ${scopeName}`);

  const out = [];
  let ruleStack = vsctm.INITIAL;
  for (const line of source.split(/\r?\n/)) {
    const result = grammar.tokenizeLine(line, ruleStack);
    for (const t of result.tokens) {
      const text = line.substring(t.startIndex, t.endIndex);
      if (text.trim() === "") continue;
      out.push({ text, scopes: t.scopes });
    }
    ruleStack = result.ruleStack;
  }
  return out;
}

/** The scopes applied to the first token whose text is exactly `text`. */
export async function scopesOf(source, text, scopeName = SCOPE) {
  const tokens = await tokenize(source, scopeName);
  // Several captures absorb trailing whitespace — `(mut\s+)` yields the token
  // "mut " — so compare trimmed. The scopes are what is under test, not the
  // exact span.
  const hit = tokens.find((t) => t.text.trim() === text);
  if (!hit) {
    throw new Error(
      `no token exactly "${text}" in:\n` +
        tokens.map((t) => `  ${JSON.stringify(t.text)}`).join("\n"),
    );
  }
  return hit.scopes;
}

/** Does any scope on this token start with `prefix`? */
export function hasScope(scopes, prefix) {
  return scopes.some((s) => s === prefix || s.startsWith(prefix + "."));
}

/**
 * Expand a VS Code snippet body to the text a user would end up with, taking
 * the default for every placeholder.
 *
 * Written because the obvious `body.replace(/\$\{[^}]*\}/g, "")` is wrong on
 * real snippets: `${3:{}}` has `{` as its default, so that regex stops at the
 * first `}` and leaves a stray brace behind — which looks exactly like an
 * unbalanced snippet and is not one. Inside `${n:...}` an unescaped `}` closes
 * the placeholder, so nested `${...}` is tracked but a bare `{` is literal.
 */
export function expandSnippet(body) {
  let out = "";
  let i = 0;
  while (i < body.length) {
    const ch = body[i];
    if (ch === "\\" && i + 1 < body.length) {
      out += body[i + 1];
      i += 2;
      continue;
    }
    if (ch === "$" && body[i + 1] === "{") {
      let depth = 1;
      let j = i + 2;
      let inner = "";
      while (j < body.length && depth > 0) {
        if (body[j] === "\\") {
          inner += body[j + 1] ?? "";
          j += 2;
          continue;
        }
        if (body[j] === "$" && body[j + 1] === "{") {
          depth++;
          inner += "${";
          j += 2;
          continue;
        }
        if (body[j] === "}") {
          depth--;
          if (depth === 0) break;
          inner += "}";
          j++;
          continue;
        }
        inner += body[j];
        j++;
      }
      // `1:default`, `1|a,b|` or just `1`.
      const colon = inner.indexOf(":");
      const bar = inner.indexOf("|");
      let value = "";
      if (colon !== -1) value = expandSnippet(inner.slice(colon + 1));
      else if (bar !== -1) value = inner.slice(bar + 1).split(",")[0];
      out += value;
      i = j + 1;
      continue;
    }
    if (ch === "$" && /\d/.test(body[i + 1] ?? "")) {
      i += 2;
      while (i < body.length && /\d/.test(body[i])) i++;
      continue;
    }
    out += ch;
    i++;
  }
  return out;
}
