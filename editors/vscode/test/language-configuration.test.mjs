// `language-configuration.json` drives bracket matching, auto-closing, comment
// toggling and indentation. It is pure data with no schema enforcement at
// runtime: a wrong comment token means ctrl-/ inserts something the parser
// rejects, and a malformed pair silently does nothing.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { extRoot, tokenize } from "./tokenize.mjs";

const cfg = JSON.parse(
  readFileSync(join(extRoot, "language-configuration.json"), "utf8"),
);

const openOf = (p) => (Array.isArray(p) ? p[0] : p.open);
const closeOf = (p) => (Array.isArray(p) ? p[1] : p.close);

test("the line comment token is what the grammar treats as a comment", async () => {
  const line = cfg.comments?.lineComment;
  assert.ok(line, "no lineComment configured — ctrl-/ does nothing");

  const probe = `${line} commented out`;
  const tokens = await tokenize(probe);
  assert.ok(
    tokens.length > 0 && tokens.every((t) => t.scopes.some((s) => s.startsWith("comment"))),
    `"${line}" is configured as the comment token but the grammar does not \
scope "${probe}" as a comment — ctrl-/ would produce a syntax error`,
  );
});

test("a configured block comment is also recognised by the grammar", async () => {
  const block = cfg.comments?.blockComment;
  if (!block) return; // Legitimately absent.
  const [open, close] = block;
  const tokens = await tokenize(`${open} hidden ${close}`);
  assert.ok(
    tokens.some((t) => t.scopes.some((s) => s.startsWith("comment"))),
    `block comment ${open}…${close} is configured but not scoped by the grammar`,
  );
});

test("brackets are well-formed pairs", () => {
  assert.ok(Array.isArray(cfg.brackets), "no brackets configured");
  assert.ok(cfg.brackets.length > 0, "brackets list is empty");
  for (const pair of cfg.brackets) {
    assert.equal(pair.length, 2, `bracket entry ${JSON.stringify(pair)} is not a pair`);
    const [open, close] = pair;
    assert.ok(open && close, `empty bracket in ${JSON.stringify(pair)}`);
    assert.notEqual(
      open,
      close,
      `bracket pair ${JSON.stringify(pair)} opens and closes with the same \
token, which VS Code cannot match`,
    );
  }
});

test("auto-closing pairs are consistent with brackets", () => {
  const pairs = cfg.autoClosingPairs ?? [];
  assert.ok(pairs.length > 0, "no autoClosingPairs configured");

  const bracketOpens = new Map(cfg.brackets.map(([o, c]) => [o, c]));
  for (const p of pairs) {
    const open = openOf(p);
    const close = closeOf(p);
    assert.ok(open && close, `malformed autoClosingPair ${JSON.stringify(p)}`);
    if (bracketOpens.has(open)) {
      assert.equal(
        close,
        bracketOpens.get(open),
        `"${open}" closes with "${bracketOpens.get(open)}" in brackets but \
"${close}" in autoClosingPairs`,
      );
    }
  }
});

test("every bracket pair also auto-closes", () => {
  const autoOpens = new Set((cfg.autoClosingPairs ?? []).map(openOf));
  const missing = cfg.brackets.filter(([o]) => !autoOpens.has(o)).map(([o]) => o);
  assert.deepEqual(
    missing,
    [],
    `these brackets match but do not auto-close, which is inconsistent typing \
behaviour: ${missing.join(" ")}`,
  );
});

test("surrounding pairs are well-formed", () => {
  for (const p of cfg.surroundingPairs ?? []) {
    assert.ok(
      openOf(p) && closeOf(p),
      `malformed surroundingPair ${JSON.stringify(p)}`,
    );
  }
});

test("string and comment pairs do not auto-close inside themselves", () => {
  // A quote configured to auto-close without `notIn` doubles up when you type
  // a quote inside a string.
  for (const p of cfg.autoClosingPairs ?? []) {
    const open = openOf(p);
    if (open !== '"' && open !== "'" && open !== "`") continue;
    const notIn = Array.isArray(p) ? undefined : p.notIn;
    assert.ok(
      notIn && notIn.includes("string"),
      `the ${open} pair should declare notIn:["string"] so typing a quote \
inside a string does not insert a second one`,
    );
  }
});
