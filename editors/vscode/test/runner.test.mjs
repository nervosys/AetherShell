// The runner builds command lines for a real program, and nothing checked that
// the flags it used existed. Two did not:
//
//   Run Selection  ->  ae -e "<code>"   error: unexpected argument '-e' found
//   Open TUI       ->  ae --tui         error: unexpected argument '--tui' found
//
// Both shipped that way. An extension command that fails inside the terminal
// looks like a broken install rather than a broken extension, which is why it
// went unreported.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const require = createRequire(import.meta.url);
const h = require('./load-extension.cjs');
const here = path.dirname(fileURLToPath(import.meta.url));

const { AetherShellRunner } = h.load('runner');

function freshRunner() {
    h.reset();
    return new AetherShellRunner(h.vscode.window.createOutputChannel('test'));
}

function lastTerminal() {
    const t = h.vscode.__test.recorded.terminals;
    return t[t.length - 1];
}

/** An editor over `text`, with the whole document selected. */
function editorWithSelection(text) {
    const document = h.doc(text);
    const lines = text.split('\n');
    const selection = {
        isEmpty: false,
        active: { line: 0, character: 0 },
        start: { line: 0, character: 0 },
        end: { line: lines.length - 1, character: lines[lines.length - 1].length },
    };
    document.getText = (range) => (range ? text : text);
    return { document, selection };
}

test('a multi-line selection is run with the flag the CLI actually has', async () => {
    const runner = freshRunner();
    await runner.runSelection(editorWithSelection('let a = 1\nlet b = 2'));

    const sent = lastTerminal().sent.join('\n');
    assert.match(sent, /\s-c\s/, `expected -c, got: ${sent}`);
    assert.doesNotMatch(sent, /\s-e\s/, 'ae does not accept -e');
});

test('a single-line selection is sent as-is', async () => {
    const runner = freshRunner();
    const editor = editorWithSelection('1 + 2');
    await runner.runSelection(editor);

    assert.equal(lastTerminal().sent.join('\n').trim(), '1 + 2');
});

test('a non-AetherShell document is refused rather than run', async () => {
    const runner = freshRunner();
    const editor = editorWithSelection('print("x")');
    editor.document.languageId = 'python';

    await runner.runSelection(editor);

    assert.equal(
        h.vscode.__test.recorded.warning.length,
        1,
        'the user should be told, not silently ignored'
    );
    assert.equal(
        h.vscode.__test.recorded.terminals.length,
        0,
        'nothing should be run for a document of another language'
    );
});

test('the TUI is launched by subcommand, not by a flag', () => {
    const runner = freshRunner();
    runner.startTui();

    const term = lastTerminal();
    assert.deepEqual(term.shellArgs, ['tui'], `got ${JSON.stringify(term.shellArgs)}`);
    assert.notDeepEqual(term.shellArgs, ['--tui'], 'ae rejects --tui');
});

test('running a file quotes the path, so a space does not split it', async () => {
    const runner = freshRunner();
    await runner.runFile(h.doc('let x = 1', { fileName: 'C:/my projects/a.ae' }));

    const sent = lastTerminal().sent.join('\n');
    assert.match(sent, /"C:\/my projects\/a\.ae"/, `unquoted path: ${sent}`);
});

test('the executable path is configurable, defaulting to ae on PATH', () => {
    const runner = freshRunner();
    runner.startTui();
    // The default is platform-dependent: ae.exe on Windows, ae elsewhere.
    const bare = process.platform === 'win32' ? 'ae.exe' : 'ae';
    assert.equal(lastTerminal().shellPath, bare);

    h.reset();
    h.vscode.__test.config.set('aethershell.executable.path', 'C:/custom/ae.exe');
    const configured = new AetherShellRunner(h.vscode.window.createOutputChannel('t'));
    configured.startTui();
    assert.equal(lastTerminal().shellPath, 'C:/custom/ae.exe');
});

test('no source file invokes ae with an interface it does not have', () => {
    // Read the source, so a failure names the file a maintainer would edit --
    // with comments stripped first. Without that this fires on the comment
    // documenting the bug, which quotes the wrong flag in order to explain it.
    // The same prose-versus-code distinction the documentation ratchets in this
    // repository already had to learn.
    const stripComments = (s) =>
        s.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
    const src = ['runner.ts', 'extension.ts', 'agentPanel.ts']
        .map((f) => stripComments(readFileSync(path.join(here, '..', 'src', f), 'utf8')))
        .join('\n');

    for (const bad of ['-e "', "'--tui'", '"--tui"', '--eval', '--exec']) {
        assert.ok(!src.includes(bad), `uses ${bad}, which ae does not accept`);
    }
    assert.match(src, /-c "/, 'running code should use -c');
    assert.match(src, /'tui'/, 'the TUI should be launched by subcommand');
});
