/**
 * Discrete Fractal Analysis Engine for SHA-256
 *
 * SHA-256 operates on discrete space {0,1}^256, not continuous R^n.
 * All analysis tools are built for discrete fractals — fractals on
 * hypercubes, Boolean function spectra, and discrete self-similarity.
 *
 * NO continuous fractal methods are used.
 */

import {
  compressBlock,
  flipBit,
  getWordBit,
  type CompressionTrace,
  type RoundState,
} from "./sha256-engine";

// --- Types ---

export interface DiscreteBoxCountingResult {
  round: number;
  scales: number[]; // [1, 2, 4, 8, 16, 32, 64, 128]
  ballCounts: number[]; // N(r) at each scale
  dimensionEstimates: number[]; // D between consecutive scales
  averageDimension: number;
  hasAnomaly: boolean; // D < 200 at any scale
  anomalyScale: number | null;
  anomalyDimension: number | null;
}

export interface WalshSpectrumResult {
  round: number;
  avgAbsCorrelation: number[]; // per output bit
  maxCorrelation: number;
  anomalies: { inputBit: number; outputBit: number; correlation: number }[];
  spectralFlatness: number; // 1.0 = random, <1.0 = structured
}

export interface SelfSimilarityResult {
  round: number;
  distanceHistogram: number[]; // counts for distances 0..256
  scaleDivergences: number[]; // KL divergence between consecutive scale distributions
  selfSimilarityScore: number; // 0 = no self-similarity, 1 = perfect
  isAnomalous: boolean;
}

export interface ClusterTreeResult {
  round: number;
  clusterCounts: number[]; // for thresholds [8, 16, 32, 64, 128]
  imbalance: number; // 0 = balanced, 1 = max imbalanced
  maxClusterFraction: number;
  isAnomalous: boolean;
}

export interface DimensionProfile {
  round: number;
  profile: [number, number][]; // [scale, dimension_at_that_scale]
  minDimension: number;
  minDimensionScale: number;
  isAnomaly: boolean;
}

export interface FullDiscreteAnalysis {
  boxCounting: DiscreteBoxCountingResult[];
  walshSpectrum: WalshSpectrumResult[];
  selfSimilarity: SelfSimilarityResult[];
  clusterTree: ClusterTreeResult[];
  dimensionProfile: DimensionProfile[];
}

// --- Internal Helpers ---

/**
 * Extract state vector as an array of 8 u32 words from a RoundState
 */
function roundToWords(rs: RoundState): number[] {
  return [rs.a, rs.b, rs.c, rs.d, rs.e, rs.f, rs.g, rs.h];
}

/**
 * Compute Hamming distance between two state vectors (each = 8 × 32-bit words = 256 bits)
 */
function hammingDistance(v1: number[], v2: number[]): number {
  let dist = 0;
  for (let i = 0; i < 8; i++) {
    // XOR the words and count bits
    const xor = (v1[i] ^ v2[i]) >>> 0;
    dist += popcount32(xor);
  }
  return dist;
}

/**
 * Population count for a 32-bit unsigned integer
 */
function popcount32(x: number): number {
  x = x >>> 0;
  x = x - ((x >>> 1) & 0x55555555);
  x = (x & 0x33333333) + ((x >>> 2) & 0x33333333);
  x = (x + (x >>> 4)) & 0x0f0f0f0f;
  x = x + (x >>> 8);
  x = x + (x >>> 16);
  return x & 0x7f;
}

/**
 * Convert a state vector (8 u32 words) to a 256-bit binary array
 */
function stateToBits(words: number[]): number[] {
  const bits: number[] = [];
  for (let w = 0; w < 8; w++) {
    for (let b = 0; b < 32; b++) {
      bits.push(getWordBit(words[w], b));
    }
  }
  return bits;
}

/**
 * Compute parity of a subset of bits in a state vector
 */
function parityOfBlock(words: number[], startBit: number, blockSize: number): number {
  let parity = 0;
  for (let i = startBit; i < startBit + blockSize && i < 256; i++) {
    const wordIdx = Math.floor(i / 32);
    const bitIdx = i % 32;
    parity ^= getWordBit(words[wordIdx], bitIdx);
  }
  return parity;
}

// --- Collected State Vectors ---

interface RoundStateCollection {
  baseState: number[]; // 8 u32 words
  flippedStates: number[][]; // 256 × 8 u32 words
}

/**
 * Collect all state vectors at each round for base + 256 flipped inputs.
 * This is the foundational computation: 257 compressBlock calls.
 */
function collectRoundStates(
  inputBlock: Uint8Array,
  onProgress?: (pct: number) => void
): RoundStateCollection[] {
  // Base trace
  const baseTrace = compressBlock(inputBlock);

  // All 64 round states from base
  const results: RoundStateCollection[] = [];
  for (let r = 0; r < 64; r++) {
    results.push({
      baseState: roundToWords(baseTrace.rounds[r]),
      flippedStates: [],
    });
  }

  // For each of 256 bit flips, compute full trace and collect
  for (let bit = 0; bit < 256; bit++) {
    const modifiedBlock = flipBit(inputBlock, bit);
    const trace = compressBlock(modifiedBlock);

    for (let r = 0; r < 64; r++) {
      results[r].flippedStates.push(roundToWords(trace.rounds[r]));
    }

    if (onProgress && bit % 32 === 0) {
      onProgress(Math.round((bit / 256) * 60)); // 0-60% for collection phase
    }
  }

  return results;
}

// --- Discrete Box-Counting ---

/**
 * Compute discrete box-counting dimension for a single round.
 *
 * Method:
 * - We have 257 points (base + 256 flipped states) in {0,1}^256
 * - At scale r, a ball of Hamming radius r covers points within distance r
 * - N(r) = total_points / max_points_in_any_ball_of_radius_r
 *   (This measures how "spread out" the point set is)
 * - D(r1,r2) = -log(N(r2)/N(r1)) / log(r2/r1)
 * - For random-like diffusion, D ≈ 256 (full dimension of the hypercube)
 * - Anomaly: D < 200 indicates dimensional collapse
 */
export function computeBoxCounting(
  collection: RoundStateCollection
): DiscreteBoxCountingResult {
  const allStates = [collection.baseState, ...collection.flippedStates];
  const n = allStates.length; // 257

  // Precompute pairwise Hamming distances
  const distances: number[][] = Array.from({ length: n }, () => new Array(n).fill(0));
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      const d = hammingDistance(allStates[i], allStates[j]);
      distances[i][j] = d;
      distances[j][i] = d;
    }
  }

  const scales = [1, 2, 4, 8, 16, 32, 64, 128];
  const ballCounts: number[] = [];

  for (const r of scales) {
    // For each point, count how many other points fall within Hamming distance r
    let maxBallSize = 0;
    for (let i = 0; i < n; i++) {
      let ballSize = 1; // count self
      for (let j = 0; j < n; j++) {
        if (i !== j && distances[i][j] <= r) {
          ballSize++;
        }
      }
      if (ballSize > maxBallSize) maxBallSize = ballSize;
    }
    // N(r) = total / max_density = n / max_ball_fraction
    // Higher N(r) = more spread out
    const N_r = maxBallSize > 0 ? n / maxBallSize : n;
    ballCounts.push(N_r);
  }

  // Compute dimension estimates between consecutive scales
  const dimensionEstimates: number[] = [];
  for (let i = 0; i < scales.length - 1; i++) {
    const r1 = scales[i];
    const r2 = scales[i + 1];
    const N1 = ballCounts[i];
    const N2 = ballCounts[i + 1];

    if (N1 > 0 && N2 > 0 && N2 / N1 > 0 && r2 / r1 > 1) {
      const D = -Math.log(N2 / N1) / Math.log(r2 / r1);
      dimensionEstimates.push(D);
    } else {
      dimensionEstimates.push(256); // fallback to full dimension
    }
  }

  const averageDimension =
    dimensionEstimates.length > 0
      ? dimensionEstimates.reduce((a, b) => a + b, 0) / dimensionEstimates.length
      : 256;

  // Anomaly detection: D < 200 at any scale
  let hasAnomaly = false;
  let anomalyScale: number | null = null;
  let anomalyDimension: number | null = null;

  for (let i = 0; i < dimensionEstimates.length; i++) {
    if (dimensionEstimates[i] < 200) {
      hasAnomaly = true;
      anomalyScale = scales[i];
      anomalyDimension = dimensionEstimates[i];
      break; // report first anomaly
    }
  }

  return {
    round: -1, // caller sets this
    scales,
    ballCounts,
    dimensionEstimates,
    averageDimension,
    hasAnomaly,
    anomalyScale,
    anomalyDimension,
  };
}

// --- Walsh-Hadamard Spectrum ---

/**
 * Compute Walsh-Hadamard spectral analysis for a single round.
 *
 * Method:
 * - For each (input_bit, output_bit) pair, the "correlation" is:
 *   the fraction of the 256 bit flips that cause output_bit to change
 * - This measures Boolean function linearity
 * - Spectral flatness = geometric_mean(|correlations|) / arithmetic_mean(|correlations|)
 * - For random: flatness ≈ 1.0, for structured: < 1.0
 * - Anomaly: flatness < 0.95 or any |correlation| > 0.3
 */
export function computeWalshSpectrum(
  collection: RoundStateCollection
): WalshSpectrumResult {
  const baseBits = stateToBits(collection.baseState);

  // For each input bit flip, which output bits changed?
  const changeMatrix: boolean[][] = []; // [inputBit][outputBit]
  for (let inputBit = 0; inputBit < 256; inputBit++) {
    const flippedBits = stateToBits(collection.flippedStates[inputBit]);
    const changed: boolean[] = [];
    for (let outputBit = 0; outputBit < 256; outputBit++) {
      changed.push(baseBits[outputBit] !== flippedBits[outputBit]);
    }
    changeMatrix.push(changed);
  }

  // Per output bit: average absolute correlation
  const avgAbsCorrelation: number[] = [];
  const perOutputCounts: number[] = []; // raw counts for anomaly detection

  for (let outputBit = 0; outputBit < 256; outputBit++) {
    let count = 0;
    for (let inputBit = 0; inputBit < 256; inputBit++) {
      if (changeMatrix[inputBit][outputBit]) count++;
    }
    const corr = count / 256;
    avgAbsCorrelation.push(corr);
    perOutputCounts.push(count);
  }

  // Max correlation (across all output bits)
  const maxCorrelation = Math.max(...avgAbsCorrelation);

  // Find anomalies: any individual (inputBit, outputBit) with very high correlation
  // We compute per-input-bit influence on each output bit
  const anomalies: { inputBit: number; outputBit: number; correlation: number }[] = [];

  // For anomaly detection: for each output bit, check if any single input bit
  // has disproportionate influence. Since each input bit flip either changes an
  // output bit or not, the correlation per (input, output) pair is 0 or 1 for a
  // single sample. The aggregate correlation is the fraction across all 256 inputs.
  // An anomaly is when a single input bit's contribution dominates.

  // We look for output bits where the aggregate correlation is > 0.3
  for (let outputBit = 0; outputBit < 256; outputBit++) {
    if (avgAbsCorrelation[outputBit] > 0.3) {
      // Find which input bits contribute
      for (let inputBit = 0; inputBit < 256; inputBit++) {
        if (changeMatrix[inputBit][outputBit]) {
          anomalies.push({
            inputBit,
            outputBit,
            correlation: 1.0, // single-sample correlation is binary
          });
          if (anomalies.length >= 50) break; // cap for performance
        }
      }
      if (anomalies.length >= 50) break;
    }
  }

  // Also detect anomalies where aggregate correlation < 0.05 (too low = not mixing well)
  for (let outputBit = 0; outputBit < 256; outputBit++) {
    if (avgAbsCorrelation[outputBit] < 0.05 && perOutputCounts[outputBit] < 13) {
      // This output bit barely changes — find which input bits DO affect it
      for (let inputBit = 0; inputBit < 256; inputBit++) {
        if (changeMatrix[inputBit][outputBit]) {
          anomalies.push({
            inputBit,
            outputBit,
            correlation: 1.0,
          });
          if (anomalies.length >= 100) break;
        }
      }
      if (anomalies.length >= 100) break;
    }
  }

  // Spectral flatness: geometric mean / arithmetic mean of |correlations|
  const absCorrs = avgAbsCorrelation.map((c) => Math.max(c, 1e-10)); // avoid log(0)
  const logSum = absCorrs.reduce((s, c) => s + Math.log(c), 0);
  const geoMean = Math.exp(logSum / absCorrs.length);
  const ariMean = absCorrs.reduce((s, c) => s + c, 0) / absCorrs.length;
  const spectralFlatness = ariMean > 0 ? geoMean / ariMean : 0;

  return {
    round: -1,
    avgAbsCorrelation,
    maxCorrelation,
    anomalies: anomalies.slice(0, 50), // cap for display
    spectralFlatness,
  };
}

// --- Self-Similarity ---

/**
 * Compute discrete self-similarity analysis for a single round.
 *
 * Method:
 * - Compute Hamming distance histogram between all pairs of reachable states
 * - Coarsen by grouping bits into blocks of varying sizes and computing parity
 * - KL divergence between consecutive scale distributions
 * - Score = 1 / (1 + mean(KL_divergences))
 * - Self-similar = consistent distribution shape across scales
 */
export function computeSelfSimilarity(
  collection: RoundStateCollection
): SelfSimilarityResult {
  const allStates = [collection.baseState, ...collection.flippedStates];
  const n = allStates.length;

  // 1. Compute Hamming distance histogram
  const distanceHistogram = new Array(257).fill(0); // distances 0..256
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      const d = hammingDistance(allStates[i], allStates[j]);
      distanceHistogram[d]++;
    }
  }
  // Include self-distances (0): n points each at distance 0 from themselves
  distanceHistogram[0] = n;

  // 2. Compute distributions at different scales (block sizes)
  // Scale = block size for parity grouping: [1, 2, 4, 8, 16, 32]
  // At scale s, we group consecutive bits into blocks of size s,
  // compute parity of each block, and measure "reduced Hamming distance"
  const blockSizes = [1, 2, 4, 8, 16, 32];
  const distributions: number[][] = [];

  for (const blockSize of blockSizes) {
    const numBlocks = Math.ceil(256 / blockSize);
    // For each state, compute reduced representation (parities)
    const reducedStates: number[][] = allStates.map((words) => {
      const parities: number[] = [];
      for (let b = 0; b < numBlocks; b++) {
        parities.push(parityOfBlock(words, b * blockSize, blockSize));
      }
      return parities;
    });

    // Compute reduced Hamming distance histogram
    const maxDist = numBlocks;
    const hist = new Array(maxDist + 1).fill(0);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let d = 0;
        for (let k = 0; k < numBlocks; k++) {
          if (reducedStates[i][k] !== reducedStates[j][k]) d++;
        }
        hist[d]++;
      }
    }
    hist[0] = n; // self-distances
    distributions.push(hist);
  }

  // 3. Compute KL divergence between consecutive scale distributions
  const scaleDivergences: number[] = [];
  for (let i = 0; i < distributions.length - 1; i++) {
    const p = normalizeDistribution(distributions[i]);
    const q = normalizeDistribution(distributions[i + 1]);

    // KL divergence with proper handling of zeros
    let kl = 0;
    const len = Math.max(p.length, q.length);
    for (let k = 0; k < len; k++) {
      const pi = k < p.length ? p[k] : 1e-10;
      const qi = k < q.length ? q[k] : 1e-10;
      if (pi > 1e-10) {
        kl += pi * Math.log(pi / Math.max(qi, 1e-10));
      }
    }
    scaleDivergences.push(kl);
  }

  // 4. Self-similarity score
  const meanKL =
    scaleDivergences.length > 0
      ? scaleDivergences.reduce((a, b) => a + b, 0) / scaleDivergences.length
      : 0;
  const selfSimilarityScore = 1 / (1 + meanKL);

  // 5. Anomaly detection
  const isAnomalous = selfSimilarityScore < 0.3 || scaleDivergences.some((d) => d > 5);

  return {
    round: -1,
    distanceHistogram,
    scaleDivergences,
    selfSimilarityScore,
    isAnomalous,
  };
}

/**
 * Normalize a histogram into a probability distribution
 */
function normalizeDistribution(hist: number[]): number[] {
  const total = hist.reduce((a, b) => a + b, 0);
  if (total === 0) return hist.map(() => 1 / hist.length);
  return hist.map((c) => c / total);
}

// --- Cluster Tree ---

/**
 * Compute single-linkage cluster analysis for a single round.
 *
 * Method:
 * - Compute pairwise Hamming distances
 * - Single-linkage clustering at thresholds [8, 16, 32, 64, 128]
 * - Count clusters at each threshold
 * - Imbalance = max_cluster_size / total - 1/n_clusters
 */
export function computeClusterTree(
  collection: RoundStateCollection
): ClusterTreeResult {
  const allStates = [collection.baseState, ...collection.flippedStates];
  const n = allStates.length;

  // Precompute pairwise distances (upper triangle only, then use Union-Find)
  const thresholds = [8, 16, 32, 64, 128];
  const clusterCounts: number[] = [];
  let maxImbalance = 0;
  let maxClusterFraction = 0;

  for (const threshold of thresholds) {
    // Union-Find for single-linkage clustering
    const parent = new Array(n).fill(0).map((_, i) => i);
    const rank = new Array(n).fill(0);

    function find(x: number): number {
      if (parent[x] !== x) parent[x] = find(parent[x]);
      return parent[x];
    }

    function union(x: number, y: number): void {
      const px = find(x);
      const py = find(y);
      if (px === py) return;
      if (rank[px] < rank[py]) parent[px] = py;
      else if (rank[px] > rank[py]) parent[py] = px;
      else {
        parent[py] = px;
        rank[px]++;
      }
    }

    // Union pairs within threshold
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        if (hammingDistance(allStates[i], allStates[j]) <= threshold) {
          union(i, j);
        }
      }
    }

    // Count clusters and their sizes
    const clusterSizes = new Map<number, number>();
    for (let i = 0; i < n; i++) {
      const root = find(i);
      clusterSizes.set(root, (clusterSizes.get(root) || 0) + 1);
    }

    const numClusters = clusterSizes.size;
    clusterCounts.push(numClusters);

    const sizes = Array.from(clusterSizes.values());
    const maxCluster = Math.max(...sizes);
    maxClusterFraction = Math.max(maxClusterFraction, maxCluster / n);

    // Imbalance: how uneven are the cluster sizes
    // 0 = perfectly balanced, 1 = maximally imbalanced
    if (numClusters > 1) {
      const avgSize = n / numClusters;
      const variance = sizes.reduce((s, sz) => s + (sz - avgSize) ** 2, 0) / numClusters;
      const maxVariance = (n - avgSize) ** 2 * (numClusters - 1) / numClusters + avgSize ** 2 / numClusters;
      const imbalance = maxVariance > 0 ? variance / maxVariance : 0;
      maxImbalance = Math.max(maxImbalance, imbalance);
    }
  }

  const imbalance = maxImbalance;
  const isAnomalous = maxClusterFraction > 0.9 || imbalance > 0.8;

  return {
    round: -1,
    clusterCounts,
    imbalance,
    maxClusterFraction,
    isAnomalous,
  };
}

// --- Dimension Profile ---

/**
 * Combine box-counting results into a profile of (scale, dimension) per round.
 * Look for dimension dips that indicate structural anomalies.
 */
export function computeDimensionProfile(
  boxCounting: DiscreteBoxCountingResult
): DimensionProfile {
  const profile: [number, number][] = [];
  let minDim = 256;
  let minDimScale = 1;

  for (let i = 0; i < boxCounting.scales.length - 1; i++) {
    const scale = boxCounting.scales[i];
    const dim = boxCounting.dimensionEstimates[i];
    profile.push([scale, dim]);
    if (dim < minDim) {
      minDim = dim;
      minDimScale = scale;
    }
  }

  // Add last scale with its own dimension estimate (use average)
  profile.push([
    boxCounting.scales[boxCounting.scales.length - 1],
    boxCounting.averageDimension,
  ]);

  return {
    round: boxCounting.round,
    profile,
    minDimension: minDim,
    minDimensionScale: minDimScale,
    isAnomaly: boxCounting.hasAnomaly,
  };
}

// --- Main Entry Point ---

/**
 * Compute the full discrete fractal analysis for SHA-256.
 *
 * This computes state vectors for base + 256 bit flips across all 64 rounds,
 * then applies all five analysis methods.
 *
 * @param inputBlock - 64-byte input block
 * @param onProgress - Optional progress callback (0-100)
 * @returns FullDiscreteAnalysis with all results
 */
export function computeFullDiscreteAnalysis(
  inputBlock: Uint8Array,
  onProgress?: (pct: number) => void
): FullDiscreteAnalysis {
  // Phase 1: Collect all state vectors (60% of work)
  const roundCollections = collectRoundStates(inputBlock, onProgress);

  // Phase 2: Run analyses on each round (40% of work)
  const boxCounting: DiscreteBoxCountingResult[] = [];
  const walshSpectrum: WalshSpectrumResult[] = [];
  const selfSimilarity: SelfSimilarityResult[] = [];
  const clusterTree: ClusterTreeResult[] = [];
  const dimensionProfile: DimensionProfile[] = [];

  for (let r = 0; r < 64; r++) {
    const collection = roundCollections[r];

    const bc = computeBoxCounting(collection);
    bc.round = r;
    boxCounting.push(bc);

    const ws = computeWalshSpectrum(collection);
    ws.round = r;
    walshSpectrum.push(ws);

    const ss = computeSelfSimilarity(collection);
    ss.round = r;
    selfSimilarity.push(ss);

    const ct = computeClusterTree(collection);
    ct.round = r;
    clusterTree.push(ct);

    const dp = computeDimensionProfile(bc);
    dp.round = r;
    dimensionProfile.push(dp);

    if (onProgress && r % 8 === 0) {
      onProgress(60 + Math.round((r / 64) * 40));
    }
  }

  onProgress?.(100);

  return {
    boxCounting,
    walshSpectrum,
    selfSimilarity,
    clusterTree,
    dimensionProfile,
  };
}
