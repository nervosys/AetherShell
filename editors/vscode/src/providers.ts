import * as vscode from 'vscode';

/**
 * AetherShell Document Symbol Provider
 * Provides outline view with functions, variables, and structures
 */
export class AetherShellSymbolProvider implements vscode.DocumentSymbolProvider {
    provideDocumentSymbols(
        document: vscode.TextDocument,
        _token: vscode.CancellationToken
    ): vscode.DocumentSymbol[] {
        const symbols: vscode.DocumentSymbol[] = [];
        const text = document.getText();
        const lines = text.split('\n');

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            const trimmed = line.trim();

            // Skip comments and empty lines
            if (trimmed.startsWith('#') || trimmed.startsWith('//') || trimmed === '') {
                continue;
            }

            // Function definitions: let name = fn(params) => ...
            const fnMatch = line.match(/^\s*let\s+(\w+)\s*=\s*fn\s*\(([^)]*)\)\s*=>/);
            if (fnMatch) {
                const name = fnMatch[1];
                const params = fnMatch[2];
                const range = new vscode.Range(i, 0, i, line.length);
                const selectionRange = new vscode.Range(
                    i, line.indexOf(name),
                    i, line.indexOf(name) + name.length
                );
                const symbol = new vscode.DocumentSymbol(
                    name,
                    `fn(${params})`,
                    vscode.SymbolKind.Function,
                    range,
                    selectionRange
                );
                symbols.push(symbol);
                continue;
            }

            // Variable definitions: let name = value
            const letMatch = line.match(/^\s*let\s+(\w+)\s*=\s*(.+)/);
            if (letMatch) {
                const name = letMatch[1];
                const value = letMatch[2].trim();
                let kind = vscode.SymbolKind.Variable;
                let detail = '';

                // Determine type from value
                if (value.startsWith('{')) {
                    kind = vscode.SymbolKind.Object;
                    detail = 'Record';
                } else if (value.startsWith('[')) {
                    kind = vscode.SymbolKind.Array;
                    detail = 'Array';
                } else if (value.startsWith('"') || value.startsWith("'")) {
                    kind = vscode.SymbolKind.String;
                    detail = 'String';
                } else if (/^\d/.test(value)) {
                    kind = vscode.SymbolKind.Number;
                    detail = value.includes('.') ? 'Float' : 'Int';
                } else if (value === 'true' || value === 'false') {
                    kind = vscode.SymbolKind.Boolean;
                    detail = 'Bool';
                } else if (value.startsWith('fn(')) {
                    // Already handled above
                    continue;
                }

                const range = new vscode.Range(i, 0, i, line.length);
                const selectionRange = new vscode.Range(
                    i, line.indexOf(name),
                    i, line.indexOf(name) + name.length
                );
                const symbol = new vscode.DocumentSymbol(
                    name,
                    detail,
                    kind,
                    range,
                    selectionRange
                );
                symbols.push(symbol);
            }

            // Agent definitions: agent(...)
            const agentMatch = line.match(/^\s*agent\s*\(/);
            if (agentMatch) {
                const range = new vscode.Range(i, 0, i, line.length);
                const selectionRange = new vscode.Range(i, line.indexOf('agent'), i, line.indexOf('agent') + 5);
                const symbol = new vscode.DocumentSymbol(
                    'agent',
                    'AI Agent',
                    vscode.SymbolKind.Class,
                    range,
                    selectionRange
                );
                symbols.push(symbol);
            }

            // Match expressions
            const matchMatch = line.match(/^\s*match\s+(\w+)\s*\{/);
            if (matchMatch) {
                const varName = matchMatch[1];
                // Find the end of the match block
                let endLine = i;
                let braceCount = 1;
                for (let j = i + 1; j < lines.length && braceCount > 0; j++) {
                    const matchLine = lines[j];
                    braceCount += (matchLine.match(/\{/g) || []).length;
                    braceCount -= (matchLine.match(/\}/g) || []).length;
                    if (braceCount === 0) {
                        endLine = j;
                    }
                }
                const range = new vscode.Range(i, 0, endLine, lines[endLine].length);
                const selectionRange = new vscode.Range(i, line.indexOf('match'), i, line.indexOf('match') + 5);
                const symbol = new vscode.DocumentSymbol(
                    `match ${varName}`,
                    'Pattern Match',
                    vscode.SymbolKind.Enum,
                    range,
                    selectionRange
                );
                symbols.push(symbol);
            }
        }

        return symbols;
    }
}

/**
 * AetherShell Folding Range Provider
 * Provides code folding for blocks, arrays, records, and comments
 */
export class AetherShellFoldingProvider implements vscode.FoldingRangeProvider {
    provideFoldingRanges(
        document: vscode.TextDocument,
        _context: vscode.FoldingContext,
        _token: vscode.CancellationToken
    ): vscode.FoldingRange[] {
        const ranges: vscode.FoldingRange[] = [];
        const text = document.getText();
        const lines = text.split('\n');

        const stack: { char: string; line: number }[] = [];
        let commentStart: number | null = null;

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            const trimmed = line.trim();

            // Comment blocks
            if (trimmed.startsWith('#') || trimmed.startsWith('//')) {
                if (commentStart === null) {
                    commentStart = i;
                }
            } else {
                if (commentStart !== null && i - commentStart > 1) {
                    ranges.push(new vscode.FoldingRange(
                        commentStart,
                        i - 1,
                        vscode.FoldingRangeKind.Comment
                    ));
                }
                commentStart = null;
            }

            // Track braces, brackets, and parentheses
            for (let j = 0; j < line.length; j++) {
                const char = line[j];

                // Skip if inside string
                if (this.isInsideString(line, j)) {
                    continue;
                }

                if (char === '{' || char === '[' || char === '(') {
                    stack.push({ char, line: i });
                } else if (char === '}' || char === ']' || char === ')') {
                    const matching = char === '}' ? '{' : char === ']' ? '[' : '(';
                    // Find matching opener
                    for (let k = stack.length - 1; k >= 0; k--) {
                        if (stack[k].char === matching) {
                            const startLine = stack[k].line;
                            if (i > startLine) {
                                ranges.push(new vscode.FoldingRange(
                                    startLine,
                                    i,
                                    vscode.FoldingRangeKind.Region
                                ));
                            }
                            stack.splice(k, 1);
                            break;
                        }
                    }
                }
            }
        }

        // Handle trailing comments
        if (commentStart !== null && lines.length - commentStart > 1) {
            ranges.push(new vscode.FoldingRange(
                commentStart,
                lines.length - 1,
                vscode.FoldingRangeKind.Comment
            ));
        }

        return ranges;
    }

    private isInsideString(line: string, pos: number): boolean {
        let inString = false;
        let stringChar = '';

        for (let i = 0; i < pos; i++) {
            const char = line[i];
            if ((char === '"' || char === "'") && (i === 0 || line[i - 1] !== '\\')) {
                if (!inString) {
                    inString = true;
                    stringChar = char;
                } else if (char === stringChar) {
                    inString = false;
                }
            }
        }

        return inString;
    }
}

/**
 * AetherShell Hover Provider
 * Provides hover documentation for builtins and keywords
 */
export class AetherShellHoverProvider implements vscode.HoverProvider {
    private readonly builtinDocs: Map<string, { signature: string; description: string; example?: string }>;

    constructor() {
        this.builtinDocs = new Map([
            // Core
            ['print', { signature: 'print(value) -> value', description: 'Print a value to stdout and return it.', example: 'print("Hello, world!")' }],
            ['echo', { signature: 'echo(value) -> value', description: 'Alias for print.', example: 'echo("Hello")' }],
            ['help', { signature: 'help() | help(topic)', description: 'Display help for builtins or a specific topic.' }],
            ['type_of', { signature: 'type_of(value) -> String', description: 'Get the type name of a value.', example: 'type_of(42) // "Int"' }],
            ['len', { signature: 'len(collection) -> Int', description: 'Get the length of a string, array, or record.', example: 'len([1, 2, 3]) // 3' }],
            ['keys', { signature: 'keys(record) -> Array', description: 'Get all keys from a record.', example: 'keys({a: 1, b: 2}) // ["a", "b"]' }],
            ['values', { signature: 'values(record) -> Array', description: 'Get all values from a record.', example: 'values({a: 1, b: 2}) // [1, 2]' }],

            // Functional
            ['map', { signature: 'map(array, fn) -> Array', description: 'Transform each element using a function.', example: '[1, 2, 3] | map(fn(x) => x * 2) // [2, 4, 6]' }],
            ['where', { signature: 'where(array, fn) -> Array', description: 'Filter elements that match a predicate.', example: '[1, 2, 3, 4] | where(fn(x) => x > 2) // [3, 4]' }],
            ['filter', { signature: 'filter(array, fn) -> Array', description: 'Alias for where.', example: '[1, 2, 3] | filter(fn(x) => x % 2 == 0)' }],
            ['reduce', { signature: 'reduce(array, fn, init) -> value', description: 'Reduce array to single value using accumulator.', example: '[1, 2, 3] | reduce(fn(a, b) => a + b, 0) // 6' }],
            ['take', { signature: 'take(array, n) -> Array', description: 'Take the first n elements.', example: '[1, 2, 3, 4, 5] | take(3) // [1, 2, 3]' }],
            ['skip', { signature: 'skip(array, n) -> Array', description: 'Skip the first n elements.', example: '[1, 2, 3, 4, 5] | skip(2) // [3, 4, 5]' }],
            ['first', { signature: 'first(array) -> value', description: 'Get the first element or null.', example: '[1, 2, 3] | first // 1' }],
            ['last', { signature: 'last(array) -> value', description: 'Get the last element or null.', example: '[1, 2, 3] | last // 3' }],
            ['any', { signature: 'any(array, fn) -> Bool', description: 'Check if any element matches predicate.', example: '[1, 2, 3] | any(fn(x) => x > 2) // true' }],
            ['all', { signature: 'all(array, fn) -> Bool', description: 'Check if all elements match predicate.', example: '[1, 2, 3] | all(fn(x) => x > 0) // true' }],

            // String
            ['split', { signature: 'split(str, delimiter) -> Array', description: 'Split string by delimiter.', example: '"a,b,c" | split(",") // ["a", "b", "c"]' }],
            ['join', { signature: 'join(array, delimiter) -> String', description: 'Join array elements with delimiter.', example: '["a", "b", "c"] | join("-") // "a-b-c"' }],
            ['trim', { signature: 'trim(str) -> String', description: 'Remove leading/trailing whitespace.', example: '"  hello  " | trim // "hello"' }],
            ['upper', { signature: 'upper(str) -> String', description: 'Convert to uppercase.', example: '"hello" | upper // "HELLO"' }],
            ['lower', { signature: 'lower(str) -> String', description: 'Convert to lowercase.', example: '"HELLO" | lower // "hello"' }],
            ['replace', { signature: 'replace(str, find, replacement) -> String', description: 'Replace occurrences.', example: '"hello" | replace("l", "L") // "heLLo"' }],
            ['contains', { signature: 'contains(str, substr) -> Bool', description: 'Check if string contains substring.', example: '"hello" | contains("ell") // true' }],
            ['starts_with', { signature: 'starts_with(str, prefix) -> Bool', description: 'Check if string starts with prefix.', example: '"hello" | starts_with("he") // true' }],
            ['ends_with', { signature: 'ends_with(str, suffix) -> Bool', description: 'Check if string ends with suffix.', example: '"hello" | ends_with("lo") // true' }],

            // Array
            ['flatten', { signature: 'flatten(array) -> Array', description: 'Flatten nested arrays one level.', example: '[[1, 2], [3, 4]] | flatten // [1, 2, 3, 4]' }],
            ['reverse', { signature: 'reverse(array) -> Array', description: 'Reverse array order.', example: '[1, 2, 3] | reverse // [3, 2, 1]' }],
            ['slice', { signature: 'slice(array, start, end) -> Array', description: 'Get a slice of the array.', example: '[1, 2, 3, 4, 5] | slice(1, 4) // [2, 3, 4]' }],
            ['range', { signature: 'range(start, end) -> Array', description: 'Generate integer range [start, end).', example: 'range(1, 5) // [1, 2, 3, 4]' }],
            ['zip', { signature: 'zip(array1, array2) -> Array', description: 'Pair elements from two arrays.', example: 'zip([1, 2], ["a", "b"]) // [[1, "a"], [2, "b"]]' }],
            ['push', { signature: 'push(array, value) -> Array', description: 'Append value to array.', example: '[1, 2] | push(3) // [1, 2, 3]' }],
            ['concat', { signature: 'concat(array1, array2) -> Array', description: 'Concatenate two arrays.', example: 'concat([1, 2], [3, 4]) // [1, 2, 3, 4]' }],
            ['sort', { signature: 'sort(array) -> Array', description: 'Sort array in ascending order.', example: '[3, 1, 2] | sort // [1, 2, 3]' }],
            ['unique', { signature: 'unique(array) -> Array', description: 'Remove duplicate elements.', example: '[1, 2, 2, 3] | unique // [1, 2, 3]' }],

            // Math
            ['abs', { signature: 'abs(n) -> Number', description: 'Absolute value.', example: 'abs(-5) // 5' }],
            ['min', { signature: 'min(a, b) | min(array) -> Number', description: 'Minimum of two values or array.', example: 'min(3, 7) // 3' }],
            ['max', { signature: 'max(a, b) | max(array) -> Number', description: 'Maximum of two values or array.', example: 'max(3, 7) // 7' }],
            ['sqrt', { signature: 'sqrt(n) -> Float', description: 'Square root.', example: 'sqrt(16) // 4.0' }],
            ['pow', { signature: 'pow(base, exp) -> Number', description: 'Exponentiation.', example: 'pow(2, 3) // 8' }],
            ['floor', { signature: 'floor(n) -> Int', description: 'Round down to integer.', example: 'floor(3.7) // 3' }],
            ['ceil', { signature: 'ceil(n) -> Int', description: 'Round up to integer.', example: 'ceil(3.2) // 4' }],
            ['round', { signature: 'round(n) -> Int', description: 'Round to nearest integer.', example: 'round(3.5) // 4' }],
            ['sum', { signature: 'sum(array) -> Number', description: 'Sum of numeric array.', example: '[1, 2, 3] | sum // 6' }],
            ['avg', { signature: 'avg(array) -> Float', description: 'Average of numeric array.', example: '[1, 2, 3] | avg // 2.0' }],
            ['product', { signature: 'product(array) -> Number', description: 'Product of numeric array.', example: '[1, 2, 3, 4] | product // 24' }],

            // Type conversion
            ['to_string', { signature: 'to_string(value) -> String', description: 'Convert value to string.', example: 'to_string(42) // "42"' }],
            ['to_int', { signature: 'to_int(value) -> Int', description: 'Convert to integer.', example: 'to_int("42") // 42' }],
            ['to_float', { signature: 'to_float(value) -> Float', description: 'Convert to float.', example: 'to_float("3.14") // 3.14' }],

            // File system
            ['ls', { signature: 'ls() | ls(path) -> Table', description: 'List directory contents as a table.', example: 'ls(".") | where(fn(r) => r.size > 1000)' }],
            ['cat', { signature: 'cat(path) -> String', description: 'Read file contents.', example: 'cat("README.md")' }],
            ['pwd', { signature: 'pwd() -> String', description: 'Get current working directory.', example: 'pwd()' }],
            ['cd', { signature: 'cd(path) -> null', description: 'Change current directory.', example: 'cd("src")' }],
            ['exists', { signature: 'exists(path) -> Bool', description: 'Check if path exists.', example: 'exists("file.txt")' }],
            ['mkdir', { signature: 'mkdir(path) -> null', description: 'Create directory.', example: 'mkdir("new_folder")' }],
            ['rm', { signature: 'rm(path) -> null', description: 'Remove file or directory.', example: 'rm("old_file.txt")' }],

            // AI
            ['ai', { signature: 'ai(prompt) | ai(prompt, model) -> String', description: 'Query an AI model.', example: 'ai("Explain recursion")' }],
            ['agent', { signature: 'agent(goal, tools?, options?) -> AgentResult', description: 'Create an autonomous AI agent.', example: 'agent("List Python files", ["ls", "cat"])' }],

            // OS
            ['env', { signature: 'env(name) -> String', description: 'Get environment variable.', example: 'env("HOME")' }],
            ['which', { signature: 'which(cmd) -> String', description: 'Find command location.', example: 'which("git")' }],
            ['os', { signature: 'os() -> String', description: 'Get operating system name.', example: 'os() // "windows"' }],
            ['arch', { signature: 'arch() -> String', description: 'Get CPU architecture.', example: 'arch() // "x86_64"' }],
            ['hostname', { signature: 'hostname() -> String', description: 'Get system hostname.', example: 'hostname()' }],
        ]);
    }

    provideHover(
        document: vscode.TextDocument,
        position: vscode.Position,
        _token: vscode.CancellationToken
    ): vscode.Hover | null {
        const range = document.getWordRangeAtPosition(position);
        if (!range) {
            return null;
        }

        const word = document.getText(range);

        // Check builtins
        const builtin = this.builtinDocs.get(word);
        if (builtin) {
            const md = new vscode.MarkdownString();
            md.appendCodeblock(builtin.signature, 'aethershell');
            md.appendMarkdown(`\n\n${builtin.description}`);
            if (builtin.example) {
                md.appendMarkdown(`\n\n**Example:**`);
                md.appendCodeblock(builtin.example, 'aethershell');
            }
            return new vscode.Hover(md, range);
        }

        // Check keywords
        const keywords: Record<string, string> = {
            'let': 'Declare a variable binding.\n\n```aethershell\nlet name = value\n```',
            'fn': 'Lambda (anonymous function) expression.\n\n```aethershell\nfn(x, y) => x + y\n```',
            'match': 'Pattern matching expression.\n\n```aethershell\nmatch value {\n  pattern => result,\n  _ => default\n}\n```',
            'if': 'Conditional expression.\n\n```aethershell\nif condition { then_value } else { else_value }\n```',
            'true': 'Boolean true literal.',
            'false': 'Boolean false literal.',
            'null': 'Null value literal.',
        };

        if (keywords[word]) {
            const md = new vscode.MarkdownString();
            md.appendMarkdown(`**${word}** — AetherShell keyword\n\n${keywords[word]}`);
            return new vscode.Hover(md, range);
        }

        return null;
    }
}

/**
 * Register all AetherShell language providers
 */
export function registerProviders(context: vscode.ExtensionContext): void {
    const selector: vscode.DocumentSelector = { language: 'aethershell', scheme: '*' };

    // Register symbol provider for outline view
    context.subscriptions.push(
        vscode.languages.registerDocumentSymbolProvider(
            selector,
            new AetherShellSymbolProvider()
        )
    );

    // Register folding provider
    context.subscriptions.push(
        vscode.languages.registerFoldingRangeProvider(
            selector,
            new AetherShellFoldingProvider()
        )
    );

    // Register hover provider
    context.subscriptions.push(
        vscode.languages.registerHoverProvider(
            selector,
            new AetherShellHoverProvider()
        )
    );
}
