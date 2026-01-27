/**
 * AetherShell Browser Extension - Background Service Worker
 * 
 * Handles extension lifecycle, context menus, and WASM initialization.
 */

import init, { AetherShell, ae_version } from './wasm/aether_wasm.js';

// Global shell instance
let shell = null;
let wasmReady = false;

// Initialize WASM module
async function initWasm() {
    try {
        await init();
        shell = new AetherShell();
        wasmReady = true;
        console.log(`AetherShell initialized: ${ae_version()}`);
    } catch (error) {
        console.error('Failed to initialize AetherShell WASM:', error);
    }
}

// Initialize on install/startup
chrome.runtime.onInstalled.addListener(async () => {
    await initWasm();
    setupContextMenus();
});

chrome.runtime.onStartup.addListener(async () => {
    if (!wasmReady) {
        await initWasm();
    }
});

// Setup context menus
function setupContextMenus() {
    chrome.contextMenus.create({
        id: 'aethershell-ai-explain',
        title: 'AI: Explain Selection',
        contexts: ['selection']
    });

    chrome.contextMenus.create({
        id: 'aethershell-ai-summarize',
        title: 'AI: Summarize Selection',
        contexts: ['selection']
    });

    chrome.contextMenus.create({
        id: 'aethershell-ai-translate',
        title: 'AI: Translate Selection',
        contexts: ['selection']
    });

    chrome.contextMenus.create({
        id: 'aethershell-separator',
        type: 'separator',
        contexts: ['selection']
    });

    chrome.contextMenus.create({
        id: 'aethershell-eval',
        title: 'Evaluate as AetherShell',
        contexts: ['selection']
    });
}

// Handle context menu clicks
chrome.contextMenus.onClicked.addListener(async (info, tab) => {
    if (!wasmReady) {
        await initWasm();
    }

    const selectedText = info.selectionText;
    
    switch (info.menuItemId) {
        case 'aethershell-ai-explain':
            handleAIAction(tab.id, 'explain', selectedText);
            break;
        case 'aethershell-ai-summarize':
            handleAIAction(tab.id, 'summarize', selectedText);
            break;
        case 'aethershell-ai-translate':
            handleAIAction(tab.id, 'translate', selectedText);
            break;
        case 'aethershell-eval':
            handleEval(tab.id, selectedText);
            break;
    }
});

// Handle AI actions
async function handleAIAction(tabId, action, text) {
    // For now, show a notification - full AI integration requires API keys
    const prompts = {
        explain: `Explain the following:\n\n${text}`,
        summarize: `Summarize the following:\n\n${text}`,
        translate: `Translate the following to English:\n\n${text}`
    };

    // Send to content script to display result
    chrome.tabs.sendMessage(tabId, {
        type: 'ai-result',
        action,
        prompt: prompts[action],
        // In full implementation, this would call the AI
        result: `[AI ${action} would process: "${text.substring(0, 50)}..."]`
    });
}

// Handle AetherShell evaluation
async function handleEval(tabId, code) {
    if (!shell) {
        chrome.tabs.sendMessage(tabId, {
            type: 'eval-result',
            error: 'AetherShell not initialized'
        });
        return;
    }

    try {
        const result = shell.evalJson(code);
        chrome.tabs.sendMessage(tabId, {
            type: 'eval-result',
            code,
            result: JSON.parse(result)
        });
    } catch (error) {
        chrome.tabs.sendMessage(tabId, {
            type: 'eval-result',
            code,
            error: error.message
        });
    }
}

// Handle keyboard commands
chrome.commands.onCommand.addListener(async (command) => {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    
    switch (command) {
        case 'open-terminal':
            chrome.tabs.sendMessage(tab.id, { type: 'toggle-terminal' });
            break;
        case 'ai-assist':
            chrome.tabs.sendMessage(tab.id, { type: 'ai-assist-selection' });
            break;
    }
});

// Handle messages from popup and content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.type === 'eval') {
        if (!shell) {
            sendResponse({ error: 'AetherShell not initialized' });
            return true;
        }

        try {
            const result = shell.evalJson(message.code);
            sendResponse({ result: JSON.parse(result) });
        } catch (error) {
            sendResponse({ error: error.message });
        }
        return true;
    }

    if (message.type === 'get-version') {
        sendResponse({ version: wasmReady ? ae_version() : 'not initialized' });
        return true;
    }

    if (message.type === 'reset') {
        if (shell) {
            shell.reset();
            sendResponse({ success: true });
        } else {
            sendResponse({ error: 'Shell not initialized' });
        }
        return true;
    }
});

// Initialize immediately
initWasm();
