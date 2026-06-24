/** Augment global types for WebAssembly when not available via lib. */
interface WebAssemblyMemory {
  readonly buffer: ArrayBuffer;
  grow(delta: number): number;
}

declare namespace WebAssembly {
  class Memory {
    constructor(descriptor: { initial: number; maximum?: number; shared?: boolean });
    readonly buffer: ArrayBuffer;
    grow(delta: number): number;
  }
  function instantiate(
    bytes: BufferSource,
    importObject?: Record<string, unknown>,
  ): Promise<{ instance: WebAssembly.Instance; module: WebAssembly.Module }>;
  interface Instance {
    readonly exports: Record<string, unknown>;
  }
  interface Module {}
}
