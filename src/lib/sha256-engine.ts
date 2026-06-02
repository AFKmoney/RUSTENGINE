/**
 * SHA-256 Compression Engine with Round-by-Round State Capture
 * 
 * This implements the SHA-256 compression function with the ability
 * to capture intermediate state (a,b,c,d,e,f,g,h) at every round.
 * 
 * We do NOT implement full SHA-256 padding/hashing here — we focus
 * on the compression function that processes a single 512-bit block.
 */

// SHA-256 initial hash values (first 32 bits of fractional parts of square roots of first 8 primes)
const H_INIT = [
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
  0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

// SHA-256 round constants (first 32 bits of fractional parts of cube roots of first 64 primes)
const K: number[] = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
  0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
  0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
  0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
  0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
  0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
  0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
  0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
  0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

// Bit rotation and shift helpers (32-bit unsigned)
function rotr(x: number, n: number): number {
  return ((x >>> n) | (x << (32 - n))) >>> 0;
}

function shr(x: number, n: number): number {
  return x >>> n;
}

// SHA-256 logical functions
function ch(x: number, y: number, z: number): number {
  return ((x & y) ^ (~x & z)) >>> 0;
}

function maj(x: number, y: number, z: number): number {
  return ((x & y) ^ (x & z) ^ (y & z)) >>> 0;
}

function bigSigma0(x: number): number {
  return (rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22)) >>> 0;
}

function bigSigma1(x: number): number {
  return (rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25)) >>> 0;
}

function smallSigma0(x: number): number {
  return (rotr(x, 7) ^ rotr(x, 18) ^ shr(x, 3)) >>> 0;
}

function smallSigma1(x: number): number {
  return (rotr(x, 17) ^ rotr(x, 19) ^ shr(x, 10)) >>> 0;
}

export interface RoundState {
  round: number;
  a: number; b: number; c: number; d: number;
  e: number; f: number; g: number; h: number;
  T1: number;
  T2: number;
}

export interface CompressionTrace {
  rounds: RoundState[];
  finalState: number[];
  messageSchedule: number[];
}

/**
 * Compress a 512-bit (64-byte) block with SHA-256, capturing state at every round.
 * 
 * @param block - 64-byte input block
 * @param hashState - Optional initial hash state (8 x 32-bit words). Defaults to SHA-256 IV.
 * @returns CompressionTrace with all 64 round states and the final hash state
 */
export function compressBlock(
  block: Uint8Array,
  hashState?: number[]
): CompressionTrace {
  if (block.length !== 64) {
    throw new Error(`Block must be 64 bytes, got ${block.length}`);
  }

  // Parse block into 16 x 32-bit big-endian words
  const M: number[] = [];
  for (let i = 0; i < 16; i++) {
    M[i] =
      ((block[i * 4] << 24) |
        (block[i * 4 + 1] << 16) |
        (block[i * 4 + 2] << 8) |
        block[i * 4 + 3]) >>>
      0;
  }

  // Prepare message schedule W[0..63]
  const W: number[] = new Array(64);
  for (let t = 0; t < 16; t++) {
    W[t] = M[t];
  }
  for (let t = 16; t < 64; t++) {
    W[t] = (smallSigma1(W[t - 2]) + W[t - 7] + smallSigma0(W[t - 15]) + W[t - 16]) >>> 0;
  }

  // Initialize working variables from hash state
  const h0 = hashState || H_INIT;
  let a = h0[0] >>> 0;
  let b = h0[1] >>> 0;
  let c = h0[2] >>> 0;
  let d = h0[3] >>> 0;
  let e = h0[4] >>> 0;
  let f = h0[5] >>> 0;
  let g = h0[6] >>> 0;
  let h = h0[7] >>> 0;

  const rounds: RoundState[] = [];

  // 64 rounds of compression
  for (let t = 0; t < 64; t++) {
    const T1 = (h + bigSigma1(e) + ch(e, f, g) + K[t] + W[t]) >>> 0;
    const T2 = (bigSigma0(a) + maj(a, b, c)) >>> 0;

    h = g;
    g = f;
    f = e;
    e = (d + T1) >>> 0;
    d = c;
    c = b;
    b = a;
    a = (T1 + T2) >>> 0;

    rounds.push({
      round: t,
      a, b, c, d, e, f, g, h,
      T1, T2,
    });
  }

  // Compute final hash state (add back to initial)
  const finalState = [
    (h0[0] + a) >>> 0,
    (h0[1] + b) >>> 0,
    (h0[2] + c) >>> 0,
    (h0[3] + d) >>> 0,
    (h0[4] + e) >>> 0,
    (h0[5] + f) >>> 0,
    (h0[6] + g) >>> 0,
    (h0[7] + h) >>> 0,
  ];

  return { rounds, finalState, messageSchedule: W };
}

/**
 * Full SHA-256 hash for a message (with proper padding).
 * Used for verifying correctness against known test vectors.
 */
export function sha256Full(message: Uint8Array): number[] {
  // SHA-256 padding
  const msgLen = message.length;
  const bitLen = msgLen * 8;

  // Padded message length must be a multiple of 512 bits (64 bytes)
  // We need at least: msgLen + 1 + 8 bytes
  let paddedLen = msgLen + 1;
  while (paddedLen % 64 !== 56) {
    paddedLen++;
  }
  paddedLen += 8; // 64-bit length

  const padded = new Uint8Array(paddedLen);
  padded.set(message);
  padded[msgLen] = 0x80; // Append 1 bit

  // Append length in bits as big-endian 64-bit integer
  // JavaScript numbers can't hold 64-bit precisely, but for messages < 2^32 bits it's fine
  const lenHi = Math.floor(bitLen / 0x100000000);
  const lenLo = bitLen >>> 0;
  padded[paddedLen - 8] = (lenHi >>> 24) & 0xff;
  padded[paddedLen - 7] = (lenHi >>> 16) & 0xff;
  padded[paddedLen - 6] = (lenHi >>> 8) & 0xff;
  padded[paddedLen - 5] = lenHi & 0xff;
  padded[paddedLen - 4] = (lenLo >>> 24) & 0xff;
  padded[paddedLen - 3] = (lenLo >>> 16) & 0xff;
  padded[paddedLen - 2] = (lenLo >>> 8) & 0xff;
  padded[paddedLen - 1] = lenLo & 0xff;

  // Process each 64-byte block
  let hashState = [...H_INIT];
  for (let i = 0; i < paddedLen; i += 64) {
    const block = padded.slice(i, i + 64);
    const trace = compressBlock(block, hashState);
    hashState = trace.finalState;
  }

  return hashState;
}

/**
 * Convert hash state (8 x 32-bit words) to hex string
 */
export function hashToHex(state: number[]): string {
  return state
    .map((w) => (w >>> 0).toString(16).padStart(8, "0"))
    .join("");
}

/**
 * Verify SHA-256 implementation against known test vectors.
 * Returns true if all tests pass.
 */
export function verifySha256(): { passed: boolean; results: { input: string; expected: string; got: string; ok: boolean }[] } {
  const testVectors = [
    {
      input: "",
      expected: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    },
    {
      input: "abc",
      expected: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    },
    {
      input: "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
      expected: "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    },
  ];

  const results = testVectors.map((tv) => {
    const input = new TextEncoder().encode(tv.input);
    const hash = sha256Full(input);
    const got = hashToHex(hash);
    return {
      input: tv.input || "(empty string)",
      expected: tv.expected,
      got,
      ok: got === tv.expected,
    };
  });

  return {
    passed: results.every((r) => r.ok),
    results,
  };
}

/**
 * Flip a single bit in a Uint8Array at the given bit index.
 * Returns a new Uint8Array with the bit flipped.
 */
export function flipBit(data: Uint8Array, bitIndex: number): Uint8Array {
  const copy = new Uint8Array(data);
  const byteIndex = Math.floor(bitIndex / 8);
  const bitOffset = 7 - (bitIndex % 8); // MSB first
  copy[byteIndex] ^= 1 << bitOffset;
  return copy;
}

/**
 * Get the value of a specific bit in a Uint8Array.
 */
export function getBit(data: Uint8Array, bitIndex: number): 0 | 1 {
  const byteIndex = Math.floor(bitIndex / 8);
  const bitOffset = 7 - (bitIndex % 8);
  return ((data[byteIndex] >> bitOffset) & 1) as 0 | 1;
}

/**
 * Get the value of a specific bit in a 32-bit word.
 */
export function getWordBit(word: number, bitIndex: number): 0 | 1 {
  return ((word >>> (31 - bitIndex)) & 1) as 0 | 1;
}
