/* tslint:disable */
/* eslint-disable */

/**
 * AetherShell WASM runtime
 */
export class AetherWasm {
    free(): void;
    [Symbol.dispose](): void;
    builtins(): any[];
    eval(code: string): string;
    eval_display(code: string): string;
    get_var(name: string): string;
    constructor();
    parse(code: string): string;
    reset(): void;
    set_var(name: string, json: string): void;
    variables(): any[];
    version(): string;
}

export function ae_eval(code: string): string;

export function ae_parse(code: string): string;

export function ae_version(): string;

/**
 * Initialize WASM module
 */
export function init(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_aetherwasm_free: (a: number, b: number) => void;
    readonly ae_eval: (a: number, b: number) => [number, number, number, number];
    readonly ae_parse: (a: number, b: number) => [number, number, number, number];
    readonly ae_version: () => [number, number];
    readonly aetherwasm_builtins: (a: number) => [number, number];
    readonly aetherwasm_eval: (a: number, b: number, c: number) => [number, number, number, number];
    readonly aetherwasm_eval_display: (a: number, b: number, c: number) => [number, number, number, number];
    readonly aetherwasm_get_var: (a: number, b: number, c: number) => [number, number, number, number];
    readonly aetherwasm_new: () => number;
    readonly aetherwasm_parse: (a: number, b: number, c: number) => [number, number, number, number];
    readonly aetherwasm_reset: (a: number) => void;
    readonly aetherwasm_set_var: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly aetherwasm_variables: (a: number) => [number, number];
    readonly aetherwasm_version: (a: number) => [number, number];
    readonly init: () => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
