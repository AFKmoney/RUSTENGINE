/**
 * Pollard's Kangaroo (Lambda) Algorithm for ECDLP
 *
 * Given:
 *   - Target public key P = k * G where k is in [a, b]
 *   - We want to find k
 *
 * Algorithm:
 *   1. Tame kangaroo: starts at T0 = b * G (known position, known distance b)
 *      Jumps with step sizes that are powers of 2, records (T_pos, T_dist)
 *      After N jumps, T has traveled a known distance from b
 *
 *   2. Wild kangaroo: starts at P (unknown position k, distance 0 from k)
 *      Jumps with SAME step sizes, records (W_pos, W_dist)
 *
 *   3. If W lands on same point as T, then:
 *      k = b + T_dist - W_dist
 *
 * Expected number of jumps: ~3.5 * √(b - a) for each kangaroo
 * This is MUCH better than brute force which needs (b - a) attempts
 *
 * Implementation uses distinguished points for memory efficiency:
 *   - Only store points where x mod D = 0 for some D
 *   - When a wild distinguished point matches a tame one → SOLVED
 */

import {
  Point,
  scalarMultiply,
  pointAdd,
  getGenerator,
  mod,
  N,
  P,
  hexToBytes,
  bytesToHex,
  decompressPublicKey,
  getPublicKey,
} from './secp256k1';

// --- Types ---

export interface KangarooProgress {
  iteration: number;
  tameDistance: bigint;
  wildDistance: bigint;
  found: boolean;
  privateKey?: bigint;
  tameDPs: number;
  wildDPs: number;
  tamePos?: Point;
  wildPos?: Point;
}

export interface KangarooResult {
  found: boolean;
  privateKey?: bigint;
  iterations: number;
  timeMs: number;
  tameDPs: number;
  wildDPs: number;
}

export interface BruteForceResult {
  found: boolean;
  privateKey?: bigint;
  checked: number;
}

// --- Helper: Compute log2 of a BigInt ---

function bigIntLog2(n: bigint): number {
  if (n <= 0n) return 0;
  return n.toString(2).length - 1;
}

// --- Step Function ---

/**
 * Compute the step size index from a point's x-coordinate.
 * Maps the x-coordinate to an index in [0, numStepSizes)
 *
 * The number of step sizes should be ~2 * log2(range_size)
 * so that the average step size is approximately √(range_size) / 4
 */
function stepIndex(x: bigint, numStepSizes: number): number {
  return Number(x % BigInt(numStepSizes));
}

/**
 * Pre-compute jump points: jumpTable[i] = 2^i * G
 * This avoids repeated scalar multiplications during the main loop.
 */
function precomputeJumpTable(numStepSizes: number): Point[] {
  const G = getGenerator();
  const table: Point[] = [];
  for (let i = 0; i < numStepSizes; i++) {
    const step = 1n << BigInt(i); // 2^i
    const point = scalarMultiply(step, G);
    if (point === null) {
      throw new Error(`Failed to compute jump point 2^${i} * G`);
    }
    table.push(point);
  }
  return table;
}

// --- Distinguished Point Check ---

/**
 * Check if a point is a distinguished point.
 * A point is distinguished if its x-coordinate mod D === 0.
 *
 * D is chosen based on the range size:
 * D = 2^d where d ≈ log2(√range_size) - some_constant
 *
 * For memory efficiency, we want roughly √(range_size) / 2^d distinguished points
 * per kangaroo run. Typical: d such that there are ~2^20 distinguished points.
 */
function isDistinguished(x: bigint, D: bigint): boolean {
  return x % D === 0n;
}

/**
 * Compute the distinguished point parameter D.
 * We want approximately O(√n / K) distinguished points, where K is a constant.
 *
 * For practical purposes:
 * - D = 2^d where d = max(1, floor(log2(√range_size)) - 10)
 * - This means roughly 1 in 2^d points is distinguished
 * - We expect to find √range_size / 2^d distinguished points per kangaroo
 */
function computeDParameter(rangeSize: bigint): bigint {
  const logRange = bigIntLog2(rangeSize);
  const sqrtBits = Math.floor(logRange / 2);

  // We want roughly 1 in 2^10 to 2^20 points to be distinguished
  // For small ranges, use smaller D
  let d = Math.max(1, sqrtBits - 10);
  d = Math.min(d, 20); // cap at 2^20 to avoid too sparse storage
  d = Math.max(d, 1);  // at least 2^1

  return 1n << BigInt(d);
}

// --- Main Kangaroo Algorithm ---

/**
 * Run Pollard's Kangaroo algorithm
 *
 * @param targetPubkey - The public key P = k*G we want to find k for
 * @param rangeStart - Lower bound of k (inclusive), i.e., a
 * @param rangeEnd - Upper bound of k (inclusive), i.e., b
 * @param maxIterations - Safety limit
 * @param onProgress - Callback for progress updates (called every N iterations)
 * @param checkInterval - How often to call the progress callback (default 1000)
 */
export function pollardKangaroo(
  targetPubkey: Point,
  rangeStart: bigint,
  rangeEnd: bigint,
  maxIterations: number,
  onProgress?: (progress: KangarooProgress) => void,
  checkInterval: number = 1000
): KangarooResult {
  const startTime = Date.now();
  const G = getGenerator();

  // Range parameters
  const rangeSize = rangeEnd - rangeStart + 1n;
  const logRange = bigIntLog2(rangeSize);

  // Number of step sizes: 2 * log2(range_size) gives good average step size
  const numStepSizes = Math.max(2, Math.min(2 * logRange, 64));

  // Pre-compute jump table
  const jumpTable = precomputeJumpTable(numStepSizes);

  // Distinguished point parameter
  const D = computeDParameter(rangeSize);

  // Expected number of iterations: ~3.5 * √(range_size)
  const expectedIterations = Number(3n * sqrtBigInt(rangeSize) / 2n);

  // --- Tame Kangaroo ---
  // Start at T0 = b * G
  let tamePos = scalarMultiply(rangeEnd, G);
  if (tamePos === null) {
    throw new Error("Failed to compute starting position for tame kangaroo");
  }
  let tameDistance = 0n; // distance traveled from starting position

  // --- Wild Kangaroo ---
  // Start at P = k * G (the target)
  let wildPos: Point = { ...targetPubkey };
  let wildDistance = 0n; // distance traveled from starting position

  // Storage for distinguished points
  const tameDPs = new Map<string, bigint>(); // x_hex → distance
  const wildDPs = new Map<string, bigint>(); // x_hex → distance

  let tameDPCount = 0;
  let wildDPCount = 0;

  // Run the tame kangaroo for a "warm-up" phase first
  // The tame kangaroo should make ~√(range_size) jumps to establish a good trail
  const warmupJumps = Math.min(Number(sqrtBigInt(rangeSize)), expectedIterations);

  for (let i = 0; i < warmupJumps; i++) {
    const si = stepIndex(tamePos.x, numStepSizes);
    const stepSize = 1n << BigInt(si); // 2^si
    const jumpPoint = jumpTable[si];

    tamePos = pointAdd(tamePos, jumpPoint)!;
    if (tamePos === null) {
      throw new Error("Tame kangaroo hit point at infinity during warmup");
    }
    tameDistance += stepSize;

    // Check for distinguished point
    if (isDistinguished(tamePos.x, D)) {
      const xHex = tamePos.x.toString(16).padStart(64, '0');
      if (!tameDPs.has(xHex)) {
        tameDPs.set(xHex, tameDistance);
        tameDPCount++;
      }
    }
  }

  // Now run both kangaroos alternately
  const maxIter = Math.min(maxIterations, expectedIterations * 4);
  let found = false;
  let privateKey: bigint | undefined;
  let iteration = 0;

  for (iteration = 0; iteration < maxIter && !found; iteration++) {
    // --- Wild Kangaroo Jump ---
    {
      const si = stepIndex(wildPos.x, numStepSizes);
      const stepSize = 1n << BigInt(si);
      const jumpPoint = jumpTable[si];

      wildPos = pointAdd(wildPos, jumpPoint)!;
      if (wildPos === null) {
        // Extremely unlikely - wild kangaroo hit point at infinity
        break;
      }
      wildDistance += stepSize;

      // Check for distinguished point
      if (isDistinguished(wildPos.x, D)) {
        const xHex = wildPos.x.toString(16).padStart(64, '0');
        if (!wildDPs.has(xHex)) {
          wildDPs.set(xHex, wildDistance);
          wildDPCount++;
        }

        // Check if this matches a tame distinguished point
        if (tameDPs.has(xHex)) {
          const tameDist = tameDPs.get(xHex)!;
          // k = rangeEnd + tameDist - wildDist
          const candidateKey = rangeEnd + tameDist - wildDistance;

          // Verify the candidate
          if (candidateKey >= rangeStart && candidateKey <= rangeEnd) {
            const candidatePoint = scalarMultiply(candidateKey, G);
            if (candidatePoint !== null && candidatePoint.x === targetPubkey.x) {
              found = true;
              privateKey = candidateKey;
            }
          }
        }
      }

      // Also check wild DPs against each other (collision between two wild DPs)
      // This can happen if we restart the wild kangaroo
    }

    // --- Tame Kangaroo Jump (every other iteration for balance) ---
    if (iteration % 2 === 0) {
      const si = stepIndex(tamePos.x, numStepSizes);
      const stepSize = 1n << BigInt(si);
      const jumpPoint = jumpTable[si];

      tamePos = pointAdd(tamePos, jumpPoint)!;
      if (tamePos === null) {
        break;
      }
      tameDistance += stepSize;

      if (isDistinguished(tamePos.x, D)) {
        const xHex = tamePos.x.toString(16).padStart(64, '0');
        if (!tameDPs.has(xHex)) {
          tameDPs.set(xHex, tameDistance);
          tameDPCount++;
        }

        // Check if this matches a wild distinguished point
        if (wildDPs.has(xHex)) {
          const wildDist = wildDPs.get(xHex)!;
          const candidateKey = rangeEnd + tameDistance - wildDist;

          if (candidateKey >= rangeStart && candidateKey <= rangeEnd) {
            const candidatePoint = scalarMultiply(candidateKey, G);
            if (candidatePoint !== null && candidatePoint.x === targetPubkey.x) {
              found = true;
              privateKey = candidateKey;
            }
          }
        }
      }
    }

    // Progress callback
    if (onProgress && iteration % checkInterval === 0) {
      onProgress({
        iteration,
        tameDistance,
        wildDistance,
        found,
        privateKey,
        tameDPs: tameDPCount,
        wildDPs: wildDPCount,
        tamePos,
        wildPos,
      });

      // Yield to the event loop
      // (The caller should use setTimeout-based chunking for async behavior)
    }
  }

  const timeMs = Date.now() - startTime;

  return {
    found,
    privateKey,
    iterations: iteration,
    timeMs,
    tameDPs: tameDPCount,
    wildDPs: wildDPCount,
  };
}

/**
 * Brute force search (for small ranges or address-only puzzles)
 * Try each key in [rangeStart, rangeEnd], compute pubkey, compare
 *
 * @param targetCompressedPubkey - Target compressed pubkey hex
 * @param rangeStart - Start of range
 * @param rangeEnd - End of range
 * @param onProgress - Progress callback
 */
export function bruteForceSearch(
  targetCompressedPubkey: string,
  rangeStart: bigint,
  rangeEnd: bigint,
  onProgress?: (current: bigint, checked: number) => void
): BruteForceResult {
  const target = targetCompressedPubkey.toLowerCase();

  for (let k = rangeStart; k <= rangeEnd; k++) {
    // Convert k to 32-byte big-endian array
    const keyBytes = bigintToBytes32(k);

    try {
      const { compressed } = getPublicKey(keyBytes);
      const compressedHex = bytesToHex(compressed);

      if (compressedHex === target) {
        return { found: true, privateKey: k, checked: Number(k - rangeStart) + 1 };
      }
    } catch {
      // Skip invalid keys
    }

    if (onProgress) {
      const checked = Number(k - rangeStart) + 1;
      if (checked % 100 === 0) {
        onProgress(k, checked);
      }
    }
  }

  return { found: false, checked: Number(rangeEnd - rangeStart) + 1 };
}

// --- Helper Functions ---

/**
 * BigInt square root (integer approximation)
 */
function sqrtBigInt(n: bigint): bigint {
  if (n < 0n) throw new Error("Cannot compute sqrt of negative number");
  if (n < 2n) return n;
  let x = n;
  let y = (x + 1n) / 2n;
  while (y < x) {
    x = y;
    y = (x + n / x) / 2n;
  }
  return x;
}

/**
 * Convert a BigInt to a 32-byte big-endian Uint8Array
 */
function bigintToBytes32(value: bigint): Uint8Array {
  const bytes = new Uint8Array(32);
  for (let i = 0; i < 32; i++) {
    bytes[31 - i] = Number((value >> BigInt(i * 8)) & 0xFFn);
  }
  return bytes;
}

/**
 * Parse a compressed public key hex string to a Point
 */
export function parseCompressedPubkey(hex: string): Point {
  const bytes = hexToBytes(hex);
  return decompressPublicKey(bytes);
}

/**
 * Compute the expected number of iterations for Kangaroo on a given range
 */
export function estimateKangarooIterations(rangeStart: bigint, rangeEnd: bigint): number {
  const rangeSize = rangeEnd - rangeStart + 1n;
  const sqrtSize = sqrtBigInt(rangeSize);
  return Number(4n * sqrtSize); // ~4 * sqrt(n) total iterations
}

/**
 * Estimate time in seconds based on JS BigInt performance
 * Conservative: ~500K point additions/sec
 */
export function estimateTimeSeconds(rangeStart: bigint, rangeEnd: bigint): number {
  const iters = estimateKangarooIterations(rangeStart, rangeEnd);
  const opsPerSec = 500_000;
  return iters / opsPerSec;
}

/**
 * Format time in human-readable form
 */
export function formatTime(seconds: number): string {
  if (seconds < 1) return `${(seconds * 1000).toFixed(0)}ms`;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  if (seconds < 3600) return `${(seconds / 60).toFixed(1)}min`;
  if (seconds < 86400) return `${(seconds / 3600).toFixed(1)}hrs`;
  if (seconds < 86400 * 365) return `${(seconds / 86400).toFixed(1)}days`;
  return `${(seconds / (86400 * 365)).toFixed(1)}years`;
}
