import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';
import { activate as activateMarkdownPreview } from './markdownPreview';
import { registerProviders } from './providers';
import { registerRunnerCommands, AetherShellRunner } from './runner';
import { AetherShellCodeActionProvider, registerCodeActionCommands } from './codeActions';
import { registerAgentPanel } from './agentPanel';

let client: LanguageClient | undefined;
let outputChannel: vscode.OutputChannel;
let runner: AetherShellRunner | undefined;
let extensionContext: vscode.ExtensionContext;

export async function activate(context: vscode.ExtensionContext) {
    extensionContext = context;
    outputChannel = vscode.window.createOutputChannel('AetherShell');
    outputChannel.appendLine('AetherShell extension activating...');

    // Register language providers (symbols, folding, hover)
    registerProviders(context);
    outputChannel.appendLine('Language providers registered');

    // Register code runner commands
    runner = registerRunnerCommands(context, outputChannel);
    outputChannel.appendLine('Code runner registered');

    // Register code action provider
    context.subscriptions.push(
        vscode.languages.registerCodeActionsProvider(
            { language: 'aethershell' },
            new AetherShellCodeActionProvider(),
            {
                providedCodeActionKinds: AetherShellCodeActionProvider.providedCodeActionKinds
            }
        )
    );
    registerCodeActionCommands(context);
    outputChannel.appendLine('Code actions registered');

    // Register agent panel
    registerAgentPanel(context);
    outputChannel.appendLine('Agent panel registered');

    // Register run-on-save handler
    const config = vscode.workspace.getConfiguration('aethershell');
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument(async (document) => {
            const runOnSave = vscode.workspace.getConfiguration('aethershell').get<boolean>('runOnSave', false);
            if (runOnSave && document.languageId === 'aethershell' && runner) {
                await runner.runFile(document);
            }
        })
    );

    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    // Always register these commands so they're available even if LSP is disabled
    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.restartServer', async () => {
            await restartServer();
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('aethershell.showOutput', () => {
            outputChannel.show();
        })
    );

    if (!lspEnabled) {
        outputChannel.appendLine('Language server is disabled via configuration');
    } else {
        // Start the language server
        await startServer(context);
    }

    // Return the markdown preview plugin for VS Code to use
    return activateMarkdownPreview();
}

async function startServer(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('aethershell');
    let serverPath = config.get<string>('lsp.path', '');

    // If no path configured, look for the binary
    if (!serverPath) {
        // Try to find in common locations
        const possiblePaths = [
            // Development: built from source
            path.join(context.extensionPath, '..', '..', 'target', 'release', 'aethershell-lsp'),
            path.join(context.extensionPath, '..', '..', 'target', 'debug', 'aethershell-lsp'),
            // Installed globally
            'aethershell-lsp',
        ];

        // On Windows, add .exe
        if (process.platform === 'win32') {
            possiblePaths.unshift(
                path.join(context.extensionPath, '..', '..', 'target', 'release', 'aethershell-lsp.exe'),
                path.join(context.extensionPath, '..', '..', 'target', 'debug', 'aethershell-lsp.exe')
            );
        }

        for (const p of possiblePaths) {
            try {
                const { execSync } = require('child_process');
                if (p.includes('target')) {
                    // Check if file exists
                    const fs = require('fs');
                    if (fs.existsSync(p)) {
                        serverPath = p;
                        break;
                    }
                } else {
                    // Try to run it
                    execSync(`${p} --version`, { stdio: 'ignore' });
                    serverPath = p;
                    break;
                }
            } catch {
                // Try next path
            }
        }
    }

    if (!serverPath) {
        outputChannel.appendLine('Warning: AetherShell language server not found');
        outputChannel.appendLine('Please build the LSP server with: cargo build -p aethershell-lsp --release');
        outputChannel.appendLine('Or set the path in settings: aethershell.lsp.path');
        vscode.window.showWarningMessage(
            'AetherShell language server not found. Build it with "cargo build -p aethershell-lsp --release"'
        );
        return;
    }

    outputChannel.appendLine(`Starting language server: ${serverPath}`);

    const serverOptions: ServerOptions = {
        run: {
            command: serverPath,
            transport: TransportKind.stdio
        },
        debug: {
            command: serverPath,
            transport: TransportKind.stdio
        }
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'aethershell' },
            { scheme: 'untitled', language: 'aethershell' }
        ],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.ae')
        },
        outputChannel,
        traceOutputChannel: outputChannel
    };

    client = new LanguageClient(
        'aethershell',
        'AetherShell Language Server',
        serverOptions,
        clientOptions
    );

    try {
        await client.start();
        outputChannel.appendLine('Language server started successfully');
    } catch (error) {
        outputChannel.appendLine(`Failed to start language server: ${error}`);
        vscode.window.showErrorMessage(`Failed to start AetherShell language server: ${error}`);
    }
}

async function restartServer() {
    outputChannel.appendLine('Restarting language server...');

    if (client) {
        try {
            await client.stop();
        } catch (error) {
            outputChannel.appendLine(`Warning during stop: ${error}`);
        }
        client = undefined;
    }

    if (extensionContext) {
        await startServer(extensionContext);
    } else {
        outputChannel.appendLine('Extension context not available — cannot restart');
        vscode.window.showErrorMessage('Could not restart: extension context unavailable');
    }
}

export async function deactivate(): Promise<void> {
    if (runner) {
        runner.dispose();
        runner = undefined;
    }
    if (client) {
        await client.stop();
        client = undefined;
    }
}
