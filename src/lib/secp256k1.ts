/**
 * secp256k1 Elliptic Curve Operations (Pure TypeScript)
 *
 * Implements forward-only elliptic curve math for computing public keys
 * from private keys. This is NOT for reversing ECDSA — it's for generating
 * the structured inputs (public keys) that we feed into SHA-256 fractal analysis.
 *
 * All arithmetic uses BigInt for 256-bit precision.
 */

// secp256k1 curve parameters
const P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2Fn; // field prime
const N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141n; // group order
const A = 0n; // curve y² = x³ + 7
const B = 7n;
const GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798n; // generator X
const GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8n; // generator Y

// Point type: affine coordinates, or null for point at infinity
export interface Point {
  x: bigint;
  y: bigint;
}

// Point at infinity represented as null
type ECPoint = Point | null;

// --- Modular Arithmetic ---

/**
 * Modular reduction: always returns a non-negative result in [0, p)
 */
function mod(a: bigint, p: bigint = P): bigint {
  const result = a % p;
  return result < 0n ? result + p : result;
}

/**
 * Modular multiplicative inverse using Fermat's little theorem.
 * Since P is prime, a^(-1) ≡ a^(p-2) mod p
 */
function modInverse(a: bigint, p: bigint = P): bigint {
  return modPow(a, p - 2n, p);
}

/**
 * Modular exponentiation using square-and-multiply
 */
function modPow(base: bigint, exp: bigint, p: bigint): bigint {
  if (exp === 0n) return 1n;
  if (exp === 1n) return mod(base, p);

  let result = 1n;
  let b = mod(base, p);
  let e = exp;

  while (e > 0n) {
    if (e & 1n) {
      result = mod(result * b, p);
    }
    e >>= 1n;
    b = mod(b * b, p);
  }

  return result;
}

/**
 * Modular square root using Tonelli-Shanks algorithm.
 * For secp256k1, P ≡ 3 (mod 4), so sqrt(n) = n^((P+1)/4) mod P
 */
function modSqrt(n: bigint, p: bigint = P): bigint {
  // Verify P ≡ 3 (mod 4) — secp256k1 satisfies this
  if (mod(p, 4n) !== 3n) {
    throw new Error("modSqrt: P must be ≡ 3 (mod 4) for this optimization");
  }
  const root = modPow(n, (p + 1n) / 4n, p);
  // Verify
  if (mod(root * root, p) !== mod(n, p)) {
    throw new Error("modSqrt: no square root exists");
  }
  return root;
}

// --- Elliptic Curve Point Operations ---

/**
 * Check if a point is on the secp256k1 curve
 */
export function isOnCurve(point: ECPoint): boolean {
  if (point === null) return true; // Point at infinity is on the curve
  const { x, y } = point;
  // y² ≡ x³ + 7 (mod P)
  const lhs = mod(y * y, P);
  const rhs = mod(x * x * x + B, P);
  return lhs === rhs;
}

/**
 * Point addition: P1 + P2 on secp256k1
 */
function pointAdd(p1: ECPoint, p2: ECPoint): ECPoint {
  // Identity cases
  if (p1 === null) return p2;
  if (p2 === null) return p1;

  // Same point → use pointDouble
  if (p1.x === p2.x && p1.y === p2.y) {
    return pointDouble(p1);
  }

  // P1 + (-P1) = O (point at infinity)
  if (p1.x === p2.x && p1.y === mod(-p2.y, P)) {
    return null;
  }

  // General case: slope = (y2 - y1) / (x2 - x1)
  const dy = mod(p2.y - p1.y, P);
  const dx = mod(p2.x - p1.x, P);
  const slope = mod(dy * modInverse(dx, P), P);

  // x3 = slope² - x1 - x2
  const x3 = mod(slope * slope - p1.x - p2.x, P);
  // y3 = slope * (x1 - x3) - y1
  const y3 = mod(slope * (p1.x - x3) - p1.y, P);

  return { x: x3, y: y3 };
}

/**
 * Point doubling: 2*P on secp256k1
 */
function pointDouble(p: ECPoint): ECPoint {
  if (p === null) return null;
  if (p.y === 0n) return null; // Tangent is vertical

  // slope = (3x² + a) / (2y) — for secp256k1, a = 0
  const numerator = mod(3n * p.x * p.x + A, P);
  const denominator = mod(2n * p.y, P);
  const slope = mod(numerator * modInverse(denominator, P), P);

  // x3 = slope² - 2x
  const x3 = mod(slope * slope - 2n * p.x, P);
  // y3 = slope * (x - x3) - y
  const y3 = mod(slope * (p.x - x3) - p.y, P);

  return { x: x3, y: y3 };
}

/**
 * Scalar multiplication using double-and-add algorithm.
 * k * P = sum of 2^i * P for each bit i where k_i = 1
 */
function scalarMultiply(k: bigint, point: ECPoint): ECPoint {
  if (point === null) return null;
  if (k === 0n) return null;
  if (k < 0n) {
    // Negate: -P = (x, -y mod P)
    return scalarMultiply(-k, negatePoint(point));
  }

  // Reduce k modulo N (group order)
  k = mod(k, N);
  if (k === 0n) return null;

  let result: ECPoint = null; // Point at infinity (additive identity)
  let addend: ECPoint = point;

  while (k > 0n) {
    if (k & 1n) {
      result = pointAdd(result, addend);
    }
    addend = pointDouble(addend);
    k >>= 1n;
  }

  return result;
}

/**
 * Negate a point: -P = (x, P - y)
 */
function negatePoint(p: ECPoint): ECPoint {
  if (p === null) return null;
  return { x: p.x, y: mod(-p.y, P) };
}

// --- Public Key Generation ---

/**
 * Get the generator point G
 */
export function getGenerator(): Point {
  return { x: GX, y: GY };
}

/**
 * Compute the public key from a private key.
 * Public key = private_key * G (generator point)
 *
 * @param privateKey - 32-byte private key as Uint8Array (big-endian)
 * @returns Object with compressed and uncompressed public key bytes
 */
export function getPublicKey(privateKey: Uint8Array): {
  compressed: Uint8Array;
  uncompressed: Uint8Array;
} {
  if (privateKey.length !== 32) {
    throw new Error(`Private key must be 32 bytes, got ${privateKey.length}`);
  }

  // Convert private key bytes to BigInt (big-endian)
  let k = 0n;
  for (let i = 0; i < 32; i++) {
    k = (k << 8n) | BigInt(privateKey[i]);
  }

  // Validate private key range: 1 <= k < N
  if (k === 0n || k >= N) {
    throw new Error("Private key must be in range [1, N-1]");
  }

  // Compute public key point: P = k * G
  const point = scalarMultiply(k, { x: GX, y: GY });

  if (point === null) {
    throw new Error("Scalar multiplication resulted in point at infinity");
  }

  // Verify the point is on the curve (sanity check)
  if (!isOnCurve(point)) {
    throw new Error("Computed point is not on the curve — implementation error");
  }

  // Encode as uncompressed: 0x04 + x(32 bytes) + y(32 bytes) = 65 bytes
  const uncompressed = new Uint8Array(65);
  uncompressed[0] = 0x04;
  bigintToBytes32(point.x, uncompressed, 1);
  bigintToBytes32(point.y, uncompressed, 33);

  // Encode as compressed: 0x02/0x03 + x(32 bytes) = 33 bytes
  const compressed = new Uint8Array(33);
  compressed[0] = (point.y & 1n) === 0n ? 0x02 : 0x03; // 0x02 if y even, 0x03 if y odd
  bigintToBytes32(point.x, compressed, 1);

  return { compressed, uncompressed };
}

/**
 * Parse a compressed or uncompressed public key from hex string.
 * Returns the raw bytes.
 */
export function parsePublicKey(pubkeyHex: string): Uint8Array {
  const clean = pubkeyHex.replace(/\s/g, "").toLowerCase();
  if (clean.length === 130 && clean.startsWith("04")) {
    // Uncompressed
    return hexToBytes(clean);
  } else if (clean.length === 66 && (clean.startsWith("02") || clean.startsWith("03"))) {
    // Compressed
    return hexToBytes(clean);
  }
  throw new Error(
    `Invalid public key format: expected 66 or 130 hex chars starting with 02/03/04, got ${clean.length} chars`
  );
}

/**
 * Decompress a public key: given the x-coordinate and parity,
 * compute the y-coordinate.
 */
export function decompressPublicKey(compressed: Uint8Array): Point {
  if (compressed.length !== 33) {
    throw new Error(`Compressed public key must be 33 bytes, got ${compressed.length}`);
  }
  const prefix = compressed[0];
  if (prefix !== 0x02 && prefix !== 0x03) {
    throw new Error(`Invalid compressed key prefix: 0x${prefix.toString(16).padStart(2, "0")}`);
  }

  // Extract x coordinate
  let x = 0n;
  for (let i = 1; i < 33; i++) {
    x = (x << 8n) | BigInt(compressed[i]);
  }

  // Compute y² = x³ + 7 mod P
  const ySquared = mod(x * x * x + B, P);

  // Compute y = sqrt(y²) using Tonelli-Shanks (optimized for P ≡ 3 mod 4)
  let y = modSqrt(ySquared, P);

  // Choose the correct y based on parity prefix
  const isYOdd = (y & 1n) === 1n;
  const prefixIsOdd = prefix === 0x03;

  if (isYOdd !== prefixIsOdd) {
    y = mod(-y, P);
  }

  return { x, y };
}

/**
 * Validate a public key: check if it's a valid point on the curve
 */
export function validatePublicKey(pubkeyHex: string): {
  valid: boolean;
  error?: string;
  point?: Point;
} {
  try {
    const bytes = parsePublicKey(pubkeyHex);

    if (bytes.length === 65) {
      // Uncompressed
      let x = 0n;
      let y = 0n;
      for (let i = 0; i < 32; i++) {
        x = (x << 8n) | BigInt(bytes[1 + i]);
        y = (y << 8n) | BigInt(bytes[33 + i]);
      }
      const point = { x, y };
      if (!isOnCurve(point)) {
        return { valid: false, error: "Point is not on the secp256k1 curve" };
      }
      return { valid: true, point };
    } else {
      // Compressed
      const point = decompressPublicKey(bytes);
      if (!isOnCurve(point)) {
        return { valid: false, error: "Decompressed point is not on the curve" };
      }
      return { valid: true, point };
    }
  } catch (e) {
    return { valid: false, error: (e as Error).message };
  }
}

/**
 * Generate a random private key using crypto.getRandomValues
 */
export function generateRandomPrivateKey(): Uint8Array {
  const key = new Uint8Array(32);
  crypto.getRandomValues(key);

  // Ensure key is in valid range [1, N-1]
  // The probability of generating 0 or >= N is astronomically small,
  // but we handle it for correctness
  let k = 0n;
  for (let i = 0; i < 32; i++) {
    k = (k << 8n) | BigInt(key[i]);
  }

  if (k === 0n || k >= N) {
    // Retry — this is virtually impossible
    return generateRandomPrivateKey();
  }

  return key;
}

// --- Helper Functions ---

/**
 * Convert a BigInt to 32 bytes (big-endian) and write into target array at offset
 */
function bigintToBytes32(value: bigint, target: Uint8Array, offset: number): void {
  for (let i = 0; i < 32; i++) {
    target[offset + 31 - i] = Number((value >> BigInt(i * 8)) & 0xFFn);
  }
}

/**
 * Convert a hex string to Uint8Array
 */
export function hexToBytes(hex: string): Uint8Array {
  const clean = hex.replace(/\s/g, "");
  if (clean.length % 2 !== 0) {
    throw new Error("Hex string must have even length");
  }
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

/**
 * Convert a Uint8Array to lowercase hex string
 */
export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")
    .toLowerCase();
}

/**
 * Convert a private key from hex string to Uint8Array
 */
export function privateKeyFromHex(hex: string): Uint8Array {
  const clean = hex.replace(/\s/g, "").toLowerCase();
  if (clean.length !== 64) {
    throw new Error(`Private key hex must be 64 characters, got ${clean.length}`);
  }
  return hexToBytes(clean);
}

/**
 * Verify secp256k1 implementation against known test vectors.
 * Returns test results for display.
 */
export function verifySecp256k1(): {
  passed: boolean;
  results: { name: string; expected: string; got: string; ok: boolean }[];
} {
  const results: { name: string; expected: string; got: string; ok: boolean }[] = [];

  // Test 1: Generator point is on the curve
  const genOnCurve = isOnCurve({ x: GX, y: GY });
  results.push({
    name: "Generator on curve",
    expected: "true",
    got: String(genOnCurve),
    ok: genOnCurve,
  });

  // Test 2: privkey = 1 → known compressed pubkey
  const key1 = new Uint8Array(32);
  key1[31] = 1; // private key = 1
  const pub1 = getPublicKey(key1);
  const pub1Hex = bytesToHex(pub1.compressed);
  const expected1 = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
  results.push({
    name: "privkey=1 compressed pubkey",
    expected: expected1,
    got: pub1Hex,
    ok: pub1Hex === expected1,
  });

  // Test 3: privkey = 2 → known compressed pubkey
  const key2 = new Uint8Array(32);
  key2[31] = 2;
  const pub2 = getPublicKey(key2);
  const pub2Hex = bytesToHex(pub2.compressed);
  const expected2 = "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
  results.push({
    name: "privkey=2 compressed pubkey",
    expected: expected2,
    got: pub2Hex,
    ok: pub2Hex === expected2,
  });

  // Test 4: privkey = 3 → known compressed pubkey
  const key3 = new Uint8Array(32);
  key3[31] = 3;
  const pub3 = getPublicKey(key3);
  const pub3Hex = bytesToHex(pub3.compressed);
  const expected3 = "02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
  results.push({
    name: "privkey=3 compressed pubkey",
    expected: expected3,
    got: pub3Hex,
    ok: pub3Hex === expected3,
  });

  // Test 5: Point addition G + G = 2G
  const doubleG = pointDouble({ x: GX, y: GY });
  const addGG = pointAdd({ x: GX, y: GY }, { x: GX, y: GY });
  const doubleMatch = doubleG !== null && addGG !== null &&
    doubleG.x === addGG.x && doubleG.y === addGG.y;
  results.push({
    name: "G + G = 2G",
    expected: "double == add",
    got: doubleMatch ? "double == add" : "MISMATCH",
    ok: doubleMatch,
  });

  // Test 6: 2G matches privkey=2
  const twoGMatch = doubleG !== null &&
    doubleG.x.toString(16) === pub2Hex.slice(2);
  results.push({
    name: "2G matches privkey=2",
    expected: "x coords match",
    got: twoGMatch ? "match" : "MISMATCH",
    ok: twoGMatch,
  });

  // Test 7: Verify N * G = point at infinity
  const nG = scalarMultiply(N, { x: GX, y: GY });
  results.push({
    name: "N*G = point at infinity",
    expected: "null",
    got: nG === null ? "null" : "NOT null",
    ok: nG === null,
  });

  // Test 8: Uncompressed pubkey format
  const pub1Uncompressed = bytesToHex(pub1.uncompressed);
  const expectedUncompressed1 = "0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8";
  results.push({
    name: "privkey=1 uncompressed pubkey",
    expected: expectedUncompressed1,
    got: pub1Uncompressed,
    ok: pub1Uncompressed === expectedUncompressed1,
  });

  return {
    passed: results.every((r) => r.ok),
    results,
  };
}

// Export for testing / advanced usage
export { P, N, A, B, GX, GY, mod, modInverse, modPow, pointAdd, pointDouble, scalarMultiply };
