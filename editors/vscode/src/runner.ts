import * as vscode from 'vscode';

/**
 * AetherShell Code Runner
 * Runs AetherShell code in an integrated terminal or captures output
 */
export class AetherShellRunner {
    private terminal: vscode.Terminal | undefined;
    private outputChannel: vscode.OutputChannel;

    constructor(outputChannel: vscode.OutputChannel) {
        this.outputChannel = outputChannel;
    }

    /**
     * Get or create the AetherShell terminal
     */
    private getTerminal(): vscode.Terminal {
        // Check if terminal is still alive
        if (this.terminal && !this.terminal.exitStatus) {
            return this.terminal;
        }

        // Create new terminal
        this.terminal = vscode.window.createTerminal({
            name: 'AetherShell',
            shellPath: this.getAetherShellPath(),
            shellArgs: [],
            env: {
                AETHER_SHELL_MODE: 'vscode'
            }
        });

        return this.terminal;
    }

    /**
     * Find the AetherShell executable path
     */
    private getAetherShellPath(): string {
        const config = vscode.workspace.getConfiguration('aethershell');
        const customPath = config.get<string>('executable.path', '');

        if (customPath) {
            return customPath;
        }

        // Default to 'ae' in PATH
        return process.platform === 'win32' ? 'ae.exe' : 'ae';
    }

    /**
     * Run the current file
     */
    async runFile(document: vscode.TextDocument): Promise<void> {
        if (document.languageId !== 'aethershell') {
            vscode.window.showWarningMessage('Not an AetherShell file');
            return;
        }

        // Save the document first
        if (document.isDirty) {
            await document.save();
        }

        const terminal = this.getTerminal();
        terminal.show();
        terminal.sendText(`# Running: ${document.fileName}`);

        const aePath = this.getAetherShellPath();
        terminal.sendText(`${aePath} "${document.fileName}"`);
    }

    /**
     * Run selected text or current line
     */
    async runSelection(editor: vscode.TextEditor): Promise<void> {
        const document = editor.document;
        if (document.languageId !== 'aethershell') {
            vscode.window.showWarningMessage('Not an AetherShell file');
            return;
        }

        let code: string;
        if (editor.selection.isEmpty) {
            // Run current line
            const line = document.lineAt(editor.selection.active.line);
            code = line.text;
        } else {
            // Run selection
            code = document.getText(editor.selection);
        }

        if (!code.trim()) {
            return;
        }

        const terminal = this.getTerminal();
        terminal.show();

        // For multi-line code, use -e flag
        if (code.includes('\n')) {
            // Write to temp file and run
            const escapedCode = code.replace(/"/g, '\\"');
            terminal.sendText(`${this.getAetherShellPath()} -e "${escapedCode}"`);
        } else {
            terminal.sendText(code);
        }
    }

    /**
     * Start an interactive REPL session
     */
    startRepl(): void {
        const terminal = this.getTerminal();
        terminal.show();
    }

    /**
     * Start TUI mode
     */
    startTui(): void {
        const terminal = vscode.window.createTerminal({
            name: 'AetherShell TUI',
            shellPath: this.getAetherShellPath(),
            shellArgs: ['--tui'],
        });
        terminal.show();
    }

    /**
     * Dispose terminal
     */
    dispose(): void {
        if (this.terminal) {
            this.terminal.dispose();
            this.terminal = undefined;
        }
    }
}

/**
 * Register code runner commands
 */
export function registerRunnerCommands(
    context: vscode.ExtensionContext,
    outputChannel: vscode.OutputChannel
): AetherShellRunner {
    const runner = new AetherShellRunner(outputChannel);

    // Run current file
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.runFile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (editor) {
                await runner.runFile(editor.document);
            }
        })
    );

    // Run selection or line
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.runSelection', async () => {
            const editor = vscode.window.activeTextEditor;
            if (editor) {
                await runner.runSelection(editor);
            }
        })
    );

    // Start REPL
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.startRepl', () => {
            runner.startRepl();
        })
    );

    // Start TUI
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.startTui', () => {
            runner.startTui();
        })
    );

    return runner;
}
