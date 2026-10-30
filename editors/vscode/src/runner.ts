import * as vscode from 'vscode';

/**
 * IronShell Code Runner
 * Runs IronShell code in an integrated terminal or captures output
 */
export class IronShellRunner {
    private terminal: vscode.Terminal | undefined;
    private outputChannel: vscode.OutputChannel;

    constructor(outputChannel: vscode.OutputChannel) {
        this.outputChannel = outputChannel;
    }

    /**
     * Get or create the IronShell terminal
     */
    private getTerminal(): vscode.Terminal {
        // Check if terminal is still alive
        if (this.terminal && !this.terminal.exitStatus) {
            return this.terminal;
        }

        // Create new terminal
        this.terminal = vscode.window.createTerminal({
            name: 'IronShell',
            shellPath: this.getIronShellPath(),
            shellArgs: [],
            env: {
                IRONSHELL_SHELL_MODE: 'vscode'
            }
        });

        return this.terminal;
    }

    /**
     * Find the IronShell executable path
     */
    private getIronShellPath(): string {
        const config = vscode.workspace.getConfiguration('ironshell');
        const customPath = config.get<string>('executable.path', '');

        if (customPath) {
            return customPath;
        }

        // Default to 'ish' in PATH
        return process.platform === 'win32' ? 'ish.exe' : 'ish';
    }

    /**
     * Run the current file
     */
    async runFile(document: vscode.TextDocument): Promise<void> {
        if (document.languageId !== 'ironshell') {
            vscode.window.showWarningMessage('Not an IronShell file');
            return;
        }

        // Save the document first
        if (document.isDirty) {
            await document.save();
        }

        const terminal = this.getTerminal();
        terminal.show();
        terminal.sendText(`# Running: ${document.fileName}`);

        const aePath = this.getIronShellPath();
        terminal.sendText(`${aePath} "${document.fileName}"`);
    }

    /**
     * Run selected text or current line
     */
    async runSelection(editor: vscode.TextEditor): Promise<void> {
        const document = editor.document;
        if (document.languageId !== 'ironshell') {
            vscode.window.showWarningMessage('Not an IronShell file');
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

        // Multi-line selections go through the command flag; a bare paste
        // would be read by whatever shell owns the terminal, not by ae.
        //
        // The flag is -c. This read -e, which ae has never accepted --
        //   error: unexpected argument '-e' found
        // -- so running a multi-line selection failed for everyone who
        // tried it. Verified against the clap definition in src/main.rs
        // (`#[arg(long, short = 'c')]`) and against the published binary.
        if (code.includes('\n')) {
            const escapedCode = code.replace(/"/g, '\\"');
            terminal.sendText(`${this.getIronShellPath()} -c "${escapedCode}"`);
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
            name: 'IronShell TUI',
            shellPath: this.getIronShellPath(),
            // `tui` is a subcommand, not a flag. This read ['--tui'], which
            // ae rejects with "unexpected argument '--tui' found", so the
            // Open TUI command opened a terminal that immediately errored.
            shellArgs: ['tui'],
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
): IronShellRunner {
    const runner = new IronShellRunner(outputChannel);

    // Run current file
    context.subscriptions.push(
        vscode.commands.registerCommand('ironshell.runFile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (editor) {
                await runner.runFile(editor.document);
            }
        })
    );

    // Run selection or line
    context.subscriptions.push(
        vscode.commands.registerCommand('ironshell.runSelection', async () => {
            const editor = vscode.window.activeTextEditor;
            if (editor) {
                await runner.runSelection(editor);
            }
        })
    );

    // Start REPL
    context.subscriptions.push(
        vscode.commands.registerCommand('ironshell.startRepl', () => {
            runner.startRepl();
        })
    );

    // Start TUI
    context.subscriptions.push(
        vscode.commands.registerCommand('ironshell.startTui', () => {
            runner.startTui();
        })
    );

    return runner;
}
