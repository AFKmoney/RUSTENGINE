/**
 * Bitcoin Key Derivation Pipeline
 *
 * Forward direction only: privkey → pubkey → SHA-256(pubkey) → SHA-256(hash) → RIPEMD-160 → address
 *
 * We focus on the SHA-256 step for our fractal analysis:
 * - Does SHA-256 behave differently on secp256k1 public keys (structured) vs random data?
 * - This is about analyzing SHA-256, NOT reversing ECDSA
 */

import { getPublicKey, generateRandomPrivateKey, bytesToHex, hexToBytes, privateKeyFromHex, decompressPublicKey, validatePublicKey } from './secp256k1';
import { sha256Full, hashToHex } from './sha256-engine';

// --- Types ---

export interface PipelineResult {
  privateKeyHex: string;          // 64 hex chars (32 bytes)
  publicKeyUncompressedHex: string;  // 130 hex chars (65 bytes)
  publicKeyCompressedHex: string;    // 66 hex chars (33 bytes)
  sha256OfPubkey: string;            // SHA-256(compressed pubkey) — 64 hex chars (32 bytes)
  doubleSha256: string;              // SHA-256(SHA-256(compressed pubkey)) — 64 hex chars
  sha256OfUncompressed: string;      // SHA-256(uncompressed pubkey) — for comparison
  // Note: RIPEMD-160 is not yet implemented — placeholder
  ripemd160Available: boolean;
}

export interface HammingDistanceResult {
  distance: number;
  maxPossible: number;
  percentage: number;
}

export interface KeySpaceEntry {
  privateKeyInt: number;
  publicKeyCompressedHex: string;
  sha256Hex: string;
}

export interface KeySpaceDistance {
  key1: number;
  key2: number;
  hammingDistance: number;
  isCloserThanRandom: boolean;
  expectedRandomDistance: number; // ~128 for 256-bit outputs
}

// --- Pipeline Functions ---

/**
 * Compute the full Bitcoin pipeline from a private key (hex string)
 */
export function computePipeline(privateKeyHex: string): PipelineResult {
  const privKeyBytes = privateKeyFromHex(privateKeyHex);
  return computePipelineFromBytes(privKeyBytes);
}

/**
 * Compute the full Bitcoin pipeline from a private key (bytes)
 */
export function computePipelineFromBytes(privateKey: Uint8Array): PipelineResult {
  const { compressed, uncompressed } = getPublicKey(privateKey);

  const privateKeyHex = bytesToHex(privateKey);
  const publicKeyCompressedHex = bytesToHex(compressed);
  const publicKeyUncompressedHex = bytesToHex(uncompressed);

  // SHA-256(compressed pubkey)
  const sha256Compressed = sha256Full(compressed);
  const sha256OfPubkey = hashToHex(sha256Compressed);

  // SHA-256(SHA-256(compressed pubkey)) — double SHA-256
  const firstHashBytes = hexToBytes(sha256OfPubkey);
  const sha256Double = sha256Full(firstHashBytes);
  const doubleSha256 = hashToHex(sha256Double);

  // SHA-256(uncompressed pubkey) — for comparison
  const sha256Uncompressed = sha256Full(uncompressed);
  const sha256OfUncompressed = hashToHex(sha256Uncompressed);

  return {
    privateKeyHex,
    publicKeyUncompressedHex,
    publicKeyCompressedHex,
    sha256OfPubkey,
    doubleSha256,
    sha256OfUncompressed,
    ripemd160Available: false,
  };
}

/**
 * Parse a public key from hex (compressed or uncompressed)
 * Returns the raw bytes as Uint8Array
 */
export function parsePublicKey(pubkeyHex: string): Uint8Array {
  const clean = pubkeyHex.replace(/\s/g, "").toLowerCase();

  if (clean.length === 130 && clean.startsWith("04")) {
    return hexToBytes(clean);
  } else if (clean.length === 66 && (clean.startsWith("02") || clean.startsWith("03"))) {
    return hexToBytes(clean);
  }

  throw new Error(
    `Invalid public key: expected 66 or 130 hex chars, got ${clean.length}`
  );
}

/**
 * Get the SHA-256 input block from a compressed public key.
 *
 * A compressed pubkey is 33 bytes. SHA-256 pads this to a 512-bit (64-byte) block:
 * - 33 bytes of pubkey data
 * - 0x80 padding byte
 * - 22 zero bytes
 * - 8 bytes of length (264 bits = 0x0000000000000108)
 *
 * For our fractal analysis, we want the ACTUAL padded 64-byte block
 * that SHA-256 processes internally.
 */
export function pubkeyToSha256Block(pubkeyHex: string): Uint8Array {
  const pubkeyBytes = parsePublicKey(pubkeyHex);
  const msgLen = pubkeyBytes.length;
  const bitLen = msgLen * 8;

  // SHA-256 padding: message + 0x80 + zeros + 64-bit length
  let paddedLen = msgLen + 1;
  while (paddedLen % 64 !== 56) {
    paddedLen++;
  }
  paddedLen += 8;

  if (paddedLen !== 64) {
    // For a 33-byte or 65-byte pubkey, padded length should be 64
    throw new Error(`Unexpected padded length: ${paddedLen}`);
  }

  const block = new Uint8Array(64);
  block.set(pubkeyBytes);
  block[msgLen] = 0x80;

  // Append length in bits as big-endian 64-bit
  const lenHi = Math.floor(bitLen / 0x100000000);
  const lenLo = bitLen >>> 0;
  block[56] = (lenHi >>> 24) & 0xff;
  block[57] = (lenHi >>> 16) & 0xff;
  block[58] = (lenHi >>> 8) & 0xff;
  block[59] = lenHi & 0xff;
  block[60] = (lenLo >>> 24) & 0xff;
  block[61] = (lenLo >>> 16) & 0xff;
  block[62] = (lenLo >>> 8) & 0xff;
  block[63] = lenLo & 0xff;

  return block;
}

/**
 * Generate a random 33-byte input (same size as compressed pubkey) for comparison
 */
export function generateRandomInput33(): Uint8Array {
  const data = new Uint8Array(33);
  crypto.getRandomValues(data);
  return data;
}

/**
 * Pad a 33-byte input to a 64-byte SHA-256 block (same as pubkeyToSha256Block but for arbitrary data)
 */
export function input33ToSha256Block(data: Uint8Array): Uint8Array {
  if (data.length !== 33) {
    throw new Error(`Expected 33 bytes, got ${data.length}`);
  }

  const msgLen = 33;
  const bitLen = msgLen * 8;
  const block = new Uint8Array(64);
  block.set(data);
  block[33] = 0x80;

  // Length: 264 bits = 0x108
  block[63] = 0x08; // 264 & 0xFF
  block[62] = 0x01; // (264 >> 8) & 0xFF

  return block;
}

/**
 * Compute SHA-256 of a 33-byte input (padded to 64-byte block internally)
 */
export function sha256Of33Bytes(data: Uint8Array): string {
  const hash = sha256Full(data);
  return hashToHex(hash);
}

// --- Hamming Distance ---

/**
 * Compute the Hamming distance between two hex strings of equal length
 */
export function hammingDistanceHex(hex1: string, hex2: string): HammingDistanceResult {
  if (hex1.length !== hex2.length) {
    throw new Error(`Hex strings must be equal length: ${hex1.length} vs ${hex2.length}`);
  }

  let distance = 0;
  const maxPossible = hex1.length * 4; // each hex char = 4 bits

  for (let i = 0; i < hex1.length; i++) {
    const v1 = parseInt(hex1[i], 16);
    const v2 = parseInt(hex2[i], 16);
    const xor = v1 ^ v2;
    distance += popcount4(xor);
  }

  return {
    distance,
    maxPossible,
    percentage: (distance / maxPossible) * 100,
  };
}

/**
 * Popcount for a 4-bit value (0-15)
 */
function popcount4(x: number): number {
  const table = [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4];
  return table[x & 0xf];
}

// --- Key Space Explorer ---

/**
 * Explore a range of private keys and compute their pubkeys and SHA-256 hashes.
 * Returns entries for keys from startKey to startKey + count - 1
 */
export function exploreKeySpace(startKey: number, count: number): KeySpaceEntry[] {
  const entries: KeySpaceEntry[] = [];

  for (let k = startKey; k < startKey + count; k++) {
    const privKey = new Uint8Array(32);
    // Write k as big-endian 32 bytes
    let val = BigInt(k);
    for (let i = 31; i >= 0; i--) {
      privKey[i] = Number(val & 0xFFn);
      val >>= 8n;
    }

    const { compressed } = getPublicKey(privKey);
    const compressedHex = bytesToHex(compressed);
    const sha256Hex = sha256Of33Bytes(compressed);

    entries.push({
      privateKeyInt: k,
      publicKeyCompressedHex: compressedHex,
      sha256Hex,
    });
  }

  return entries;
}

/**
 * Compute distances between consecutive keys in the key space
 */
export function computeKeySpaceDistances(entries: KeySpaceEntry[]): KeySpaceDistance[] {
  const distances: KeySpaceDistance[] = [];
  const expectedRandomDistance = 128; // Expected Hamming distance for random 256-bit strings

  for (let i = 0; i < entries.length - 1; i++) {
    const e1 = entries[i];
    const e2 = entries[i + 1];

    // Compare SHA-256 outputs
    const result = hammingDistanceHex(e1.sha256Hex, e2.sha256Hex);

    distances.push({
      key1: e1.privateKeyInt,
      key2: e2.privateKeyInt,
      hammingDistance: result.distance,
      isCloserThanRandom: result.distance < expectedRandomDistance - 10, // 10-bit tolerance
      expectedRandomDistance,
    });
  }

  return distances;
}

/**
 * Generate a batch of random 33-byte inputs and their SHA-256 hashes
 * for the comparison analysis
 */
export function generateRandomComparisonBatch(count: number): {
  inputs: string[]; // hex of 33-byte inputs
  sha256Hashes: string[]; // hex of SHA-256 outputs
} {
  const inputs: string[] = [];
  const sha256Hashes: string[] = [];

  for (let i = 0; i < count; i++) {
    const data = generateRandomInput33();
    inputs.push(bytesToHex(data));
    sha256Hashes.push(sha256Of33Bytes(data));
  }

  return { inputs, sha256Hashes };
}

/**
 * Compute the average Hamming distance between consecutive entries in a list of hex hashes
 */
export function averageConsecutiveHamming(hashes: string[]): number {
  if (hashes.length < 2) return 0;

  let total = 0;
  for (let i = 0; i < hashes.length - 1; i++) {
    const result = hammingDistanceHex(hashes[i], hashes[i + 1]);
    total += result.distance;
  }

  return total / (hashes.length - 1);
}

// Re-export for convenience
export { validatePublicKey, generateRandomPrivateKey, bytesToHex, hexToBytes, decompressPublicKey };
