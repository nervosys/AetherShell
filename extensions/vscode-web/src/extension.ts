import * as vscode from 'vscode';

let agentApiUrl: string;

export function activate(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('aethershell');
    agentApiUrl = config.get('agentApiUrl', 'http://localhost:3002');

    // Register eval command
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.eval', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) return;

            const selection = editor.selection;
            const text = selection.isEmpty
                ? editor.document.lineAt(selection.active.line).text
                : editor.document.getText(selection);

            try {
                const result = await evalCode(text);
                const outputChannel = vscode.window.createOutputChannel('AetherShell');
                outputChannel.appendLine(`> ${text}`);
                outputChannel.appendLine(JSON.stringify(result, null, 2));
                outputChannel.show();
            } catch (err: any) {
                vscode.window.showErrorMessage(`AetherShell: ${err.message}`);
            }
        })
    );

    // Register connect command
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.connectAgent', async () => {
            const url = await vscode.window.showInputBox({
                prompt: 'AetherShell Agent API URL',
                value: agentApiUrl,
            });
            if (url) {
                agentApiUrl = url;
                try {
                    const resp = await fetch(`${agentApiUrl}/health`);
                    if (resp.ok) {
                        vscode.window.showInformationMessage(`Connected to AetherShell at ${agentApiUrl}`);
                    } else {
                        vscode.window.showWarningMessage(`AetherShell health check failed: ${resp.status}`);
                    }
                } catch {
                    vscode.window.showErrorMessage(`Cannot reach AetherShell at ${agentApiUrl}`);
                }
            }
        })
    );

    // Hover provider for builtins
    context.subscriptions.push(
        vscode.languages.registerHoverProvider('aethershell', {
            async provideHover(document, position) {
                const range = document.getWordRangeAtPosition(position, /[\w.]+/);
                if (!range) return;
                const word = document.getText(range);

                try {
                    const resp = await fetch(`${agentApiUrl}/api/v1/builtins/${encodeURIComponent(word)}`);
                    if (resp.ok) {
                        const info: any = await resp.json();
                        const md = new vscode.MarkdownString();
                        md.appendCodeblock(info.signature || word, 'aethershell');
                        if (info.description) md.appendMarkdown(`\n\n${info.description}`);
                        return new vscode.Hover(md);
                    }
                } catch {
                    // Agent API not available — no hover
                }
                return undefined;
            }
        })
    );

    // Completion provider
    context.subscriptions.push(
        vscode.languages.registerCompletionItemProvider('aethershell', {
            async provideCompletionItems(document, position) {
                const linePrefix = document.lineAt(position).text.substring(0, position.character);

                try {
                    const resp = await fetch(`${agentApiUrl}/api/v1/builtins`);
                    if (!resp.ok) return [];
                    const builtins: any = await resp.json();
                    const items: vscode.CompletionItem[] = (builtins.builtins || []).map((b: any) => {
                        const item = new vscode.CompletionItem(b.name, vscode.CompletionItemKind.Function);
                        item.detail = b.description || '';
                        item.insertText = b.name.includes('.') ? b.name : `${b.name}()`;
                        return item;
                    });
                    return items;
                } catch {
                    return [];
                }
            }
        }, '.')
    );
}

async function evalCode(code: string): Promise<any> {
    const resp = await fetch(`${agentApiUrl}/api/v1/eval`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ code }),
    });
    if (!resp.ok) {
        const text = await resp.text();
        throw new Error(`Eval failed: ${text}`);
    }
    return resp.json();
}

export function deactivate() {}
