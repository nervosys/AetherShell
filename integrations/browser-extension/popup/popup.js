/**
 * AetherShell Browser Extension - Popup Script
 */

document.addEventListener('DOMContentLoaded', async () => {
    const codeInput = document.getElementById('code');
    const output = document.getElementById('output');
    const runBtn = document.getElementById('run');
    const clearBtn = document.getElementById('clear');
    const terminalBtn = document.getElementById('terminal');
    const versionSpan = document.getElementById('version');

    // Get version from background script
    chrome.runtime.sendMessage({ type: 'get-version' }, (response) => {
        if (response && response.version) {
            versionSpan.textContent = response.version;
        }
    });

    // Run code
    runBtn.addEventListener('click', async () => {
        const code = codeInput.value.trim();
        if (!code) return;

        output.className = 'output';
        output.innerHTML = '<pre>Running...</pre>';

        chrome.runtime.sendMessage({ type: 'eval', code }, (response) => {
            if (response.error) {
                output.className = 'output error';
                output.innerHTML = `<pre>Error: ${response.error}</pre>`;
            } else {
                output.className = 'output success';
                const formatted = typeof response.result === 'object'
                    ? JSON.stringify(response.result, null, 2)
                    : String(response.result);
                output.innerHTML = `<pre>${escapeHtml(formatted)}</pre>`;
            }
        });
    });

    // Clear output
    clearBtn.addEventListener('click', () => {
        output.className = 'output';
        output.innerHTML = '<pre>Cleared.</pre>';
        codeInput.value = '';
    });

    // Open terminal overlay
    terminalBtn.addEventListener('click', async () => {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        chrome.tabs.sendMessage(tab.id, { type: 'toggle-terminal' });
        window.close();
    });

    // Run on Ctrl+Enter
    codeInput.addEventListener('keydown', (e) => {
        if (e.ctrlKey && e.key === 'Enter') {
            runBtn.click();
        }
    });

    // Load last code from storage
    chrome.storage.local.get(['lastCode'], (result) => {
        if (result.lastCode) {
            codeInput.value = result.lastCode;
        }
    });

    // Save code on change
    codeInput.addEventListener('input', () => {
        chrome.storage.local.set({ lastCode: codeInput.value });
    });
});

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}
