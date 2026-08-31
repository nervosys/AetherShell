// Snippets are code the extension writes into the user's file. They should at
// minimum be well-formed, use placeholders VS Code understands, and not teach
// syntax the language does not have.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { extRoot, tokenize, expandSnippet } from "./tokenize.mjs";

const snippets = JSON.parse(
  readFileSync(join(extRoot, "snippets", "aethershell.json"), "utf8"),
);
const entries = Object.entries(snippets);

const bodyOf = (s) => (Array.isArray(s.body) ? s.body.join("\n") : s.body);

test("there are snippets to check", () => {
  assert.ok(entries.length > 5, `only ${entries.length} snippets`);
});

test("every snippet has a prefix, a body and a description", () => {
  for (const [name, s] of entries) {
    assert.ok(s.prefix, `${name}: no prefix`);
    assert.ok(s.body, `${name}: no body`);
    assert.ok(
      s.description,
      `${name}: no description — the completion list shows the prefix alone`,
    );
  }
});

test("snippet prefixes are unique", () => {
  const seen = new Map();
  for (const [name, s] of entries) {
    for (const p of [s.prefix].flat()) {
      assert.ok(
        !seen.has(p),
        `prefix "${p}" is used by both ${seen.get(p)} and ${name}; one shadows the other`,
      );
      seen.set(p, name);
    }
  }
});

test("placeholders are well-formed and numbered from 1", () => {
  for (const [name, s] of entries) {
    const body = bodyOf(s);

    // Unescaped `$` must introduce $0, $1, ${1}, ${1:label} or ${1|a,b|}.
    for (const m of body.matchAll(/(^|[^\\])\$(.)/g)) {
      const next = m[2];
      assert.ok(
        /[0-9{]/.test(next),
        `${name}: "$${next}" is not a placeholder and is not escaped — VS Code \
will drop it`,
      );
    }

    const nums = [...body.matchAll(/\$\{?(\d+)/g)].map((m) => Number(m[1]));
    if (nums.length === 0) continue;
    const positive = nums.filter((n) => n > 0);
    if (positive.length > 0) {
      assert.equal(
        Math.min(...positive),
        1,
        `${name}: tab stops start at ${Math.min(...positive)}; VS Code visits \
them in ascending order from 1, so a gap sends the cursor somewhere unexpected`,
      );
      const sorted = [...new Set(positive)].sort((a, b) => a - b);
      for (let i = 0; i < sorted.length; i++) {
        assert.equal(
          sorted[i],
          i + 1,
          `${name}: tab stops ${sorted.join(",")} skip a number`,
        );
      }
    }
  }
});

test("snippet bodies expand to balanced brackets", () => {
  const pairs = { ")": "(", "]": "[", "}": "{" };
  for (const [name, s] of entries) {
    // Expand properly rather than stripping placeholders with a regex: the
    // default of `${3:{}}` is `{`, so a naive strip leaves a stray brace and
    // reports a balanced snippet as broken.
    const body = expandSnippet(bodyOf(s));
    const stack = [];
    for (const ch of body) {
      if ("([{".includes(ch)) stack.push(ch);
      else if (ch in pairs) {
        assert.equal(
          stack.pop(),
          pairs[ch],
          `${name}: unbalanced "${ch}" in body`,
        );
      }
    }
    assert.equal(stack.length, 0, `${name}: ${stack.length} bracket(s) left open`);
  }
});

test("snippet bodies do not use keywords the language lacks", () => {
  // A snippet is a teaching device; one that expands to `for`/`while`/`return`
  // would teach syntax the parser rejects.
  const absent = ["for ", "while ", "return ", "elif ", "switch ", "def "];
  for (const [name, s] of entries) {
    const body = expandSnippet(bodyOf(s));
    for (const kw of absent) {
      assert.ok(
        !body.includes(kw),
        `${name}: body contains "${kw.trim()}", which AetherShell has no keyword for`,
      );
    }
  }
});

test("snippet bodies tokenize without falling into an error state", async () => {
  // Not a parse — the grammar is not a parser. But a body that leaves the
  // tokenizer inside an unterminated string or comment at EOF is malformed in
  // a way the user will see the moment it expands.
  for (const [name, s] of entries) {
    const tokens = await tokenize(expandSnippet(bodyOf(s)));
    assert.ok(tokens.length > 0, `${name}: body tokenizes to nothing`);

    const last = tokens[tokens.length - 1];
    assert.ok(
      !last.scopes.some((sc) => sc.startsWith("string.quoted")),
      `${name}: body ends inside an unterminated string — everything the user \
types after it loses highlighting`,
    );
  }
});
