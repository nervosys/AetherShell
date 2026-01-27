// TypeScript definitions for @nervosys/aethershell
// WebAssembly bindings for AetherShell - AI-powered typed shell

/**
 * Initialize the WASM module. Must be called before using any other functions.
 * @example
 * ```typescript
 * import init, { AetherShell } from '@nervosys/aethershell';
 * await init();
 * ```
 */
export default function init(): Promise<void>;

// ============================================================================
// Standalone Functions (use global environment)
// ============================================================================

/**
 * Get the WASM module version
 * @returns Version string like "ae-wasm 0.2.0"
 */
export function ae_version(): string;

/**
 * Reset the global environment to default state
 */
export function ae_reset(): void;

/**
 * Evaluate AetherShell code using the global environment
 * @param line - AetherShell code to evaluate
 * @returns String representation of the result
 * @example
 * ```typescript
 * ae_eval('let x = 42');
 * ae_eval('x * 2'); // "84"
 * ```
 */
export function ae_eval(line: string): string;

/**
 * Evaluate AetherShell code and return JSON result
 * @param line - AetherShell code to evaluate
 * @returns JSON string of the result, or `{"error":"..."}` on failure
 * @example
 * ```typescript
 * ae_eval_json('[1,2,3] | map(fn(x) => x * 2)'); // "[2,4,6]"
 * ```
 */
export function ae_eval_json(line: string): string;

/**
 * Get a variable from the global environment as JSON
 * @param name - Variable name
 * @returns JSON string of the value, or "null" if not found
 */
export function ae_get_var(name: string): string;

/**
 * Set a variable in the global environment from JSON
 * @param name - Variable name
 * @param json_value - JSON string value to set
 * @returns true if successful, false on parse error
 */
export function ae_set_var(name: string, json_value: string): boolean;

/**
 * List all available builtin functions
 * @returns JSON array of builtin names
 */
export function ae_list_builtins(): string;

// ============================================================================
// A2UI Event Queue
// ============================================================================

/**
 * Poll for the next A2UI event from the queue
 * @returns JSON string of the event, or undefined if queue is empty
 */
export function ae_poll_event(): string | undefined;

/**
 * Get the number of pending A2UI events
 * @returns Number of events in the queue
 */
export function ae_event_count(): number;

/**
 * Clear all pending A2UI events
 */
export function ae_clear_events(): void;

// ============================================================================
// AetherShell Class
// ============================================================================

/**
 * AetherShell instance with its own isolated environment.
 * Use this for multiple independent shell contexts.
 * 
 * @example
 * ```typescript
 * const shell = new AetherShell();
 * shell.eval('let data = [1, 2, 3]');
 * const result = shell.evalJson('data | map(fn(x) => x * 2)');
 * console.log(JSON.parse(result)); // [2, 4, 6]
 * ```
 */
export class AetherShell {
    /**
     * Create a new AetherShell instance with a fresh environment
     */
    constructor();

    /**
     * Evaluate AetherShell code and return string result
     * @param code - AetherShell code to evaluate
     * @returns String representation of the result
     */
    eval(code: string): string;

    /**
     * Evaluate AetherShell code and return JSON result
     * @param code - AetherShell code to evaluate
     * @returns JSON string of the result
     */
    evalJson(code: string): string;

    /**
     * Execute a pipeline on input data
     * @param input_json - JSON string of input data
     * @param operations - Pipeline operations (e.g., "map(fn(x) => x * 2) | filter(fn(x) => x > 2)")
     * @returns JSON string of the result
     * @example
     * ```typescript
     * shell.pipe('[1,2,3]', 'map(fn(x) => x * 2)'); // "[2,4,6]"
     * ```
     */
    pipe(input_json: string, operations: string): string;

    /**
     * Get a variable from the environment
     * @param name - Variable name
     * @returns JSON string of the value
     */
    getVar(name: string): string;

    /**
     * Set a variable in the environment
     * @param name - Variable name
     * @param json_value - JSON string value
     * @returns true if successful
     */
    setVar(name: string, json_value: string): boolean;

    /**
     * Reset the environment to default state
     */
    reset(): void;

    /**
     * Get the version string
     */
    version(): string;
}

// ============================================================================
// Value Types (for documentation)
// ============================================================================

/**
 * AetherShell Value types (when parsed from JSON)
 */
export type AetherValue =
    | null
    | boolean
    | number
    | string
    | AetherValue[]
    | { [key: string]: AetherValue }
    | AetherTable;

/**
 * Table structure returned by table operations
 */
export interface AetherTable {
    columns: string[];
    rows: AetherValue[];
}

/**
 * A2UI Event structure
 */
export interface A2UIEvent {
    id: string;
    timestamp: string;
    priority: 'low' | 'normal' | 'high' | 'critical';
    event_type: A2UIEventType;
}

export type A2UIEventType =
    | { type: 'Notify'; message: string; level: NotificationLevel }
    | { type: 'Toast'; message: string; level: NotificationLevel; duration_ms: number }
    | { type: 'Progress'; id: string; label: string; current: number; total: number; status: string }
    | { type: 'ProgressComplete'; id: string; success: boolean; message?: string }
    | { type: 'Prompt'; prompt_id: string; message: string; prompt_type: PromptType }
    | { type: 'Render'; content: RenderContent }
    | { type: 'Clear'; target?: string }
    | { type: 'Status'; text: string; icon?: string }
    | { type: 'AgentStarted'; agent_id: string; task?: string }
    | { type: 'AgentCompleted'; agent_id: string; success: boolean; result?: string }
    | { type: 'AgentThinking'; agent_id: string; thought: string; step: number };

export type NotificationLevel = 'info' | 'success' | 'warning' | 'error';

export type PromptType =
    | { type: 'Text'; default?: string; placeholder?: string }
    | { type: 'Confirm' }
    | { type: 'Select'; options: string[] };

export type RenderContent =
    | { type: 'Text'; text: string }
    | { type: 'Markdown'; markdown: string }
    | { type: 'Code'; code: string; language?: string }
    | { type: 'Table'; columns: string[]; rows: string[][] }
    | { type: 'Thinking'; thought: string; step: number };

// ============================================================================
// Helper Functions (to be implemented in wrapper)
// ============================================================================

/**
 * Helper to evaluate and parse result as typed value
 */
export function evalTyped<T = AetherValue>(shell: AetherShell, code: string): T;

/**
 * Subscribe to A2UI events with callback
 */
export function subscribeA2UI(callback: (event: A2UIEvent) => void): () => void;
