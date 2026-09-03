'use strict';

// Load a compiled extension module with `vscode` swapped for the stub.
//
// The extension is written against an API that only exists inside the editor
// host, so `require('vscode')` fails outside it. Rather than restructure the
// source to make it testable -- which would change the thing under test -- this
// intercepts the one module that cannot resolve and leaves everything else
// alone.

const Module = require('node:module');
const path = require('node:path');

const stub = require('./vscode-stub.cjs');

const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
    if (request === 'vscode') {
        return stub;
    }
    return originalLoad.call(this, request, parent, isMain);
};

const OUT = path.join(__dirname, '..', 'out');

/** Require a compiled module from `out/`, with the stub in place. */
function load(name) {
    const p = path.join(OUT, `${name}.js`);
    delete require.cache[require.resolve(p)];
    return require(p);
}

/** A minimal stand-in for `vscode.TextDocument`. */
function doc(text, { fileName = '/w/test.ae', languageId = 'aethershell' } = {}) {
    const lines = text.split('\n');
    return {
        fileName,
        languageId,
        uri: stub.Uri.file(fileName),
        lineCount: lines.length,
        getText(range) {
            if (!range) return text;
            if (range.start.line === range.end.line) {
                return lines[range.start.line].slice(range.start.character, range.end.character);
            }
            return lines.slice(range.start.line, range.end.line + 1).join('\n');
        },
        lineAt(i) {
            const n = typeof i === 'number' ? i : i.line;
            const t = lines[n] ?? '';
            return {
                text: t,
                lineNumber: n,
                range: new stub.Range(n, 0, n, t.length),
                firstNonWhitespaceCharacterIndex: t.length - t.trimStart().length,
                isEmptyOrWhitespace: t.trim() === '',
            };
        },
        getWordRangeAtPosition(pos, re = /[A-Za-z_][A-Za-z0-9_]*/g) {
            const t = lines[pos.line] ?? '';
            for (const m of t.matchAll(re)) {
                const s = m.index;
                const e = s + m[0].length;
                if (pos.character >= s && pos.character <= e) {
                    return new stub.Range(pos.line, s, pos.line, e);
                }
            }
            return undefined;
        },
        positionAt(offset) {
            let remaining = offset;
            for (let i = 0; i < lines.length; i++) {
                if (remaining <= lines[i].length) return new stub.Position(i, remaining);
                remaining -= lines[i].length + 1;
            }
            return new stub.Position(lines.length - 1, 0);
        },
    };
}

const token = { isCancellationRequested: false, onCancellationRequested: () => ({ dispose() {} }) };

module.exports = { load, doc, token, vscode: stub, reset: stub.__test.reset };
