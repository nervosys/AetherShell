import * as vscode from 'vscode';

/**
 * Agent Panel WebView Provider
 * Provides a panel for managing and interacting with AI agents
 */
export class AgentPanelProvider implements vscode.WebviewViewProvider {
    public static readonly viewType = 'aethershell.agentPanel';

    private _view?: vscode.WebviewView;
    private _agents: AgentInfo[] = [];

    constructor(private readonly _extensionUri: vscode.Uri) { }

    public resolveWebviewView(
        webviewView: vscode.WebviewView,
        _context: vscode.WebviewViewResolveContext,
        _token: vscode.CancellationToken
    ): void {
        this._view = webviewView;

        webviewView.webview.options = {
            enableScripts: true,
            localResourceRoots: [this._extensionUri]
        };

        webviewView.webview.html = this._getHtmlForWebview(webviewView.webview);

        // Handle messages from the webview
        webviewView.webview.onDidReceiveMessage(async (data) => {
            switch (data.type) {
                case 'createAgent':
                    await this._createAgent(data.config);
                    break;
                case 'sendMessage':
                    await this._sendMessage(data.agentId, data.message);
                    break;
                case 'deleteAgent':
                    this._deleteAgent(data.agentId);
                    break;
                case 'refresh':
                    this._refreshAgents();
                    break;
            }
        });
    }

    private async _createAgent(config: { name: string; prompt: string; tools: string[] }): Promise<void> {
        const agent: AgentInfo = {
            id: `agent_${Date.now()}`,
            name: config.name,
            prompt: config.prompt,
            tools: config.tools,
            status: 'idle',
            messages: []
        };

        this._agents.push(agent);
        this._updateWebview();

        // Insert agent code into active editor
        const editor = vscode.window.activeTextEditor;
        if (editor && editor.document.languageId === 'aethershell') {
            const toolsStr = config.tools.length > 0
                ? `, { tools: [${config.tools.map(t => `"${t}"`).join(', ')}] }`
                : '';
            const code = `let ${config.name} = agent("${config.prompt}"${toolsStr})\n`;

            await editor.edit(editBuilder => {
                editBuilder.insert(editor.selection.start, code);
            });
        }
    }

    private async _sendMessage(agentId: string, message: string): Promise<void> {
        const agent = this._agents.find(a => a.id === agentId);
        if (!agent) {
            return;
        }

        agent.status = 'running';
        agent.messages.push({ role: 'user', content: message });
        this._updateWebview();

        // In a real implementation, this would communicate with the running agent
        // For now, we show how to use the agent in the terminal
        const terminal = vscode.window.createTerminal(`Agent: ${agent.name}`);
        terminal.show();
        terminal.sendText(`# Sending message to ${agent.name}`);
        terminal.sendText(`${agent.name}("${message}")`);

        agent.status = 'idle';
        this._updateWebview();
    }

    private _deleteAgent(agentId: string): void {
        this._agents = this._agents.filter(a => a.id !== agentId);
        this._updateWebview();
    }

    private _refreshAgents(): void {
        // Scan workspace for agent definitions
        this._scanForAgents();
    }

    private async _scanForAgents(): Promise<void> {
        const files = await vscode.workspace.findFiles('**/*.ae', '**/node_modules/**', 100);

        for (const file of files) {
            const document = await vscode.workspace.openTextDocument(file);
            const text = document.getText();

            // Find agent definitions
            const agentRegex = /let\s+(\w+)\s*=\s*agent\s*\("([^"]+)"(?:,\s*\{[^}]*tools:\s*\[([^\]]*)\][^}]*\})?\)/g;
            let match;

            while ((match = agentRegex.exec(text)) !== null) {
                const [, name, prompt, toolsStr] = match;
                const tools = toolsStr
                    ? toolsStr.split(',').map(t => t.trim().replace(/"/g, '')).filter(t => t)
                    : [];

                // Check if already tracked
                if (!this._agents.find(a => a.name === name)) {
                    this._agents.push({
                        id: `agent_${name}_${Date.now()}`,
                        name,
                        prompt,
                        tools,
                        status: 'idle',
                        messages: [],
                        sourceFile: file.fsPath
                    });
                }
            }
        }

        this._updateWebview();
    }

    private _updateWebview(): void {
        if (this._view) {
            this._view.webview.postMessage({
                type: 'updateAgents',
                agents: this._agents
            });
        }
    }

    private _getHtmlForWebview(webview: vscode.Webview): string {
        return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>AetherShell Agents</title>
    <style>
        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }
        body {
            font-family: var(--vscode-font-family);
            color: var(--vscode-foreground);
            background: var(--vscode-sideBar-background);
            padding: 12px;
        }
        .header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 16px;
        }
        h2 {
            font-size: 14px;
            font-weight: 600;
        }
        .btn {
            background: var(--vscode-button-background);
            color: var(--vscode-button-foreground);
            border: none;
            padding: 6px 12px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 12px;
        }
        .btn:hover {
            background: var(--vscode-button-hoverBackground);
        }
        .btn-icon {
            background: transparent;
            padding: 4px;
            display: flex;
            align-items: center;
            justify-content: center;
        }
        .agent-list {
            display: flex;
            flex-direction: column;
            gap: 8px;
        }
        .agent-card {
            background: var(--vscode-editor-background);
            border: 1px solid var(--vscode-panel-border);
            border-radius: 6px;
            padding: 12px;
        }
        .agent-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 8px;
        }
        .agent-name {
            font-weight: 600;
            color: var(--vscode-textLink-foreground);
        }
        .agent-status {
            font-size: 10px;
            padding: 2px 6px;
            border-radius: 10px;
            text-transform: uppercase;
        }
        .status-idle {
            background: var(--vscode-testing-iconPassed);
            color: white;
        }
        .status-running {
            background: var(--vscode-progressBar-background);
            color: white;
        }
        .agent-prompt {
            font-size: 12px;
            color: var(--vscode-descriptionForeground);
            margin-bottom: 8px;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }
        .agent-tools {
            display: flex;
            flex-wrap: wrap;
            gap: 4px;
            margin-bottom: 8px;
        }
        .tool-badge {
            font-size: 10px;
            padding: 2px 6px;
            background: var(--vscode-badge-background);
            color: var(--vscode-badge-foreground);
            border-radius: 4px;
        }
        .agent-input {
            display: flex;
            gap: 4px;
        }
        .agent-input input {
            flex: 1;
            background: var(--vscode-input-background);
            border: 1px solid var(--vscode-input-border);
            color: var(--vscode-input-foreground);
            padding: 4px 8px;
            border-radius: 4px;
            font-size: 12px;
        }
        .empty-state {
            text-align: center;
            padding: 24px;
            color: var(--vscode-descriptionForeground);
        }
        .create-form {
            display: none;
            flex-direction: column;
            gap: 8px;
            margin-bottom: 16px;
            padding: 12px;
            background: var(--vscode-editor-background);
            border: 1px solid var(--vscode-panel-border);
            border-radius: 6px;
        }
        .create-form.visible {
            display: flex;
        }
        .create-form input, .create-form textarea {
            background: var(--vscode-input-background);
            border: 1px solid var(--vscode-input-border);
            color: var(--vscode-input-foreground);
            padding: 8px;
            border-radius: 4px;
            font-size: 12px;
        }
        .create-form textarea {
            resize: vertical;
            min-height: 60px;
        }
        label {
            font-size: 11px;
            color: var(--vscode-descriptionForeground);
            margin-bottom: 2px;
        }
    </style>
</head>
<body>
    <div class="header">
        <h2>🤖 AI Agents</h2>
        <div>
            <button class="btn btn-icon" onclick="refresh()" title="Refresh">🔄</button>
            <button class="btn" onclick="toggleCreateForm()">+ New</button>
        </div>
    </div>

    <div id="createForm" class="create-form">
        <label>Agent Name</label>
        <input type="text" id="agentName" placeholder="my_assistant" />
        
        <label>System Prompt</label>
        <textarea id="agentPrompt" placeholder="You are a helpful coding assistant..."></textarea>
        
        <label>Tools (comma-separated)</label>
        <input type="text" id="agentTools" placeholder="ls, cat, grep, http_get" />
        
        <div style="display: flex; gap: 8px; margin-top: 8px;">
            <button class="btn" onclick="createAgent()">Create Agent</button>
            <button class="btn" style="background: var(--vscode-button-secondaryBackground);" onclick="toggleCreateForm()">Cancel</button>
        </div>
    </div>

    <div id="agentList" class="agent-list">
        <div class="empty-state">
            <p>No agents yet.</p>
            <p>Create one or scan your workspace for existing agents.</p>
        </div>
    </div>

    <script>
        const vscode = acquireVsCodeApi();
        let agents = [];

        function toggleCreateForm() {
            document.getElementById('createForm').classList.toggle('visible');
        }

        function createAgent() {
            const name = document.getElementById('agentName').value.trim();
            const prompt = document.getElementById('agentPrompt').value.trim();
            const toolsStr = document.getElementById('agentTools').value.trim();
            const tools = toolsStr ? toolsStr.split(',').map(t => t.trim()) : [];

            if (!name || !prompt) {
                return;
            }

            vscode.postMessage({
                type: 'createAgent',
                config: { name, prompt, tools }
            });

            // Clear form
            document.getElementById('agentName').value = '';
            document.getElementById('agentPrompt').value = '';
            document.getElementById('agentTools').value = '';
            toggleCreateForm();
        }

        function sendMessage(agentId) {
            const input = document.getElementById('input_' + agentId);
            const message = input.value.trim();
            if (!message) return;

            vscode.postMessage({
                type: 'sendMessage',
                agentId,
                message
            });

            input.value = '';
        }

        function deleteAgent(agentId) {
            vscode.postMessage({
                type: 'deleteAgent',
                agentId
            });
        }

        function refresh() {
            vscode.postMessage({ type: 'refresh' });
        }

        function renderAgents() {
            const container = document.getElementById('agentList');
            
            if (agents.length === 0) {
                container.innerHTML = \`
                    <div class="empty-state">
                        <p>No agents yet.</p>
                        <p>Create one or scan your workspace for existing agents.</p>
                    </div>
                \`;
                return;
            }

            container.innerHTML = agents.map(agent => \`
                <div class="agent-card">
                    <div class="agent-header">
                        <span class="agent-name">\${agent.name}</span>
                        <div style="display: flex; gap: 4px; align-items: center;">
                            <span class="agent-status status-\${agent.status}">\${agent.status}</span>
                            <button class="btn btn-icon" onclick="deleteAgent('\${agent.id}')" title="Delete">🗑️</button>
                        </div>
                    </div>
                    <div class="agent-prompt">\${agent.prompt}</div>
                    \${agent.tools.length > 0 ? \`
                        <div class="agent-tools">
                            \${agent.tools.map(t => \`<span class="tool-badge">\${t}</span>\`).join('')}
                        </div>
                    \` : ''}
                    <div class="agent-input">
                        <input type="text" id="input_\${agent.id}" placeholder="Send message..." onkeypress="if(event.key==='Enter')sendMessage('\${agent.id}')" />
                        <button class="btn" onclick="sendMessage('\${agent.id}')">Send</button>
                    </div>
                </div>
            \`).join('');
        }

        window.addEventListener('message', event => {
            const message = event.data;
            if (message.type === 'updateAgents') {
                agents = message.agents;
                renderAgents();
            }
        });

        // Initial render
        renderAgents();
        // Scan for agents on load
        refresh();
    </script>
</body>
</html>`;
    }
}

interface AgentInfo {
    id: string;
    name: string;
    prompt: string;
    tools: string[];
    status: 'idle' | 'running' | 'error';
    messages: { role: string; content: string }[];
    sourceFile?: string;
}

/**
 * Register the agent panel
 */
export function registerAgentPanel(context: vscode.ExtensionContext): void {
    const provider = new AgentPanelProvider(context.extensionUri);

    context.subscriptions.push(
        vscode.window.registerWebviewViewProvider(AgentPanelProvider.viewType, provider)
    );
}
