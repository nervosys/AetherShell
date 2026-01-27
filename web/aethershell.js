/**
 * @nervosys/aethershell - JavaScript/TypeScript Wrapper
 * 
 * High-level API wrapper around the WASM bindings.
 * Provides async/await support and typed helpers.
 */

// Re-export WASM bindings
export * from './pkg/aether_wasm.js';
import init, * as wasm from './pkg/aether_wasm.js';

/**
 * Initialize AetherShell WASM module
 * @param wasmUrl - Optional URL to the .wasm file
 */
export async function initAetherShell(wasmUrl) {
    await init(wasmUrl);
    return {
        version: wasm.ae_version(),
        ready: true
    };
}

/**
 * Evaluate AetherShell code and return parsed result
 * @param {string} code - AetherShell code to evaluate
 * @returns {any} Parsed JavaScript value
 */
export function evaluate(code) {
    const json = wasm.ae_eval_json(code);
    try {
        const result = JSON.parse(json);
        if (result && typeof result === 'object' && 'error' in result) {
            throw new Error(result.error);
        }
        return result;
    } catch (e) {
        if (e instanceof SyntaxError) {
            return json; // Return raw string if not valid JSON
        }
        throw e;
    }
}

/**
 * Enhanced AetherShell class with async support
 */
export class AetherShellAsync {
    constructor() {
        this._shell = new wasm.AetherShell();
        this._eventListeners = [];
        this._pollInterval = null;
    }

    /**
     * Evaluate code and return parsed result
     * @param {string} code 
     * @returns {any}
     */
    eval(code) {
        const json = this._shell.evalJson(code);
        return JSON.parse(json);
    }

    /**
     * Execute code with timeout (simulated async)
     * @param {string} code 
     * @param {number} timeoutMs 
     * @returns {Promise<any>}
     */
    async evalAsync(code, timeoutMs = 30000) {
        return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                reject(new Error('Execution timeout'));
            }, timeoutMs);

            try {
                const result = this.eval(code);
                clearTimeout(timeout);
                resolve(result);
            } catch (e) {
                clearTimeout(timeout);
                reject(e);
            }
        });
    }

    /**
     * Execute a pipeline on data
     * @param {any} input - Input data (will be JSON stringified)
     * @param {string} operations - Pipeline operations
     * @returns {any}
     */
    pipe(input, operations) {
        const inputJson = typeof input === 'string' ? input : JSON.stringify(input);
        const result = this._shell.pipe(inputJson, operations);
        return JSON.parse(result);
    }

    /**
     * Get a variable's value
     * @param {string} name 
     * @returns {any}
     */
    get(name) {
        const json = this._shell.getVar(name);
        return JSON.parse(json);
    }

    /**
     * Set a variable's value
     * @param {string} name 
     * @param {any} value 
     */
    set(name, value) {
        const json = typeof value === 'string' ? `"${value}"` : JSON.stringify(value);
        this._shell.setVar(name, json);
    }

    /**
     * Subscribe to A2UI events
     * @param {function} callback - Called with each event
     * @returns {function} Unsubscribe function
     */
    onEvent(callback) {
        this._eventListeners.push(callback);

        // Start polling if not already
        if (!this._pollInterval) {
            this._pollInterval = setInterval(() => {
                let event;
                while ((event = wasm.ae_poll_event())) {
                    try {
                        const parsed = JSON.parse(event);
                        this._eventListeners.forEach(cb => cb(parsed));
                    } catch (e) {
                        console.error('Failed to parse A2UI event:', e);
                    }
                }
            }, 100);
        }

        // Return unsubscribe function
        return () => {
            const idx = this._eventListeners.indexOf(callback);
            if (idx !== -1) {
                this._eventListeners.splice(idx, 1);
            }
            if (this._eventListeners.length === 0 && this._pollInterval) {
                clearInterval(this._pollInterval);
                this._pollInterval = null;
            }
        };
    }

    /**
     * Subscribe to specific event types
     * @param {string} eventType - e.g., 'Notify', 'Progress', 'AgentThinking'
     * @param {function} callback 
     * @returns {function} Unsubscribe function
     */
    on(eventType, callback) {
        return this.onEvent(event => {
            if (event.event_type?.type === eventType) {
                callback(event);
            }
        });
    }

    /**
     * Reset the environment
     */
    reset() {
        this._shell.reset();
    }

    /**
     * Clean up resources
     */
    dispose() {
        if (this._pollInterval) {
            clearInterval(this._pollInterval);
        }
        this._eventListeners = [];
    }
}

/**
 * Create a pipeline builder for fluent API
 * @param {any} input - Initial input value
 * @returns {PipelineBuilder}
 */
export function pipeline(input) {
    return new PipelineBuilder(input);
}

class PipelineBuilder {
    constructor(input) {
        this._input = input;
        this._operations = [];
    }

    map(fn) {
        this._operations.push(`map(${fn})`);
        return this;
    }

    filter(fn) {
        this._operations.push(`filter(${fn})`);
        return this;
    }

    reduce(fn, initial) {
        this._operations.push(`reduce(${fn}, ${JSON.stringify(initial)})`);
        return this;
    }

    sort(fn) {
        this._operations.push(fn ? `sort(${fn})` : 'sort()');
        return this;
    }

    reverse() {
        this._operations.push('reverse()');
        return this;
    }

    flatten() {
        this._operations.push('flatten()');
        return this;
    }

    unique() {
        this._operations.push('unique()');
        return this;
    }

    take(n) {
        this._operations.push(`slice(0, ${n})`);
        return this;
    }

    skip(n) {
        this._operations.push(`slice(${n})`);
        return this;
    }

    select(...fields) {
        this._operations.push(`select(${fields.map(f => `"${f}"`).join(', ')})`);
        return this;
    }

    where(fn) {
        this._operations.push(`where(${fn})`);
        return this;
    }

    /**
     * Execute the pipeline
     * @param {AetherShellAsync} shell - Shell instance to use
     * @returns {any}
     */
    run(shell) {
        const ops = this._operations.join(' | ');
        return shell.pipe(this._input, ops);
    }

    /**
     * Get the pipeline as AetherShell code
     * @returns {string}
     */
    toCode() {
        const inputStr = JSON.stringify(this._input);
        return `${inputStr} | ${this._operations.join(' | ')}`;
    }
}

// Default export for convenience
export default {
    init: initAetherShell,
    evaluate,
    pipeline,
    AetherShell: wasm.AetherShell,
    AetherShellAsync
};
