/**
 * RIPEMD-160 Implementation in Pure TypeScript
 *
 * Needed for Bitcoin address verification:
 * address = Base58Check(0x00 + RIPEMD-160(SHA-256(pubkey)))
 *
 * Reference: https://homes.esat.kuleuven.be/~bosselae/ripemd160.html
 * This implementation handles messages up to ~2^64 bits.
 */

// Initial hash values
const H0 = 0x67452301;
const H1 = 0xEFCDAB89;
const H2 = 0x98BADCFE;
const H3 = 0x10325476;
const H4 = 0xC3D2E1F0;

// Left line round constants
const KL = [0x00000000, 0x5A827999, 0x6ED9EBA1, 0x8F1BBCDC, 0xA953FD4E];
// Right line round constants
const KR = [0x50A28BE6, 0x5C4DD124, 0x6D703EF3, 0x7A6D76E9, 0x00000000];

// Left line message word selection
const RL = [
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,  // Round 1
  7, 4, 13, 1, 10, 6, 15, 3, 12, 0, 9, 5, 2, 14, 11, 8,  // Round 2
  3, 10, 14, 4, 9, 15, 8, 1, 2, 7, 0, 6, 13, 11, 5, 12,  // Round 3
  1, 9, 11, 10, 0, 8, 12, 4, 13, 3, 7, 15, 14, 5, 6, 2,  // Round 4
  4, 0, 5, 9, 7, 12, 2, 10, 14, 1, 3, 8, 11, 6, 15, 13,  // Round 5
];

// Right line message word selection
const RR = [
  5, 14, 7, 0, 9, 2, 11, 4, 13, 6, 15, 8, 1, 10, 3, 12,  // Round 1
  6, 11, 3, 7, 0, 13, 5, 10, 14, 15, 8, 12, 4, 9, 1, 2,  // Round 2
  15, 5, 1, 3, 7, 14, 6, 9, 11, 8, 12, 2, 10, 0, 4, 13,  // Round 3
  8, 6, 4, 1, 3, 11, 15, 0, 5, 12, 2, 13, 9, 7, 10, 14,  // Round 4
  12, 15, 10, 4, 1, 5, 8, 7, 6, 2, 13, 14, 0, 3, 9, 11,  // Round 5
];

// Left line rotation amounts
const SL = [
  11, 14, 15, 12, 5, 8, 7, 9, 11, 13, 14, 15, 6, 7, 9, 8,  // Round 1
  7, 6, 8, 13, 11, 9, 7, 15, 7, 12, 15, 9, 11, 7, 13, 12,  // Round 2
  11, 13, 6, 7, 14, 9, 13, 15, 14, 8, 13, 6, 5, 12, 7, 5,  // Round 3
  11, 12, 14, 15, 14, 15, 9, 8, 9, 14, 5, 6, 8, 6, 5, 12,  // Round 4
  9, 15, 5, 11, 6, 8, 13, 12, 5, 12, 13, 14, 11, 8, 5, 6,  // Round 5
];

// Right line rotation amounts
const SR = [
  8, 9, 9, 11, 13, 15, 15, 5, 7, 7, 8, 11, 14, 14, 12, 6,  // Round 1
  9, 13, 15, 7, 12, 8, 9, 11, 7, 7, 12, 7, 6, 15, 13, 11,  // Round 2
  9, 7, 15, 11, 8, 6, 6, 14, 12, 13, 5, 14, 13, 13, 7, 5,  // Round 3
  15, 5, 8, 11, 14, 14, 6, 14, 6, 9, 12, 9, 12, 5, 15, 8,  // Round 4
  8, 5, 12, 9, 12, 5, 14, 6, 8, 13, 6, 5, 15, 13, 11, 11,  // Round 5
];

// Boolean functions for each round
function f(j: number, x: number, y: number, z: number): number {
  if (j <= 15) return x ^ y ^ z;
  if (j <= 31) return (x & y) | (~x & z);
  if (j <= 47) return (x | ~y) ^ z;
  if (j <= 63) return (x & z) | (y & ~z);
  return x ^ (y | ~z);
}

// Circular left shift
function rotl(x: number, n: number): number {
  return ((x << n) | (x >>> (32 - n))) >>> 0;
}

/**
 * Compute RIPEMD-160 hash of input data
 * @param data - Input data as Uint8Array
 * @returns 20-byte RIPEMD-160 hash as Uint8Array
 */
export function ripemd160(data: Uint8Array): Uint8Array {
  // Pre-processing: adding padding bits
  const msgLen = data.length;
  const bitLen = msgLen * 8;

  // Padded message: data + 0x80 + zeros + 8-byte length
  let paddedLen = msgLen + 1;
  while (paddedLen % 64 !== 56) {
    paddedLen++;
  }
  paddedLen += 8;

  const padded = new Uint8Array(paddedLen);
  padded.set(data);
  padded[msgLen] = 0x80;

  // Append length in bits as little-endian 64-bit
  // For messages < 2^32 bits, the high word is 0
  const lenLo = bitLen >>> 0;
  padded[paddedLen - 8] = lenLo & 0xff;
  padded[paddedLen - 7] = (lenLo >>> 8) & 0xff;
  padded[paddedLen - 6] = (lenLo >>> 16) & 0xff;
  padded[paddedLen - 5] = (lenLo >>> 24) & 0xff;
  padded[paddedLen - 4] = 0;
  padded[paddedLen - 3] = 0;
  padded[paddedLen - 2] = 0;
  padded[paddedLen - 1] = 0;

  // Initialize hash values
  let h0 = H0 >>> 0;
  let h1 = H1 >>> 0;
  let h2 = H2 >>> 0;
  let h3 = H3 >>> 0;
  let h4 = H4 >>> 0;

  // Process each 64-byte block
  for (let offset = 0; offset < paddedLen; offset += 64) {
    // Parse block into 16 x 32-bit little-endian words
    const X = new Array(16);
    for (let i = 0; i < 16; i++) {
      X[i] = (
        (padded[offset + i * 4]) |
        (padded[offset + i * 4 + 1] << 8) |
        (padded[offset + i * 4 + 2] << 16) |
        (padded[offset + i * 4 + 3] << 24)
      ) >>> 0;
    }

    // Initialize working variables
    let al = h0, bl = h1, cl = h2, dl = h3, el = h4;
    let ar = h0, br = h1, cr = h2, dr = h3, er = h4;

    // 80 rounds
    for (let j = 0; j < 80; j++) {
      const round = Math.floor(j / 16);

      // Left line
      const fl = f(j, bl, cl, dl) >>> 0;
      const tl = ((al + fl + X[RL[j]] + KL[round]) >>> 0);
      const tempL = ((el + rotl(tl, SL[j])) >>> 0);
      al = el;
      el = dl;
      dl = rotl(cl, 10);
      cl = bl;
      bl = tempL;

      // Right line
      const fr = f(79 - j, br, cr, dr) >>> 0;
      const tr = ((ar + fr + X[RR[j]] + KR[round]) >>> 0);
      const tempR = ((er + rotl(tr, SR[j])) >>> 0);
      ar = er;
      er = dr;
      dr = rotl(cr, 10);
      cr = br;
      br = tempR;
    }

    // Final addition for this block
    const t = ((h1 + cl + dr) >>> 0);
    h1 = ((h2 + dl + er) >>> 0);
    h2 = ((h3 + el + ar) >>> 0);
    h3 = ((h4 + al + br) >>> 0);
    h4 = ((h0 + bl + cr) >>> 0);
    h0 = t;
  }

  // Produce the final hash value (big-endian)
  const result = new Uint8Array(20);
  writeU32LE(result, 0, h0);
  writeU32LE(result, 4, h1);
  writeU32LE(result, 8, h2);
  writeU32LE(result, 12, h3);
  writeU32LE(result, 16, h4);

  return result;
}

function writeU32LE(buf: Uint8Array, offset: number, value: number): void {
  buf[offset] = value & 0xff;
  buf[offset + 1] = (value >>> 8) & 0xff;
  buf[offset + 2] = (value >>> 16) & 0xff;
  buf[offset + 3] = (value >>> 24) & 0xff;
}

/**
 * Verify RIPEMD-160 against known test vectors
 */
export function verifyRipemd160(): { passed: boolean; results: { input: string; expected: string; got: string; ok: boolean }[] } {
  const testVectors = [
    {
      input: "",
      expected: "9c1185a5c5e9fc54612808977ee8f548b2258d31",
    },
    {
      input: "abc",
      expected: "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc",
    },
    {
      input: "message digest",
      expected: "5d0689ef49d2fae572b881b123a85ffa21595f36",
    },
  ];

  const results = testVectors.map((tv) => {
    const input = new TextEncoder().encode(tv.input);
    const hash = ripemd160(input);
    const got = Array.from(hash)
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
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
