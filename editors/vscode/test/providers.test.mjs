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

test('every builtin the hover table documents exists in the shell', () => {
    const provider = new P.AetherShellHoverProvider();
    const table = Object.values(provider).find((v) => v instanceof Map);
    assert.ok(table, 'the builtin table was not found on the provider');
    assert.ok(table.size > 50, `only ${table.size} builtins documented; table looks empty`);

    // Every string literal in the shell's Rust sources. A builtin is dispatched
    // by name, so a name the sources never quote is one the shell cannot serve.
    const shellSrc = path.join(here, '..', '..', '..', 'src');
    let text = '';
    (function walk(dir) {
        for (const entry of readdirSync(dir)) {
            const p = path.join(dir, entry);
            if (statSync(p).isDirectory()) walk(p);
            else if (p.endsWith('.rs')) text += readFileSync(p, 'utf8');
        }
    })(shellSrc);
    const quoted = new Set([...text.matchAll(/"([a-z_][a-z0-9_]*)"/g)].map((m) => m[1]));
    assert.ok(quoted.size > 500, `only ${quoted.size} names read from the shell source`);

    const missing = [...table.keys()].filter((name) => !quoted.has(name));
    assert.deepEqual(
        missing,
        [],
        `the hover table documents builtins the shell does not have: ${missing.join(', ')}`
    );
});
