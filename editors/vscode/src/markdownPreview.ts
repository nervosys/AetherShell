/**
 * Markdown-it plugin for AetherShell syntax highlighting in VS Code Markdown Preview
 */

import type MarkdownIt from 'markdown-it';

// AetherShell syntax highlighting rules
const KEYWORDS = ['let', 'fn', 'match', 'if', 'else', 'true', 'false', 'null', 'Some', 'None'];
const BUILTINS = [
    // Core
    'print', 'echo', 'help', 'call', 'type_of', 'len', 'keys', 'values',
    // Functional
    'map', 'where', 'reduce', 'take', 'skip', 'first', 'last', 'any', 'all', 'sort_by',
    // String
    'split', 'join', 'trim', 'upper', 'lower', 'replace', 'contains', 'starts_with', 'ends_with',
    // Array
    'flatten', 'reverse', 'slice', 'range', 'zip', 'push', 'pop',
    // Math
    'abs', 'min', 'max', 'sqrt', 'pow', 'floor', 'ceil', 'round', 'sum', 'avg', 'product', 'unique',
    // File system
    'ls', 'cat', 'pwd', 'cd', 'exists', 'mkdir', 'rm', 'cp', 'mv', 'touch', 'read',
    // Config
    'config', 'config_get', 'config_set', 'config_path', 'themes',
    // AI
    'ai', 'agent', 'swarm', 'ai_model', 'mcp_tools', 'mcp_call',
    // Neural networks
    'nn_create', 'population', 'evolve', 'rl_agent',
    // OS
    'env', 'which', 'os', 'arch', 'hostname', 'http_get'
];

function escapeHtml(str: string): string {
    return str
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
}

function highlightAetherShell(code: string): string {
    const lines = code.split('\n');
    const highlightedLines: string[] = [];

    for (const line of lines) {
        let result = '';
        let i = 0;

        while (i < line.length) {
            // Comments
            if (line[i] === '#' || (line[i] === '/' && line[i + 1] === '/')) {
                result += `<span class="hljs-comment">${escapeHtml(line.slice(i))}</span>`;
                break;
            }

            // Strings
            if (line[i] === '"') {
                let j = i + 1;
                while (j < line.length && line[j] !== '"') {
                    if (line[j] === '\\') j++;
                    j++;
                }
                j++; // include closing quote
                const str = line.slice(i, j);
                // Handle string interpolation
                const highlighted = str.replace(/\$\{([^}]+)\}/g,
                    '<span class="hljs-subst">${$1}</span>');
                result += `<span class="hljs-string">${escapeHtml(str).replace(/\$\{([^}]+)\}/g, '<span class="hljs-subst">${$1}</span>')}</span>`;
                i = j;
                continue;
            }

            // Numbers
            if (/\d/.test(line[i]) || (line[i] === '-' && /\d/.test(line[i + 1]))) {
                let j = i;
                if (line[j] === '-') j++;
                while (j < line.length && /[\d.]/.test(line[j])) j++;
                result += `<span class="hljs-number">${escapeHtml(line.slice(i, j))}</span>`;
                i = j;
                continue;
            }

            // Identifiers and keywords
            if (/[a-zA-Z_]/.test(line[i])) {
                let j = i;
                while (j < line.length && /[a-zA-Z0-9_]/.test(line[j])) j++;
                const word = line.slice(i, j);

                if (KEYWORDS.includes(word)) {
                    result += `<span class="hljs-keyword">${word}</span>`;
                } else if (BUILTINS.includes(word)) {
                    result += `<span class="hljs-built_in">${word}</span>`;
                } else {
                    result += escapeHtml(word);
                }
                i = j;
                continue;
            }

            // Operators
            if ('|=>+-*/%<>!&'.includes(line[i])) {
                let op = line[i];
                if (line[i + 1] && '=>|&<>!='.includes(line[i + 1])) {
                    op += line[i + 1];
                    i++;
                }
                result += `<span class="hljs-operator">${escapeHtml(op)}</span>`;
                i++;
                continue;
            }

            // Default: pass through
            result += escapeHtml(line[i]);
            i++;
        }

        highlightedLines.push(result);
    }

    return highlightedLines.join('\n');
}

export function activate(): { extendMarkdownIt: (md: MarkdownIt) => MarkdownIt } {
    return {
        extendMarkdownIt(md: MarkdownIt): MarkdownIt {
            const defaultFence = md.renderer.rules.fence;

            md.renderer.rules.fence = (tokens, idx, options, env, self) => {
                const token = tokens[idx];
                const info = token.info ? token.info.trim().toLowerCase() : '';

                // Check if this is an AetherShell code block
                if (info === 'ae' || info === 'aether' || info === 'aethershell') {
                    const code = token.content;
                    const highlighted = highlightAetherShell(code);
                    return `<pre class="aethershell-code"><code class="language-aethershell">${highlighted}</code></pre>`;
                }

                // Fall back to default renderer
                if (defaultFence) {
                    return defaultFence(tokens, idx, options, env, self);
                }

                return `<pre><code>${escapeHtml(token.content)}</code></pre>`;
            };

            return md;
        }
    };
}
