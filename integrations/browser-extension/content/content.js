/**
 * AetherShell Browser Extension - Content Script
 * 
 * Provides the terminal overlay and page integration functionality.
 */

// Terminal overlay state
let terminalOverlay = null;
let isTerminalVisible = false;
let commandHistory = [];
let historyIndex = -1;

// Initialize content script
function init() {
    // Listen for messages from background script
    chrome.runtime.onMessage.addListener(handleMessage);

    // Listen for keyboard shortcuts
    document.addEventListener('keydown', handleKeydown);
}

// Handle messages from background script
function handleMessage(request, sender, sendResponse) {
    switch (request.type) {
        case 'toggle-terminal':
            toggleTerminal();
            sendResponse({ success: true });
            break;
        case 'eval-result':
            if (request.error) {
                showResultOverlay({ error: request.error }, true);
            } else {
                showResultOverlay(request.result, false);
            }
            sendResponse({ success: true });
            break;
        case 'ai-result':
            showAIResponseOverlay(request.result, request.action);
            sendResponse({ success: true });
            break;
        case 'ai-assist-selection':
            handleAIAssistSelection();
            sendResponse({ success: true });
            break;
        default:
            sendResponse({ success: false, error: 'Unknown action' });
    }
    return true;
}

// Handle keyboard shortcuts
function handleKeydown(e) {
    // Ctrl/Cmd + Shift + A to toggle terminal
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'A') {
        e.preventDefault();
        toggleTerminal();
    }

    // Escape to close terminal
    if (e.key === 'Escape' && isTerminalVisible) {
        hideTerminal();
    }
}

// Toggle terminal visibility
function toggleTerminal() {
    if (isTerminalVisible) {
        hideTerminal();
    } else {
        showTerminal();
    }
}

// Show terminal overlay
function showTerminal() {
    if (terminalOverlay) {
        terminalOverlay.style.display = 'flex';
        isTerminalVisible = true;
        terminalOverlay.querySelector('.ae-terminal-input').focus();
        return;
    }

    // Create terminal overlay
    terminalOverlay = document.createElement('div');
    terminalOverlay.id = 'aethershell-terminal-overlay';
    terminalOverlay.innerHTML = `
        <div class="ae-terminal-container">
            <div class="ae-terminal-header">
                <span class="ae-terminal-title">⚡ AetherShell Terminal</span>
                <div class="ae-terminal-controls">
                    <button class="ae-terminal-minimize" title="Minimize">−</button>
                    <button class="ae-terminal-close" title="Close">×</button>
                </div>
            </div>
            <div class="ae-terminal-output" id="ae-output"></div>
            <div class="ae-terminal-input-line">
                <span class="ae-terminal-prompt">ae></span>
                <input type="text" class="ae-terminal-input" placeholder="Enter AetherShell command..." autocomplete="off" />
            </div>
            <div class="ae-terminal-status">
                <span class="ae-terminal-version">AetherShell v0.2.0</span>
                <span class="ae-terminal-mode">WASM Mode</span>
            </div>
        </div>
    `;

    // Add styles
    const style = document.createElement('style');
    style.textContent = getTerminalStyles();
    terminalOverlay.appendChild(style);

    document.body.appendChild(terminalOverlay);

    // Attach event listeners
    setupTerminalEvents();

    isTerminalVisible = true;
    terminalOverlay.querySelector('.ae-terminal-input').focus();
}

// Hide terminal overlay
function hideTerminal() {
    if (terminalOverlay) {
        terminalOverlay.style.display = 'none';
        isTerminalVisible = false;
    }
}

// Setup terminal event listeners
function setupTerminalEvents() {
    const input = terminalOverlay.querySelector('.ae-terminal-input');
    const closeBtn = terminalOverlay.querySelector('.ae-terminal-close');
    const minimizeBtn = terminalOverlay.querySelector('.ae-terminal-minimize');

    // Handle command input
    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            executeCommand(input.value);
            input.value = '';
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            navigateHistory(-1, input);
        } else if (e.key === 'ArrowDown') {
            e.preventDefault();
            navigateHistory(1, input);
        }
    });

    // Close button
    closeBtn.addEventListener('click', () => {
        hideTerminal();
    });

    // Minimize button
    minimizeBtn.addEventListener('click', () => {
        terminalOverlay.classList.toggle('ae-minimized');
    });

    // Click outside to close (optional)
    terminalOverlay.addEventListener('click', (e) => {
        if (e.target === terminalOverlay) {
            hideTerminal();
        }
    });
}

// Navigate command history
function navigateHistory(direction, input) {
    if (commandHistory.length === 0) return;

    historyIndex += direction;

    if (historyIndex < 0) {
        historyIndex = 0;
    } else if (historyIndex >= commandHistory.length) {
        historyIndex = commandHistory.length;
        input.value = '';
        return;
    }

    input.value = commandHistory[historyIndex];
}

// Execute a command
async function executeCommand(command) {
    if (!command.trim()) return;

    // Add to history
    commandHistory.push(command);
    historyIndex = commandHistory.length;

    // Show command in output
    appendOutput(`<span class="ae-prompt">ae></span> ${escapeHtml(command)}`, 'command');

    try {
        // Send to background script for evaluation
        const response = await chrome.runtime.sendMessage({
            type: 'eval',
            code: command
        });

        if (response.error) {
            appendOutput(`Error: ${response.error}`, 'error');
        } else {
            appendOutput(formatResult(response.result), 'result');
        }
    } catch (error) {
        appendOutput(`Error: ${error.message}`, 'error');
    }
}

// Append output to terminal
function appendOutput(content, type = 'result') {
    const output = terminalOverlay.querySelector('#ae-output');
    const line = document.createElement('div');
    line.className = `ae-output-line ae-${type}`;
    line.innerHTML = content;
    output.appendChild(line);
    output.scrollTop = output.scrollHeight;
}

// Format result for display
function formatResult(result) {
    if (typeof result === 'string') {
        return escapeHtml(result);
    }
    if (Array.isArray(result)) {
        return `<span class="ae-array">[${result.map(formatResult).join(', ')}]</span>`;
    }
    if (typeof result === 'object' && result !== null) {
        const entries = Object.entries(result)
            .map(([k, v]) => `<span class="ae-key">${escapeHtml(k)}</span>: ${formatResult(v)}`)
            .join(', ');
        return `<span class="ae-record">{${entries}}</span>`;
    }
    if (typeof result === 'number') {
        return `<span class="ae-number">${result}</span>`;
    }
    if (typeof result === 'boolean') {
        return `<span class="ae-bool">${result}</span>`;
    }
    return escapeHtml(String(result));
}

// Escape HTML entities
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Show result overlay (for context menu actions)
function showResultOverlay(result, isError = false) {
    const overlay = document.createElement('div');
    overlay.className = 'ae-result-overlay';
    const headerClass = isError ? 'ae-result-header ae-error-header' : 'ae-result-header';
    const contentClass = isError ? 'ae-result-content ae-error-content' : 'ae-result-content';

    overlay.innerHTML = `
        <div class="ae-result-container">
            <div class="${headerClass}">
                <span>${isError ? '❌ Error' : '✓ AetherShell Result'}</span>
                <button class="ae-result-close">×</button>
            </div>
            <div class="${contentClass}">
                <pre>${isError ? escapeHtml(result.error || String(result)) : formatResult(result)}</pre>
            </div>
        </div>
    `;

    document.body.appendChild(overlay);

    overlay.querySelector('.ae-result-close').addEventListener('click', () => {
        overlay.remove();
    });

    overlay.addEventListener('click', (e) => {
        if (e.target === overlay) {
            overlay.remove();
        }
    });

    setTimeout(() => overlay.classList.add('ae-visible'), 10);
}

// Handle AI assist on current selection
function handleAIAssistSelection() {
    const selection = window.getSelection().toString().trim();
    if (selection) {
        // Send selection to background for AI processing
        chrome.runtime.sendMessage({
            type: 'ai-assist',
            text: selection
        });
    }
}

// Show AI response overlay
function showAIResponseOverlay(response, action) {
    const overlay = document.createElement('div');
    overlay.className = 'ae-result-overlay ae-ai-overlay';
    overlay.innerHTML = `
        <div class="ae-result-container">
            <div class="ae-result-header ae-ai-header">
                <span>🤖 AI ${action.charAt(0).toUpperCase() + action.slice(1)}</span>
                <button class="ae-result-close">×</button>
            </div>
            <div class="ae-result-content ae-ai-content">
                <div class="ae-ai-response">${escapeHtml(response)}</div>
            </div>
            <div class="ae-result-actions">
                <button class="ae-copy-btn">📋 Copy</button>
            </div>
        </div>
    `;

    document.body.appendChild(overlay);

    overlay.querySelector('.ae-result-close').addEventListener('click', () => {
        overlay.remove();
    });

    overlay.querySelector('.ae-copy-btn').addEventListener('click', () => {
        navigator.clipboard.writeText(response);
    });

    overlay.addEventListener('click', (e) => {
        if (e.target === overlay) {
            overlay.remove();
        }
    });

    setTimeout(() => overlay.classList.add('ae-visible'), 10);
}

// Get terminal CSS styles
function getTerminalStyles() {
    return `
        #aethershell-terminal-overlay {
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: rgba(0, 0, 0, 0.5);
            display: flex;
            justify-content: center;
            align-items: flex-start;
            padding-top: 50px;
            z-index: 2147483647;
            font-family: 'JetBrains Mono', 'Fira Code', 'Monaco', 'Consolas', monospace;
        }
        
        .ae-terminal-container {
            width: 800px;
            max-width: 90vw;
            background: #1a1a2e;
            border-radius: 8px;
            box-shadow: 0 25px 50px rgba(0, 0, 0, 0.5);
            overflow: hidden;
            border: 1px solid #2a2a4e;
        }
        
        .ae-terminal-header {
            background: linear-gradient(135deg, #16213e 0%, #0f0f23 100%);
            padding: 12px 16px;
            display: flex;
            justify-content: space-between;
            align-items: center;
            border-bottom: 1px solid #2a2a4e;
        }
        
        .ae-terminal-title {
            color: #00d9ff;
            font-size: 14px;
            font-weight: 600;
        }
        
        .ae-terminal-controls button {
            background: transparent;
            border: none;
            color: #888;
            font-size: 18px;
            cursor: pointer;
            padding: 4px 8px;
            margin-left: 4px;
            border-radius: 4px;
            transition: all 0.2s;
        }
        
        .ae-terminal-controls button:hover {
            background: rgba(255, 255, 255, 0.1);
            color: #fff;
        }
        
        .ae-terminal-close:hover {
            background: #e74c3c !important;
        }
        
        .ae-terminal-output {
            height: 300px;
            overflow-y: auto;
            padding: 16px;
            font-size: 13px;
            line-height: 1.6;
            color: #e0e0e0;
        }
        
        .ae-output-line {
            margin-bottom: 8px;
            white-space: pre-wrap;
            word-wrap: break-word;
        }
        
        .ae-command .ae-prompt {
            color: #00d9ff;
            font-weight: bold;
        }
        
        .ae-result {
            color: #98c379;
        }
        
        .ae-error {
            color: #e74c3c;
        }
        
        .ae-array {
            color: #e5c07b;
        }
        
        .ae-record {
            color: #56b6c2;
        }
        
        .ae-key {
            color: #c678dd;
        }
        
        .ae-number {
            color: #d19a66;
        }
        
        .ae-bool {
            color: #56b6c2;
        }
        
        .ae-terminal-input-line {
            display: flex;
            align-items: center;
            padding: 12px 16px;
            background: #0f0f23;
            border-top: 1px solid #2a2a4e;
        }
        
        .ae-terminal-prompt {
            color: #00d9ff;
            font-weight: bold;
            margin-right: 8px;
        }
        
        .ae-terminal-input {
            flex: 1;
            background: transparent;
            border: none;
            color: #e0e0e0;
            font-family: inherit;
            font-size: 14px;
            outline: none;
        }
        
        .ae-terminal-input::placeholder {
            color: #555;
        }
        
        .ae-terminal-status {
            padding: 8px 16px;
            background: #0d0d17;
            display: flex;
            justify-content: space-between;
            font-size: 11px;
            color: #666;
        }
        
        .ae-terminal-mode {
            color: #00d9ff;
        }
        
        #aethershell-terminal-overlay.ae-minimized .ae-terminal-output,
        #aethershell-terminal-overlay.ae-minimized .ae-terminal-input-line,
        #aethershell-terminal-overlay.ae-minimized .ae-terminal-status {
            display: none;
        }
        
        .ae-result-overlay {
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: rgba(0, 0, 0, 0.6);
            display: flex;
            justify-content: center;
            align-items: center;
            z-index: 2147483647;
            opacity: 0;
            transition: opacity 0.2s;
        }
        
        .ae-result-overlay.ae-visible {
            opacity: 1;
        }
        
        .ae-result-container {
            background: #1a1a2e;
            border-radius: 12px;
            max-width: 600px;
            width: 90%;
            max-height: 80vh;
            overflow: hidden;
            box-shadow: 0 25px 50px rgba(0, 0, 0, 0.5);
            border: 1px solid #2a2a4e;
        }
        
        .ae-result-header {
            background: linear-gradient(135deg, #16213e 0%, #0f0f23 100%);
            padding: 16px 20px;
            display: flex;
            justify-content: space-between;
            align-items: center;
            font-weight: 600;
            color: #00d9ff;
        }
        
        .ae-ai-header {
            background: linear-gradient(135deg, #1a3a2e 0%, #0f2319 100%);
            color: #00ff9d;
        }
        
        .ae-result-content {
            padding: 20px;
            overflow-y: auto;
            max-height: 400px;
            color: #e0e0e0;
            font-family: 'JetBrains Mono', monospace;
            font-size: 13px;
            line-height: 1.6;
        }
        
        .ae-result-content pre {
            margin: 0;
            white-space: pre-wrap;
        }
        
        .ae-ai-content {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            font-size: 14px;
        }
        
        .ae-result-actions {
            padding: 12px 20px;
            background: #0d0d17;
            display: flex;
            gap: 8px;
            justify-content: flex-end;
        }
        
        .ae-result-actions button {
            background: #16213e;
            border: 1px solid #2a2a4e;
            color: #e0e0e0;
            padding: 8px 16px;
            border-radius: 6px;
            cursor: pointer;
            font-size: 13px;
            transition: all 0.2s;
        }
        
        .ae-result-actions button:hover {
            background: #1f3057;
            border-color: #00d9ff;
        }
        
        .ae-error-header {
            background: linear-gradient(135deg, #3a1a1a 0%, #230f0f 100%);
            color: #e74c3c;
        }
        
        .ae-error-content {
            color: #e74c3c;
        }
    `;
}

// Initialize
init();
