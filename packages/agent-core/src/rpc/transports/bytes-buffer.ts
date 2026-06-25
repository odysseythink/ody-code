export class BytesBuffer {
  private buffer = new Uint8Array(1024);
  private size = 0;

  append(chunk: Uint8Array): void {
    if (this.size + chunk.length > this.buffer.length) {
      let newCapacity = this.buffer.length * 2;
      while (newCapacity < this.size + chunk.length) {
        newCapacity *= 2;
      }
      const newBuffer = new Uint8Array(newCapacity);
      newBuffer.set(this.buffer.subarray(0, this.size));
      this.buffer = newBuffer;
    }
    this.buffer.set(chunk, this.size);
    this.size += chunk.length;
  }

  get length(): number {
    return this.size;
  }

  indexOf(byte: number): number {
    for (let i = 0; i < this.size; i++) {
      if (this.buffer[i] === byte) return i;
    }
    return -1;
  }

  slice(start: number, end: number): Uint8Array {
    return this.buffer.slice(start, end);
  }

  discard(count: number): void {
    if (count >= this.size) {
      this.size = 0;
      return;
    }
    this.buffer.copyWithin(0, count, this.size);
    this.size -= count;
  }
}
