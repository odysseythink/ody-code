// Minimal WebAssembly declarations for the project's non-DOM build target.
// The agent-core-shared wasm-loader relies on WebAssembly types.
declare namespace WebAssembly {
  interface Memory {
    readonly buffer: ArrayBuffer;
    grow(delta: number): number;
  }
  interface Instance {
    readonly exports: Record<string, unknown>;
  }
  interface Module {}
  interface CompileError extends Error {}
  interface LinkError extends Error {}
  interface RuntimeError extends Error {}
  interface Global {
    value: unknown;
    valueOf(): unknown;
  }
  interface Table {
    readonly length: number;
    get(index: number): unknown | null;
    grow(delta: number, value?: unknown): number;
    set(index: number, value?: unknown): void;
  }
  function instantiate(
    bytes: BufferSource,
    importObject?: Record<string, unknown>,
  ): Promise<{ module: Module; instance: Instance }>;
  function compile(bytes: BufferSource): Promise<Module>;
  function validate(bytes: BufferSource): boolean;
}
