/**
 * Bitcoin Address Computation
 *
 * Computes Bitcoin addresses from public keys using:
 * address = Base58Check(0x00 + RIPEMD-160(SHA-256(pubkey)))
 *
 * Also provides private key verification.
 */

import { sha256Full, hashToHex } from './sha256-engine';
import { ripemd160 } from './ripemd160';
import { getPublicKey, bytesToHex, hexToBytes, privateKeyFromHex } from './secp256k1';

// Base58 alphabet (Bitcoin)
const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

/**
 * Convert a Uint8Array to a Base58 string
 */
function base58Encode(data: Uint8Array): string {
  // Count leading zero bytes
  let leadingZeros = 0;
  for (let i = 0; i < data.length; i++) {
    if (data[i] === 0) {
      leadingZeros++;
    } else {
      break;
    }
  }

  // Convert to BigInt for division
  let num = 0n;
  for (let i = 0; i < data.length; i++) {
    num = num * 256n + BigInt(data[i]);
  }

  // Encode to Base58
  let result = '';
  while (num > 0n) {
    const remainder = num % 58n;
    num = num / 58n;
    result = BASE58_ALPHABET[Number(remainder)] + result;
  }

  // Add leading '1' characters for each leading zero byte
  for (let i = 0; i < leadingZeros; i++) {
    result = '1' + result;
  }

  return result;
}

/**
 * Base58Check encoding
 * Format: Base58(version_byte + payload + first_4_bytes_of_double_sha256_checksum)
 *
 * @param version - Version byte (0x00 for mainnet P2PKH)
 * @param payload - The payload (typically RIPEMD-160 hash, 20 bytes)
 * @returns Base58Check-encoded string
 */
export function base58checkEncode(version: number, payload: Uint8Array): string {
  // version + payload
  const versionedPayload = new Uint8Array(1 + payload.length);
  versionedPayload[0] = version;
  versionedPayload.set(payload, 1);

  // Compute checksum: first 4 bytes of SHA-256(SHA-256(versionedPayload))
  const firstHash = sha256Full(versionedPayload);
  const firstHashBytes = hexToBytes(hashToHex(firstHash));
  const secondHash = sha256Full(firstHashBytes);
  const secondHashHex = hashToHex(secondHash);

  // Take first 4 bytes of second hash as checksum
  const checksum = hexToBytes(secondHashHex.slice(0, 8));

  // Combine version + payload + checksum
  const fullData = new Uint8Array(versionedPayload.length + 4);
  fullData.set(versionedPayload);
  fullData.set(checksum, versionedPayload.length);

  return base58Encode(fullData);
}

/**
 * Compute the Hash160 (RIPEMD-160 of SHA-256) of a compressed public key
 * @param compressedPubkeyHex - Compressed public key as hex string (66 chars)
 * @returns 20-byte Hash160 as Uint8Array
 */
export function hash160(compressedPubkeyHex: string): Uint8Array {
  const pubkeyBytes = hexToBytes(compressedPubkeyHex);

  // SHA-256(pubkey)
  const sha256Hash = sha256Full(pubkeyBytes);
  const sha256Bytes = hexToBytes(hashToHex(sha256Hash));

  // RIPEMD-160(SHA-256(pubkey))
  return ripemd160(sha256Bytes);
}

/**
 * Compute Bitcoin address from compressed public key
 * address = Base58Check(0x00 + RIPEMD-160(SHA-256(pubkey)))
 *
 * @param compressedPubkeyHex - Compressed public key as hex string (66 chars)
 * @returns Bitcoin P2PKH address string
 */
export function pubkeyToAddress(compressedPubkeyHex: string): string {
  const h160 = hash160(compressedPubkeyHex);
  return base58checkEncode(0x00, h160);
}

/**
 * Verify if a private key produces the expected address
 *
 * @param privkeyHex - Private key as hex string (64 chars)
 * @param expectedAddress - Expected Bitcoin address
 * @returns true if the private key produces the expected address
 */
export function verifyPrivateKey(privkeyHex: string, expectedAddress: string): boolean {
  try {
    const privKeyBytes = privateKeyFromHex(privkeyHex);
    const { compressed } = getPublicKey(privKeyBytes);
    const compressedHex = bytesToHex(compressed);
    const address = pubkeyToAddress(compressedHex);
    return address === expectedAddress;
  } catch {
    return false;
  }
}

/**
 * Verify if a private key produces the expected compressed public key
 *
 * @param privkeyHex - Private key as hex string (64 chars)
 * @param expectedPubkeyHex - Expected compressed public key hex
 * @returns true if the private key produces the expected public key
 */
export function verifyPrivateKeyAgainstPubkey(privkeyHex: string, expectedPubkeyHex: string): boolean {
  try {
    const privKeyBytes = privateKeyFromHex(privkeyHex);
    const { compressed } = getPublicKey(privKeyBytes);
    const compressedHex = bytesToHex(compressed);
    return compressedHex.toLowerCase() === expectedPubkeyHex.toLowerCase();
  } catch {
    return false;
  }
}

/**
 * Compute the full pipeline for a private key:
 * privkey → pubkey → SHA-256 → RIPEMD-160 → address
 */
export function computeFullPipeline(privkeyHex: string): {
  privateKeyHex: string;
  publicKeyCompressedHex: string;
  sha256Hex: string;
  hash160Hex: string;
  address: string;
} {
  const privKeyBytes = privateKeyFromHex(privkeyHex);
  const { compressed } = getPublicKey(privKeyBytes);
  const compressedHex = bytesToHex(compressed);

  const sha256Hash = sha256Full(compressed);
  const sha256Hex = hashToHex(sha256Hash);

  const h160 = hash160(compressedHex);
  const hash160Hex = Array.from(h160).map(b => b.toString(16).padStart(2, '0')).join('');

  const address = base58checkEncode(0x00, h160);

  return {
    privateKeyHex: privkeyHex,
    publicKeyCompressedHex: compressedHex,
    sha256Hex,
    hash160Hex,
    address,
  };
}
