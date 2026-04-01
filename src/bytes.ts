const UTF8_ENCODER = new TextEncoder();

export function utf8Bytes(value: string): Uint8Array {
  return UTF8_ENCODER.encode(value);
}

export function utf8ByteLength(value: string): number {
  return utf8Bytes(value).length;
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) {
    throw new Error("hex string length must be even");
  }

  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < hex.length; index += 2) {
    const value = Number.parseInt(hex.slice(index, index + 2), 16);
    if (Number.isNaN(value)) {
      throw new Error("hex string contains invalid characters");
    }
    bytes[index / 2] = value;
  }
  return bytes;
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
