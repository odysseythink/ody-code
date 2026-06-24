// WebAssembly is available globally in Node.js v22+ but is a DOM API not
// included in the ES2023 lib. This minimal declaration makes the type system
// aware of the subset we consume.
declare namespace WebAssembly {
  interface MemoryDescriptor {
    initial: number;
    maximum?: number;
    shared?: boolean;
  }
  interface Memory {
    readonly buffer: ArrayBuffer;
    grow(delta: number): number;
  }
  const Memory: {
    new (descriptor: MemoryDescriptor): Memory;
    prototype: Memory;
  };
  interface Instance {
    readonly exports: Record<string, unknown>;
  }
  interface ResultObject {
    readonly instance: Instance;
  }
  function instantiate(bytes: BufferSource, importObject?: object): Promise<ResultObject>;
}
