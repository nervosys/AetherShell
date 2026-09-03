// The document providers -- symbols, folding, hover -- are the extension's
// largest module and had no tests at all. They are ordinary functions over
// document text, so they need the editor only for its data types.
//
// The hover provider additionally carries a hardcoded table of 63 builtin
// signatures. A table like that is the same risk as any other second copy of
// the truth: it drifts, and nothing notices. The last test here checks it
// against the shell's own source.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const require = createRequire(import.meta.url);
const h = require('./load-extension.cjs');
const here = path.dirname(fileURLToPath(import.meta.url));

const P = h.load('providers');
const SymbolKind = h.vscode.SymbolKind;

const SAMPLE = [
    '# header comment',
    '# still header',
    '# third line',
    '',
    'let double = fn(x) => x * 2',
    'let name = "hi"',
    'let nums = [1, 2, 3]',
    '',
    'fn helper() {',
    '  print(1)',
    '}',
].join('\n');

// ── symbols ──────────────────────────────────────────────────────────────

test('a lambda binding is a Function symbol carrying its parameters', () => {
    const syms = new P.AetherShellSymbolProvider().provideDocumentSymbols(h.doc(SAMPLE), h.token);
    const fn = syms.find((s) => s.name === 'double');
    assert.ok(fn, 'the lambda binding was not found');
    assert.equal(fn.kind, SymbolKind.Function);
    assert.equal(fn.detail, 'fn(x)', 'the parameter list should be the detail');
});

test('bindings are typed by their literal', () => {
    const syms = new P.AetherShellSymbolProvider().provideDocumentSymbols(h.doc(SAMPLE), h.token);
    assert.equal(syms.find((s) => s.name === 'name').kind, SymbolKind.String);
    assert.equal(syms.find((s) => s.name === 'nums').kind, SymbolKind.Array);
});

test('comments and blank lines produce no symbols', () => {
    const syms = new P.AetherShellSymbolProvider()
        .provideDocumentSymbols(h.doc('# a\n\n// b\n   \n'), h.token);
    assert.deepEqual(syms, []);
});

test('a symbol range covers its line and the selection covers just the name', () => {
    const syms = new P.AetherShellSymbolProvider().provideDocumentSymbols(h.doc(SAMPLE), h.token);
    const fn = syms.find((s) => s.name === 'double');
    assert.equal(fn.range.start.line, 4);
    assert.equal(fn.selectionRange.start.line, 4);
    const width = fn.selectionRange.end.character - fn.selectionRange.start.character;
    assert.equal(width, 'double'.length, 'the selection should cover exactly the name');
});

// ── folding ──────────────────────────────────────────────────────────────

test('a run of comment lines folds as a comment region', () => {
    const folds = new P.AetherShellFoldingProvider().provideFoldingRanges(h.doc(SAMPLE), {}, h.token);
    const comment = folds.find((f) => f.kind === h.vscode.FoldingRangeKind.Comment);
    assert.ok(comment, 'the three-line comment header did not fold');
    assert.equal(comment.start, 0);
    assert.equal(comment.end, 2);
});

test('a braced block folds', () => {
    const folds = new P.AetherShellFoldingProvider().provideFoldingRanges(h.doc(SAMPLE), {}, h.token);
    assert.ok(
        folds.some((f) => f.start === 8 && f.end === 10),
        `the fn body did not fold: ${JSON.stringify(folds)}`
    );
});

test('a lone comment line does not fold, but a pair does', () => {
    const comments = (text) =>
        new P.AetherShellFoldingProvider()
            .provideFoldingRanges(h.doc(text), {}, h.token)
            .filter((f) => f.kind === h.vscode.FoldingRangeKind.Comment);

    // One line: folding it would hide nothing.
    assert.equal(comments('# only\nlet x = 1\n').length, 0);

    // Two: folding hides the second, which is worth offering.
    const pair = comments('# one\n# two\nlet x = 1\n');
    assert.equal(pair.length, 1);
    assert.equal(pair[0].start, 0);
    assert.equal(pair[0].end, 1);
});

test('an unclosed brace does not produce a fold running past the file', () => {
    const folds = new P.AetherShellFoldingProvider()
        .provideFoldingRanges(h.doc('fn broken() {\n  print(1)\n'), {}, h.token);
    for (const f of folds) {
        assert.ok(f.end <= 1, `fold ends at ${f.end}, past the last line`);
    }
});

// ── hover ────────────────────────────────────────────────────────────────

test('hovering a keyword explains it', () => {
    const hover = new P.AetherShellHoverProvider()
        .provideHover(h.doc(SAMPLE), new h.vscode.Position(4, 13), h.token);
    assert.ok(hover, 'no hover on `fn`');
    assert.match(String(hover.contents[0].value), /fn/);
});

test('hovering a documented builtin gives a signature and an example', () => {
    const doc = h.doc('[1, 2, 3] | map(fn(x) => x * 2)');
    const hover = new P.AetherShellHoverProvider()
        .provideHover(doc, new h.vscode.Position(0, 13), h.token);
    assert.ok(hover, 'no hover on `map`');
    const text = String(hover.contents[0].value);
    assert.match(text, /map\(/, 'the signature should be shown');
    assert.match(text, /```/, 'an example block should be shown');
});

test('hovering ordinary text offers nothing rather than inventing something', () => {
    const doc = h.doc('let some_local_name = 1');
    const hover = new P.AetherShellHoverProvider()
        .provideHover(doc, new h.vscode.Position(0, 8), h.token);
    if (hover) {
        assert.doesNotMatch(
            String(hover.contents[0].value),
            /some_local_name\(/,
            'a local binding must not be described as a builtin'
        );
    }
});

// ── the hover table against the shell itself ─────────────────────────────

test('every builtin the hover table documents is one the shell dispatches', () => {
    const provider = new P.AetherShellHoverProvider();
    const table = Object.values(provider).find((v) => v instanceof Map);
    assert.ok(table, 'the builtin table was not found on the provider');
    assert.ok(table.size > 50, `only ${table.size} builtins documented; table looks empty`);

    // The dispatcher's own two halves, not "any quoted string in the source".
    //
    // The first version of this test asked whether a name appeared as a string
    // literal anywhere in 200k lines of Rust. That is a proxy for "is a
    // builtin", and it was too weak: `filter`, `skip`, `to_int`, `to_float`,
    // `which`, `os` and `arch` all passed it and all answer
    // E_UNKNOWN_BUILTIN at the prompt. Seven wrong entries shipped in a hover
    // tooltip because the check tested the wrong thing.
    //
    // BUILTIN_LOOKUP is a table of `map.insert("name", index)`. FALLBACK_BUILTINS
    // is a public const pairing each fallback match arm with its function, which
    // the shell's own tests/fallback_dispatch.rs keeps equal to the real match.
    // Together they are the set of names the shell answers to.
    const shellSrc = path.join(here, '..', '..', '..', 'src');
    const builtinsRs = readFileSync(path.join(shellSrc, 'builtins.rs'), 'utf8');
    const lookup = [...builtinsRs.matchAll(/map\.insert\("([^"]+)"/g)].map((m) => m[1]);
    const fallback = [...builtinsRs.matchAll(/\("([^"]+)",\s*"bi_[a-z_0-9]+"\)/g)].map(
        (m) => m[1]
    );

    // The dispatcher has a third half, and missing it is how a correct name
    // gets reported as invented: `call_with_input_inner` also routes to
    // `workflow_builtins::call`, whose names live in `workflows.rs`. Leaving it
    // out flagged workflow_pipeline and friends as unknown even though they run.
    const workflowsRs = readFileSync(path.join(shellSrc, 'workflows.rs'), 'utf8');
    const workflow = [
        ...workflowsRs.matchAll(/\(\s*"((?:workflow|circuit_breaker)_[a-z_]+)"/g),
    ].map((m) => m[1]);

    const dispatched = new Set([...lookup, ...fallback, ...workflow]);

    // Non-vacuity: if the parse breaks, the set collapses and everything passes.
    assert.ok(
        lookup.length > 1000,
        `only ${lookup.length} names read from BUILTIN_LOOKUP; the parse is broken`
    );
    assert.ok(
        fallback.length > 50,
        `only ${fallback.length} names read from FALLBACK_BUILTINS; the parse is broken`
    );
    assert.ok(
        workflow.length > 10,
        `only ${workflow.length} names read from workflow_builtins(); the parse is broken`
    );
    assert.ok(dispatched.has('map') && dispatched.has('where'), 'known builtins are absent');
    assert.ok(!dispatched.has('skip'), 'skip is not a builtin; the set is too permissive');

    const missing = [...table.keys()].filter((name) => !dispatched.has(name));
    assert.deepEqual(
        missing,
        [],
        `the hover table documents names the shell does not dispatch: ${missing.join(', ')}`
    );
});
