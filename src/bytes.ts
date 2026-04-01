const UTF8_ENCODER = new TextEncoder();

export function utf8Bytes(value: string): Uint8Array {
  return UTF8_ENCODER.encode(value);
}

export function utf8ByteLength(value: string): number {
  return utf8Bytes(value).length;
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
