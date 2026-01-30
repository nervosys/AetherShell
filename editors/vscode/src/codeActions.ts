import * as vscode from 'vscode';

/**
 * AetherShell Code Action Provider
 * Provides quick fixes, refactorings, and AI-powered actions
 */
export class AetherShellCodeActionProvider implements vscode.CodeActionProvider {
    static readonly providedCodeActionKinds = [
        vscode.CodeActionKind.QuickFix,
        vscode.CodeActionKind.Refactor,
        vscode.CodeActionKind.Source
    ];

    provideCodeActions(
        document: vscode.TextDocument,
        range: vscode.Range | vscode.Selection,
        _context: vscode.CodeActionContext,
        _token: vscode.CancellationToken
    ): vscode.CodeAction[] {
        const actions: vscode.CodeAction[] = [];
        const line = document.lineAt(range.start.line);
        const lineText = line.text;

        // Convert function to pipeline
        const fnCallMatch = lineText.match(/(\w+)\((.+)\)/);
        if (fnCallMatch) {
            const funcName = fnCallMatch[1];
            const args = fnCallMatch[2];

            // Check if this could be a pipeline operation
            const pipelineFuncs = ['map', 'filter', 'reduce', 'sort', 'take', 'skip', 'where', 'select'];
            if (pipelineFuncs.includes(funcName)) {
                const action = new vscode.CodeAction(
                    `Convert to pipeline: data | ${funcName}(...)`,
                    vscode.CodeActionKind.Refactor
                );
                action.command = {
                    command: 'aethershell.convertToPipeline',
                    title: 'Convert to Pipeline',
                    arguments: [document, range, funcName, args]
                };
                actions.push(action);
            }
        }

        // Extract to function
        if (!range.isEmpty) {
            const action = new vscode.CodeAction(
                'Extract to function',
                vscode.CodeActionKind.RefactorExtract
            );
            action.command = {
                command: 'aethershell.extractFunction',
                title: 'Extract to Function',
                arguments: [document, range]
            };
            actions.push(action);
        }

        // Wrap in try-catch
        if (!range.isEmpty) {
            const selectedText = document.getText(range);
            if (selectedText.includes('ai(') || selectedText.includes('http_') || selectedText.includes('read(')) {
                const action = new vscode.CodeAction(
                    'Wrap with try-catch',
                    vscode.CodeActionKind.Refactor
                );
                action.command = {
                    command: 'aethershell.wrapWithTryCatch',
                    title: 'Wrap with try-catch',
                    arguments: [document, range]
                };
                actions.push(action);
            }
        }

        // Add type annotation
        const letMatch = lineText.match(/^\s*let\s+(\w+)\s*=\s*(.+)/);
        if (letMatch && !lineText.includes(':')) {
            const action = new vscode.CodeAction(
                'Add type annotation',
                vscode.CodeActionKind.Refactor
            );
            action.command = {
                command: 'aethershell.addTypeAnnotation',
                title: 'Add Type Annotation',
                arguments: [document, range.start.line]
            };
            actions.push(action);
        }

        // Convert string concatenation to format
        if (lineText.includes('" + ') && lineText.includes(' + "')) {
            const action = new vscode.CodeAction(
                'Convert to format() call',
                vscode.CodeActionKind.Refactor
            );
            action.command = {
                command: 'aethershell.convertToFormat',
                title: 'Convert to format()',
                arguments: [document, range.start.line]
            };
            actions.push(action);
        }

        // AI-powered suggestions (if AI features enabled)
        const config = vscode.workspace.getConfiguration('aethershell');
        if (config.get<boolean>('ai.enabled', true)) {
            const aiAction = new vscode.CodeAction(
                '✨ Ask AI to improve this code',
                vscode.CodeActionKind.Refactor
            );
            aiAction.command = {
                command: 'aethershell.aiImprove',
                title: 'AI Improve',
                arguments: [document, range]
            };
            actions.push(aiAction);

            // AI documentation
            if (letMatch) {
                const docAction = new vscode.CodeAction(
                    '📝 Generate AI documentation',
                    vscode.CodeActionKind.Source
                );
                docAction.command = {
                    command: 'aethershell.aiDocument',
                    title: 'AI Document',
                    arguments: [document, range]
                };
                actions.push(docAction);
            }
        }

        return actions;
    }
}

/**
 * Register code action commands
 */
export function registerCodeActionCommands(context: vscode.ExtensionContext): void {
    // Extract to function
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.extractFunction', async (
            document: vscode.TextDocument,
            range: vscode.Range
        ) => {
            const selectedText = document.getText(range);
            const funcName = await vscode.window.showInputBox({
                prompt: 'Enter function name',
                placeHolder: 'my_function'
            });

            if (!funcName) {
                return;
            }

            // Analyze selected code for parameters
            const identifiers = selectedText.match(/\b[a-z_][a-z0-9_]*\b/gi) || [];
            const uniqueIds = [...new Set(identifiers)];

            // Filter out keywords and builtins
            const keywords = ['let', 'if', 'else', 'match', 'fn', 'true', 'false', 'null', 'for', 'while', 'in'];
            const params = uniqueIds.filter(id => !keywords.includes(id));

            const edit = new vscode.WorkspaceEdit();

            // Create function definition
            const paramList = params.slice(0, 3).join(', '); // Limit to 3 params
            const funcDef = `let ${funcName} = fn(${paramList}) => {\n    ${selectedText}\n}\n\n`;

            // Insert function before the current code
            edit.insert(document.uri, new vscode.Position(0, 0), funcDef);

            // Replace selection with function call
            edit.replace(document.uri, range, `${funcName}(${paramList})`);

            await vscode.workspace.applyEdit(edit);
        })
    );

    // Wrap with try-catch
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.wrapWithTryCatch', async (
            document: vscode.TextDocument,
            range: vscode.Range
        ) => {
            const selectedText = document.getText(range);
            const indentation = document.lineAt(range.start.line).text.match(/^\s*/)?.[0] || '';

            const wrapped = `try {\n${indentation}    ${selectedText}\n${indentation}} catch err {\n${indentation}    print("Error: " + err.message)\n${indentation}    null\n${indentation}}`;

            const edit = new vscode.WorkspaceEdit();
            edit.replace(document.uri, range, wrapped);
            await vscode.workspace.applyEdit(edit);
        })
    );

    // Add type annotation
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.addTypeAnnotation', async (
            document: vscode.TextDocument,
            lineNumber: number
        ) => {
            const line = document.lineAt(lineNumber);
            const match = line.text.match(/^(\s*let\s+)(\w+)(\s*=\s*)(.+)/);

            if (!match) {
                return;
            }

            const [, prefix, name, equals, value] = match;
            let type = inferType(value);

            const userType = await vscode.window.showInputBox({
                prompt: 'Enter type annotation',
                value: type,
                placeHolder: 'Int, String, Array[Int], etc.'
            });

            if (!userType) {
                return;
            }

            const newLine = `${prefix}${name}: ${userType}${equals}${value}`;
            const edit = new vscode.WorkspaceEdit();
            edit.replace(document.uri, line.range, newLine);
            await vscode.workspace.applyEdit(edit);
        })
    );

    // Convert to format
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.convertToFormat', async (
            document: vscode.TextDocument,
            lineNumber: number
        ) => {
            const line = document.lineAt(lineNumber);
            // Simple implementation - could be enhanced
            const text = line.text;

            // This is a simplified conversion
            vscode.window.showInformationMessage('Format conversion would replace: ' + text);
        })
    );

    // Convert to pipeline
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.convertToPipeline', async (
            document: vscode.TextDocument,
            range: vscode.Range,
            funcName: string,
            args: string
        ) => {
            // Split args to find the data source
            const argParts = args.split(',').map(a => a.trim());
            if (argParts.length >= 1) {
                const dataArg = argParts[0];
                const restArgs = argParts.slice(1).join(', ');

                const pipelineCode = restArgs
                    ? `${dataArg} | ${funcName}(${restArgs})`
                    : `${dataArg} | ${funcName}()`;

                const edit = new vscode.WorkspaceEdit();
                const lineRange = document.lineAt(range.start.line).range;

                // Replace just the function call portion
                const line = document.lineAt(range.start.line).text;
                const newLine = line.replace(`${funcName}(${args})`, pipelineCode);
                edit.replace(document.uri, lineRange, newLine);

                await vscode.workspace.applyEdit(edit);
            }
        })
    );

    // AI improve
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.aiImprove', async (
            document: vscode.TextDocument,
            range: vscode.Range
        ) => {
            const selectedText = document.getText(range);

            vscode.window.withProgress({
                location: vscode.ProgressLocation.Notification,
                title: 'AI analyzing code...',
                cancellable: false
            }, async () => {
                // In a real implementation, this would call the AI
                vscode.window.showInformationMessage(
                    'AI improvement would analyze: ' + selectedText.substring(0, 50) + '...'
                );
            });
        })
    );

    // AI document
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.aiDocument', async (
            document: vscode.TextDocument,
            range: vscode.Range
        ) => {
            const selectedText = document.getText(range);

            vscode.window.withProgress({
                location: vscode.ProgressLocation.Notification,
                title: 'Generating documentation...',
                cancellable: false
            }, async () => {
                // In a real implementation, this would call the AI
                vscode.window.showInformationMessage(
                    'AI would generate docs for: ' + selectedText.substring(0, 50) + '...'
                );
            });
        })
    );
}

/**
 * Infer type from a value string
 */
function inferType(value: string): string {
    value = value.trim();

    if (value.startsWith('"') || value.startsWith("'")) {
        return 'String';
    }
    if (value === 'true' || value === 'false') {
        return 'Bool';
    }
    if (/^\d+$/.test(value)) {
        return 'Int';
    }
    if (/^\d+\.\d+$/.test(value)) {
        return 'Float';
    }
    if (value.startsWith('[')) {
        return 'Array[_]';
    }
    if (value.startsWith('{')) {
        return 'Record';
    }
    if (value.startsWith('fn(')) {
        return 'Function';
    }

    return '_';
}
