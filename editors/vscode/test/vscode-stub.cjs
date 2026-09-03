'use strict';

// A stand-in for the `vscode` module, which only exists inside the editor host.
//
// The extension's providers are ordinary functions that read a document and
// return structured results -- symbols, folding ranges, hovers, code actions.
// That logic is worth testing and needs no editor: it needs `Range`,
// `DocumentSymbol` and a handful of enums. Everything here is the smallest
// shape those call sites actually use, and each constructor keeps its arguments
// so a test can assert on them.
//
// Anything the extension calls that would reach the running editor -- showing a
// message, opening a terminal -- is recorded rather than performed, so a test
// can assert that it *would* have happened.

class Position {
    constructor(line, character) {
        this.line = line;
        this.character = character;
    }
}

class Range {
    constructor(startLine, startChar, endLine, endChar) {
        // Both call shapes are used: (Position, Position) and four numbers.
        if (startLine instanceof Position) {
            this.start = startLine;
            this.end = startChar;
        } else {
            this.start = new Position(startLine, startChar);
            this.end = new Position(endLine, endChar);
        }
    }
}

class Selection extends Range {}

class DocumentSymbol {
    constructor(name, detail, kind, range, selectionRange) {
        this.name = name;
        this.detail = detail;
        this.kind = kind;
        this.range = range;
        this.selectionRange = selectionRange;
        this.children = [];
    }
}

class FoldingRange {
    constructor(start, end, kind) {
        this.start = start;
        this.end = end;
        this.kind = kind;
    }
}

class MarkdownString {
    constructor(value = '') {
        this.value = value;
        this.isTrusted = false;
        this.supportHtml = false;
    }
    appendMarkdown(v) {
        this.value += v;
        return this;
    }
    appendCodeblock(code, lang = '') {
        this.value += '\n```' + lang + '\n' + code + '\n```\n';
        return this;
    }
    appendText(v) {
        this.value += v;
        return this;
    }
}

class Hover {
    constructor(contents, range) {
        this.contents = Array.isArray(contents) ? contents : [contents];
        this.range = range;
    }
}

class CodeActionKind {
    constructor(value) {
        this.value = value;
    }
    append(part) {
        return new CodeActionKind(`${this.value}.${part}`);
    }
}
CodeActionKind.QuickFix = new CodeActionKind('quickfix');
CodeActionKind.Refactor = new CodeActionKind('refactor');
CodeActionKind.RefactorExtract = new CodeActionKind('refactor.extract');
CodeActionKind.RefactorRewrite = new CodeActionKind('refactor.rewrite');
CodeActionKind.Source = new CodeActionKind('source');

class CodeAction {
    constructor(title, kind) {
        this.title = title;
        this.kind = kind;
        this.edit = undefined;
        this.command = undefined;
        this.isPreferred = false;
        this.diagnostics = [];
    }
}

class WorkspaceEdit {
    constructor() {
        this.edits = [];
    }
    replace(uri, range, newText) {
        this.edits.push({ op: 'replace', uri, range, newText });
    }
    insert(uri, position, newText) {
        this.edits.push({ op: 'insert', uri, position, newText });
    }
    delete(uri, range) {
        this.edits.push({ op: 'delete', uri, range });
    }
}

const SymbolKind = {
    File: 0, Module: 1, Namespace: 2, Package: 3, Class: 4, Method: 5,
    Property: 6, Field: 7, Constructor: 8, Enum: 9, Interface: 10,
    Function: 11, Variable: 12, Constant: 13, String: 14, Number: 15,
    Boolean: 16, Array: 17, Object: 18, Key: 19, Null: 20,
};

const FoldingRangeKind = { Comment: 1, Imports: 2, Region: 3 };
const ViewColumn = { Active: -1, Beside: -2, One: 1, Two: 2 };
const ProgressLocation = { SourceControl: 1, Window: 10, Notification: 15 };

// Everything the extension would do to a live editor is recorded here instead.
const recorded = {
    info: [], warning: [], error: [],
    terminals: [], outputChannels: [], commands: [], executed: [],
    registrations: [],
};

function reset() {
    for (const k of Object.keys(recorded)) recorded[k].length = 0;
    config.clear();
}

// Workspace configuration, settable per test.
const config = new Map();

const vscode = {
    Position, Range, Selection, DocumentSymbol, FoldingRange, FoldingRangeKind,
    MarkdownString, Hover, CodeAction, CodeActionKind, WorkspaceEdit,
    SymbolKind, ViewColumn, ProgressLocation,

    window: {
        showInformationMessage: (m) => { recorded.info.push(m); return Promise.resolve(undefined); },
        showWarningMessage: (m) => { recorded.warning.push(m); return Promise.resolve(undefined); },
        showErrorMessage: (m) => { recorded.error.push(m); return Promise.resolve(undefined); },
        createTerminal: (opts) => {
            const o = (opts && typeof opts === 'object') ? opts : { name: String(opts) };
            const t = { name: o.name, shellPath: o.shellPath, shellArgs: o.shellArgs,
                        sent: [], shown: 0,
                        sendText(x) { this.sent.push(x); }, show() { this.shown++; },
                        dispose() { this.disposed = true; } };
            recorded.terminals.push(t);
            return t;
        },
        createOutputChannel: (name) => {
            const c = { name, lines: [], appendLine(l) { this.lines.push(l); },
                        append(l) { this.lines.push(l); }, show() {}, clear() { this.lines.length = 0; },
                        dispose() {} };
            recorded.outputChannels.push(c);
            return c;
        },
        withProgress: (_opts, task) => task({ report: () => {} }, { isCancellationRequested: false }),
        activeTextEditor: undefined,
        registerWebviewViewProvider: (id, provider) => {
            recorded.registrations.push({ kind: 'webview', id, provider });
            return { dispose() {} };
        },
    },

    workspace: {
        getConfiguration: (section) => ({
            get: (key, dflt) => (config.has(`${section}.${key}`) ? config.get(`${section}.${key}`) : dflt),
            update: (key, value) => { config.set(`${section}.${key}`, value); return Promise.resolve(); },
        }),
        applyEdit: (edit) => { recorded.executed.push({ kind: 'applyEdit', edit }); return Promise.resolve(true); },
        openTextDocument: (o) => Promise.resolve(o),
        workspaceFolders: undefined,
    },

    commands: {
        registerCommand: (id, handler) => { recorded.commands.push({ id, handler }); return { dispose() {} }; },
        executeCommand: (id, ...args) => { recorded.executed.push({ kind: 'command', id, args }); return Promise.resolve(undefined); },
    },

    languages: {
        registerDocumentSymbolProvider: (sel, p) => { recorded.registrations.push({ kind: 'symbol', sel, p }); return { dispose() {} }; },
        registerFoldingRangeProvider: (sel, p) => { recorded.registrations.push({ kind: 'folding', sel, p }); return { dispose() {} }; },
        registerHoverProvider: (sel, p) => { recorded.registrations.push({ kind: 'hover', sel, p }); return { dispose() {} }; },
        registerCodeActionsProvider: (sel, p, meta) => { recorded.registrations.push({ kind: 'codeAction', sel, p, meta }); return { dispose() {} }; },
        createDiagnosticCollection: (name) => ({ name, set() {}, clear() {}, dispose() {} }),
    },

    Uri: { file: (p) => ({ fsPath: p, scheme: 'file', toString: () => `file://${p}` }) },

    __test: { recorded, config, reset },
};

module.exports = vscode;
