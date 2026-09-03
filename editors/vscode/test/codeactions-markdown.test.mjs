// Code actions and the markdown preview plugin, neither of which had tests.
//
// The markdown plugin is the one module in the extension that touches no editor
// API at all, so it can be exercised directly against markdown-it.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const h = require('./load-extension.cjs');

const CA = h.load('codeActions');
const MP = h.load('markdownPreview');

function actionsFor(text, range) {
    h.reset();
    const provider = new CA.AetherShellCodeActionProvider();
    return (
        provider.provideCodeActions(
            h.doc(text),
            range ?? new h.vscode.Range(0, 0, 0, text.split('\n')[0].length),
            { diagnostics: [] },
            h.token
        ) ?? []
    );
}

test('a selection offers refactors, and each one is titled and kinded', () => {
    const actions = actionsFor('let x = 1\nprint(x)\n');
    assert.ok(actions.length > 0, 'no code actions offered on a selection');
    for (const a of actions) {
        assert.ok(a.title && a.title.trim(), 'an action with no title is unusable in the menu');
        assert.ok(a.kind && a.kind.value, `"${a.title}" has no CodeActionKind`);
    }
});

test('extract-to-function is offered and is a refactor.extract', () => {
    const extract = actionsFor('let x = 1\nprint(x)\n').find((a) => /extract/i.test(a.title));
    assert.ok(extract, 'extract to function was not offered');
    assert.match(extract.kind.value, /^refactor\.extract/);
});

test('an action either edits the document or runs a command, never neither', () => {
    for (const a of actionsFor('let x = 1\nprint(x)\n')) {
        assert.ok(
            a.edit || a.command,
            `"${a.title}" would appear in the lightbulb and then do nothing`
        );
    }
});

test('action titles are unique, so the menu cannot show two identical entries', () => {
    const titles = actionsFor('let x = 1\nprint(x)\n').map((a) => a.title);
    assert.equal(new Set(titles).size, titles.length, `duplicate titles: ${titles.join(' | ')}`);
});

test('an empty document does not throw', () => {
    assert.doesNotThrow(() => actionsFor('', new h.vscode.Range(0, 0, 0, 0)));
});

// ── markdown preview ─────────────────────────────────────────────────────

test('the markdown plugin exposes the hook VS Code calls', () => {
    const api = MP.activate();
    assert.equal(typeof api.extendMarkdownIt, 'function');
});

test('the plugin returns the markdown-it instance it was given', () => {
    const md = require('markdown-it')();
    const returned = MP.activate().extendMarkdownIt(md);
    assert.equal(returned, md, 'VS Code uses the return value; dropping it disables the plugin');
});

test('an aethershell fence renders without throwing and keeps its content', () => {
    const md = MP.activate().extendMarkdownIt(require('markdown-it')());
    const html = md.render('```aethershell\nlet x = 1 | map(fn(y) => y)\n```\n');
    assert.match(html, /let/, 'the code content vanished from the rendered output');
    assert.ok(html.length > 20, 'the fence rendered to almost nothing');
});

test('a fence in another language is left alone', () => {
    const md = MP.activate().extendMarkdownIt(require('markdown-it')());
    const html = md.render('```python\nprint(1)\n```\n');
    assert.match(html, /print\(1\)/);
});

test('ordinary markdown still renders after the plugin is installed', () => {
    const md = MP.activate().extendMarkdownIt(require('markdown-it')());
    const html = md.render('# Title\n\nSome **bold** text.\n');
    assert.match(html, /<h1/);
    assert.match(html, /<strong>bold<\/strong>/);
});
