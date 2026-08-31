// Tokenization tests, run against the shipped grammar through the same
// libraries VS Code uses.
//
// Written after a probe showed the grammar doing something no static check
// would have caught: the brace rule matched *every* `{`, not just record
// literals, and its inner pattern list omitted builtins, function calls,
// member access, variables and operators. So inside any function body,
// `if` block or `try` block — which is most real code — `git.log()` and
// `print(...)` came back as unscoped text. Static inspection said the rules
// existed; only running them showed they were unreachable.

import { test } from "node:test";
import assert from "node:assert/strict";
import { tokenize, scopesOf, hasScope, MARKDOWN_HOST } from "./tokenize.mjs";

const KEYWORD_SCOPES = {
  if: "keyword.control",
  else: "keyword.control",
  match: "keyword.control",
  try: "keyword.control.exception",
  catch: "keyword.control.exception",
  throw: "keyword.control.exception",
  async: "keyword.control.async",
  await: "keyword.control.async",
  import: "keyword.control.import",
  export: "keyword.control.import",
  from: "keyword.control.import",
  as: "keyword.control.import",
  let: "storage.modifier",
  mut: "storage.modifier",
  pub: "storage.modifier",
  fn: "keyword.other.fn",
};

test("every keyword the parser accepts is scoped", async () => {
  for (const [kw, expected] of Object.entries(KEYWORD_SCOPES)) {
    // `mut` only appears after `let`; give each keyword a context where it is
    // syntactically plausible rather than asserting on a bare word.
    const src = kw === "mut" ? "let mut x = 1" : `${kw} x`;
    const scopes = await scopesOf(src, kw);
    assert.ok(
      hasScope(scopes, expected),
      `"${kw}" got ${JSON.stringify(scopes)}, expected something under ${expected}`,
    );
  }
});

test("literals are scoped as constants", async () => {
  assert.ok(
    hasScope(await scopesOf("let a = true", "true"), "constant.language.boolean"),
  );
  assert.ok(
    hasScope(await scopesOf("let a = false", "false"), "constant.language.boolean"),
  );
  assert.ok(
    hasScope(await scopesOf("let a = null", "null"), "constant.language.null"),
  );
  assert.ok(
    hasScope(await scopesOf("let a = 42", "42"), "constant.numeric.integer"),
  );
  assert.ok(
    hasScope(await scopesOf("let a = 2.5", "2.5"), "constant.numeric.float"),
  );
});

// ── The regression this suite was written for ────────────────────────────

const INSIDE_BLOCKS = [
  ["function body", (body) => `fn f() {\n  ${body}\n}`],
  ["if block", (body) => `if cond {\n  ${body}\n}`],
  ["else block", (body) => `if cond { a } else {\n  ${body}\n}`],
  ["try block", (body) => `try {\n  ${body}\n} catch e { a }`],
  ["nested block", (body) => `fn f() {\n  if c {\n    ${body}\n  }\n}`],
];

test("builtins are highlighted inside every kind of block", async () => {
  for (const [label, wrap] of INSIDE_BLOCKS) {
    const scopes = await scopesOf(wrap('print("x")'), "print");
    assert.ok(
      hasScope(scopes, "support.function.builtin"),
      `${label}: "print" got ${JSON.stringify(scopes)} — the block rule is \
swallowing its contents again`,
    );
  }
});

test("member access is highlighted inside every kind of block", async () => {
  for (const [label, wrap] of INSIDE_BLOCKS) {
    const tokens = await tokenize(wrap("git.log()"));
    const dot = tokens.find((t) => t.text === ".");
    const member = tokens.find((t) => t.text === "log");
    assert.ok(dot, `${label}: no "." token at all`);
    assert.ok(
      hasScope(dot.scopes, "keyword.operator.accessor"),
      `${label}: "." got ${JSON.stringify(dot.scopes)}`,
    );
    assert.ok(
      member && hasScope(member.scopes, "entity.name.function"),
      `${label}: member "log" got ${JSON.stringify(member?.scopes)}`,
    );
  }
});

test("operators and calls survive inside blocks", async () => {
  const src = "fn f() {\n  let n = a + b\n  g(n)\n}";
  assert.ok(
    hasScope(await scopesOf(src, "+"), "keyword.operator.arithmetic"),
    "arithmetic operator lost inside a block",
  );
  assert.ok(
    hasScope(await scopesOf(src, "g"), "entity.name.function"),
    "function call lost inside a block",
  );
});

// ── False positives: things that must NOT be highlighted ─────────────────

test("a variable named like a builtin is not scoped as one", async () => {
  // `log`, `set`, `get`, `merge` and `exec` were matched as bare builtins
  // until 1.6.0, though each is reachable only as a module member.
  for (const name of ["log", "set", "get", "merge", "exec"]) {
    const scopes = await scopesOf(`let ${name} = 1`, name);
    assert.ok(
      !hasScope(scopes, "support.function.builtin"),
      `"${name}" is scoped as a builtin: ${JSON.stringify(scopes)}`,
    );
  }
});

test("keywords inside comments and strings are not scoped as keywords", async () => {
  const comment = await scopesOf("# let else async print", "# let else async print");
  assert.ok(hasScope(comment, "comment"));
  assert.ok(
    !comment.some((s) => s.startsWith("keyword")),
    `comment leaked keyword scopes: ${JSON.stringify(comment)}`,
  );

  const inString = await tokenize('"let else print"');
  for (const t of inString) {
    assert.ok(
      !t.scopes.some((s) => s.startsWith("keyword.control")),
      `string content ${JSON.stringify(t.text)} leaked ${JSON.stringify(t.scopes)}`,
    );
  }
});

test("an identifier merely containing a keyword is not a keyword", async () => {
  for (const name of ["iffy", "letter", "matcher", "asynchronous", "elsewhere"]) {
    const scopes = await scopesOf(`${name} = 1`, name);
    assert.ok(
      !scopes.some((s) => s.startsWith("keyword.control")),
      `"${name}" was scoped ${JSON.stringify(scopes)} — a \\b boundary is missing`,
    );
  }
});

// ── Declarations ─────────────────────────────────────────────────────────

test("declarations distinguish functions from plain bindings", async () => {
  assert.ok(
    hasScope(
      await scopesOf("let f = fn(x) => x", "f"),
      "entity.name.function.definition",
    ),
    "a function binding is not marked as a function definition",
  );
  assert.ok(
    hasScope(await scopesOf("let y = 1", "y"), "variable.other.declaration"),
    "a plain binding is not marked as a declaration",
  );
  assert.ok(
    hasScope(await scopesOf("let mut z = 2", "mut"), "storage.modifier.mutable"),
    "`mut` is not marked as a mutability modifier",
  );
  assert.ok(
    hasScope(await scopesOf("let y = 1", "="), "keyword.operator.assignment"),
    "the assignment operator is unscoped in a declaration",
  );
});

// ── Strings and comments ─────────────────────────────────────────────────

test("strings and comments terminate correctly", async () => {
  // A string that ends must not swallow the rest of the file: if it did,
  // everything after it would silently lose highlighting.
  const after = await scopesOf('let a = "x"\nlet b = 2', "2");
  assert.ok(
    hasScope(after, "constant.numeric"),
    `code after a closed string is not tokenized: ${JSON.stringify(after)}`,
  );

  const afterComment = await scopesOf("# note\nlet b = 2", "2");
  assert.ok(
    hasScope(afterComment, "constant.numeric"),
    "a line comment leaked past its newline",
  );
});

test("an unterminated string does not crash the tokenizer", async () => {
  const tokens = await tokenize('let a = "oops\nlet b = 2');
  assert.ok(tokens.length > 0, "tokenizer produced nothing");
});

// ── Pipelines and lambdas, the language's signature forms ────────────────

test("pipelines scope the pipe and their stages", async () => {
  const src = "xs | map(fn(v) => v + 1) | where(fn(v) => v > 2)";
  const tokens = await tokenize(src);
  const pipes = tokens.filter((t) => t.text === "|");
  assert.equal(pipes.length, 2, "expected two pipe tokens");
  for (const p of pipes) {
    assert.ok(
      p.scopes.some((s) => s.startsWith("keyword.operator")),
      `pipe got ${JSON.stringify(p.scopes)}`,
    );
  }
  assert.ok(
    hasScope(await scopesOf(src, "map"), "support.function.builtin.pipeline"),
  );
  assert.ok(
    hasScope(await scopesOf(src, "=>"), "keyword.operator.arrow"),
  );
});

test("record keys are properties, not variables", async () => {
  const scopes = await scopesOf('let r = { name: "a" }', "name:");
  assert.ok(
    hasScope(scopes, "variable.other.property"),
    `record key got ${JSON.stringify(scopes)}`,
  );
});

// ── The embedded markdown grammar ────────────────────────────────────────

test("the markdown grammar injects aethershell into fenced blocks", async () => {
  const md = ["# Title", "", "```ae", 'print("x")', "```", ""].join("\n");
  const tokens = await tokenize(md, MARKDOWN_HOST);
  const hit = tokens.find((t) => t.text === "print");
  assert.ok(hit, "the fenced block was not tokenized at all");
  assert.ok(
    hit.scopes.some((s) => s.includes("aethershell")),
    `fenced AetherShell got ${JSON.stringify(hit.scopes)} — the injection selector \
does not match, so code blocks in READMEs render unhighlighted`,
  );
});
