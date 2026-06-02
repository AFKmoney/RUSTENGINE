/**
 * Diffusion Analyzer for SHA-256 Avalanche Effect
 * 
 * Computes bit-level diffusion by comparing two compression traces:
 * one base trace and one with a single input bit flipped.
 */

import {
  compressBlock,
  flipBit,
  getWordBit,
  type CompressionTrace,
  type RoundState,
} from "./sha256-engine";

export interface DiffusionData {
  round: number;
  // For each of the 8 state words (a-h), which bits differ from base?
  bitDiffs: boolean[][]; // [wordIndex 0..7][bitIndex 0..31]
  // Percentage of total bits that changed (out of 256 bits)
  diffusionPercent: number;
  // Per-word diffusion percentages
  wordDiffusionPercents: number[];
  // Entropy estimate (bits of uncertainty based on number of changed bits)
  entropy: number;
  // Number of active (changed) bits
  activeBitCount: number;
}

export interface FullAnalysis {
  baseTrace: CompressionTrace;
  modifiedTrace: CompressionTrace;
  flipBitIndex: number;
  diffusion: DiffusionData[];
}

export interface AvalancheProfile {
  // For each of the 256 input bits, the diffusion % at round 63
  profile: number[];
  // Average diffusion across all bits
  averageDiffusion: number;
  // Min diffusion
  minDiffusion: number;
  // Max diffusion
  maxDiffusion: number;
}

const WORD_NAMES = ["a", "b", "c", "d", "e", "f", "g", "h"];

/**
 * Extract 8 state words from a RoundState as an array
 */
function roundToWords(rs: RoundState): number[] {
  return [rs.a, rs.b, rs.c, rs.d, rs.e, rs.f, rs.g, rs.h];
}

/**
 * Compute diffusion data for each round by comparing two traces.
 */
export function computeDiffusion(
  base: CompressionTrace,
  modified: CompressionTrace
): DiffusionData[] {
  // We include the initial state as round -1 (the hash IV) but in practice
  // we compare starting from round 0
  const result: DiffusionData[] = [];

  for (let r = 0; r < 64; r++) {
    const baseWords = roundToWords(base.rounds[r]);
    const modWords = roundToWords(modified.rounds[r]);

    const bitDiffs: boolean[][] = [];
    let totalChanged = 0;
    const wordDiffusionPercents: number[] = [];

    for (let w = 0; w < 8; w++) {
      const wordDiffs: boolean[] = [];
      let wordChanged = 0;

      for (let b = 0; b < 32; b++) {
        const baseBit = getWordBit(baseWords[w], b);
        const modBit = getWordBit(modWords[w], b);
        const changed = baseBit !== modBit;
        wordDiffs.push(changed);
        if (changed) {
          wordChanged++;
          totalChanged++;
        }
      }

      bitDiffs.push(wordDiffs);
      wordDiffusionPercents.push((wordChanged / 32) * 100);
    }

    const diffusionPercent = (totalChanged / 256) * 100;

    // Entropy estimate: if p bits changed out of 256,
    // the entropy is approximately -sum(p_i * log2(p_i)) for each bit
    // Simplified: use the fraction of changed bits as a binomial entropy
    const p = totalChanged / 256;
    const entropy =
      p > 0 && p < 1
        ? -(p * Math.log2(p) + (1 - p) * Math.log2(1 - p)) * 256
        : p === 0 || p === 1
          ? 0
          : 256;

    result.push({
      round: r,
      bitDiffs,
      diffusionPercent,
      wordDiffusionPercents,
      entropy,
      activeBitCount: totalChanged,
    });
  }

  return result;
}

/**
 * Run a full diffusion analysis: SHA-256 twice (base + 1 bit flip), compute diffusion.
 */
export function computeFullAnalysis(
  inputBlock: Uint8Array,
  flipBitIndex: number,
  hashState?: number[]
): FullAnalysis {
  const baseTrace = compressBlock(inputBlock, hashState);
  const modifiedBlock = flipBit(inputBlock, flipBitIndex);
  const modifiedTrace = compressBlock(modifiedBlock, hashState);
  const diffusion = computeDiffusion(baseTrace, modifiedTrace);

  return {
    baseTrace,
    modifiedTrace,
    flipBitIndex,
    diffusion,
  };
}

/**
 * Compute the full avalanche profile: for each of the 256 input bits,
 * what percentage of output bits change at round 63.
 */
export function computeAvalancheProfile(
  inputBlock: Uint8Array,
  hashState?: number[]
): AvalancheProfile {
  const baseTrace = compressBlock(inputBlock, hashState);
  const profile: number[] = [];

  for (let bit = 0; bit < 256; bit++) {
    const modifiedBlock = flipBit(inputBlock, bit);
    const modifiedTrace = compressBlock(modifiedBlock, hashState);
    const diffusion = computeDiffusion(baseTrace, modifiedTrace);
    // Use round 63 (last round) diffusion %
    profile.push(diffusion[63].diffusionPercent);
  }

  const averageDiffusion = profile.reduce((a, b) => a + b, 0) / profile.length;
  const minDiffusion = Math.min(...profile);
  const maxDiffusion = Math.max(...profile);

  return { profile, averageDiffusion, minDiffusion, maxDiffusion };
}

/**
 * Compute the influence map: for a given input bit flip,
 * which output bits does it influence at each round.
 */
export function computeInfluenceMap(
  diffusion: DiffusionData[],
  roundIndex: number
): Map<string, number[]> {
  const influence = new Map<string, number[]>();

  if (roundIndex < 0 || roundIndex >= diffusion.length) return influence;

  const d = diffusion[roundIndex];
  for (let w = 0; w < 8; w++) {
    for (let b = 0; b < 32; b++) {
      if (d.bitDiffs[w][b]) {
        const key = `${WORD_NAMES[w]}-${b}`;
        if (!influence.has(key)) {
          influence.set(key, []);
        }
        influence.get(key)!.push(roundIndex);
      }
    }
  }

  return influence;
}

/**
 * Get the word-level heatmap data (8 words × 64 rounds)
 * Each cell is the diffusion % for that word at that round.
 */
export function getWordHeatmapData(diffusion: DiffusionData[]): number[][] {
  const heatmap: number[][] = [];
  for (let w = 0; w < 8; w++) {
    const row: number[] = [];
    for (let r = 0; r < 64; r++) {
      row.push(diffusion[r].wordDiffusionPercents[w]);
    }
    heatmap.push(row);
  }
  return heatmap;
}

/**
 * Find the avalanche point: the round at which diffusion first reaches 50%.
 */
export function findAvalanchePoint(diffusion: DiffusionData[]): number {
  for (let i = 0; i < diffusion.length; i++) {
    if (diffusion[i].diffusionPercent >= 50) {
      return i;
    }
  }
  return -1; // Never reached 50%
}

export { WORD_NAMES };
