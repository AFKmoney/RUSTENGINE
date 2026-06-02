// ═══════════════════════════════════════════════════════════════════
// VORTEX PRIME — Discrete Fractal Analysis Engine
// Box-counting on {0,1}^256, Walsh-Hadamard, Self-similarity,
// Hamming clustering, Resonance Scanner
// ═══════════════════════════════════════════════════════════════════

// ── Discrete Box-Counting on Hamming Space ──
// Measure how the number of occupied Hamming balls scales with radius
function computeDiscreteBoxCounting(roundStates) {
  const N = roundStates.length;
  if (N < 2) return { dimensions: [], scales: [] };

  // Convert states to 256-bit binary vectors
  const bitVectors = roundStates.map(s => {
    const bits = [];
    for (let w = 0; w < 8; w++) {
      for (let b = 31; b >= 0; b--) {
        bits.push((s[w] >>> b) & 1);
      }
    }
    return bits;
  });

  // Compute pairwise Hamming distances
  const distances = [];
  for (let i = 0; i < N; i++) {
    for (let j = i + 1; j < N; j++) {
      let d = 0;
      for (let k = 0; k < 256; k++) {
        if (bitVectors[i][k] !== bitVectors[j][k]) d++;
      }
      distances.push(d);
    }
  }

  // Box-counting at multiple Hamming radii
  const scales = [4, 8, 16, 32, 48, 64, 80, 96, 112, 128];
  const counts = [];

  for (const r of scales) {
    // Count distinct Hamming balls of radius r needed to cover all points
    const covered = new Set();
    let ballCount = 0;
    const uncovered = Array.from({length: N}, (_, i) => i);

    while (uncovered.length > 0) {
      const center = uncovered[0];
      ballCount++;
      for (let j = uncovered.length - 1; j >= 0; j--) {
        let d = 0;
        for (let k = 0; k < 256; k++) {
          if (bitVectors[center][k] !== bitVectors[uncovered[j]][k]) d++;
          if (d > r) break;
        }
        if (d <= r) {
          covered.add(uncovered[j]);
          uncovered.splice(j, 1);
        }
      }
    }
    counts.push(ballCount);
  }

  // Estimate fractal dimension from log-log slope
  const dimensions = [];
  for (let i = 1; i < scales.length; i++) {
    if (counts[i] > 0 && counts[i-1] > 0) {
      const d = -((Math.log(counts[i]) - Math.log(counts[i-1])) /
                  (Math.log(scales[i]) - Math.log(scales[i-1])));
      dimensions.push({ scale: scales[i], dimension: d });
    }
  }

  return { scales, counts, dimensions };
}

// ── Walsh-Hadamard Spectrum ──
// Spectral analysis of Boolean functions extracted from SHA-256 state
// For each output bit, compute the Walsh spectrum of the Boolean function
function computeWalshHadamard(roundStates) {
  const N = roundStates.length;
  if (N < 4) return { spectralFlatness: 0, maxCorrelation: 0, nonlinearity: 0, spectrum: [] };

  // Extract first 8 output bits as Boolean functions of round number
  // Use bit 0 of each word as our Boolean functions
  const boolFns = [];
  for (let w = 0; w < 8; w++) {
    const fn = [];
    for (let r = 0; r < N; r++) {
      fn.push((roundStates[r][w] >>> 31) & 1);
    }
    boolFns.push(fn);
  }

  const spectra = [];
  let totalFlatness = 0;
  let maxCorr = 0;
  let totalNonlinearity = 0;

  for (const fn of boolFns) {
    // Walsh-Hadamard transform on sampled function
    const n = fn.length;
    const W = new Float64Array(n);
    for (let i = 0; i < n; i++) W[i] = fn[i] ? 1 : -1;

    // Fast Walsh-Hadamard Transform
    let h = 1;
    while (h < n) {
      for (let i = 0; i < n; i += h * 2) {
        for (let j = i; j < i + h; j++) {
          const x = W[j];
          const y = W[j + h];
          W[j] = x + y;
          W[j + h] = x - y;
        }
      }
      h *= 2;
    }

    const absW = Array.from(W).map(Math.abs);
    const maxSpec = Math.max(...absW);
    const meanSpec = absW.reduce((a,b) => a+b, 0) / absW.length;

    const flatness = meanSpec > 0 ? (maxSpec / meanSpec) : 0;
    const nonlinearity = (n / 2) - (maxSpec / 2);

    totalFlatness += flatness;
    maxCorr = Math.max(maxCorr, maxSpec);
    totalNonlinearity += nonlinearity;

    spectra.push({ values: Array.from(W).slice(0, 64), maxCorrelation: maxSpec, flatness, nonlinearity });
  }

  return {
    spectralFlatness: totalFlatness / boolFns.length,
    maxCorrelation: maxCorr,
    nonlinearity: totalNonlinearity / boolFns.length,
    spectra
  };
}

// ── Self-Similarity Detector on Hamming Space ──
// Check if the state trajectory looks self-similar across scales
function computeSelfSimilarity(roundStates) {
  const N = roundStates.length;
  if (N < 8) return { similarity: 0, scales: [], ratios: [] };

  // Compute Hamming distance matrix
  const distMatrix = [];
  for (let i = 0; i < N; i++) {
    const row = [];
    for (let j = 0; j < N; j++) {
      let d = 0;
      for (let w = 0; w < 8; w++) {
        const xor = roundStates[i][w] ^ roundStates[j][w];
        d += popcount32(xor);
      }
      row.push(d);
    }
    distMatrix.push(row);
  }

  // Measure self-similarity at different scales
  const scales = [1, 2, 4, 8, 16];
  const ratios = [];

  for (const s of scales) {
    if (N <= s * 2) continue;

    // Compare distance distributions at scale s vs scale 1
    const dists1 = [];
    const distsS = [];

    for (let i = 0; i < N - 1; i++) {
      dists1.push(distMatrix[i][i + 1]);
      if (i + s < N) {
        distsS.push(distMatrix[i][i + s]);
      }
    }

    if (dists1.length === 0 || distsS.length === 0) continue;

    const mean1 = dists1.reduce((a,b) => a+b, 0) / dists1.length;
    const meanS = distsS.reduce((a,b) => a+b, 0) / distsS.length;

    const ratio = mean1 > 0 ? meanS / (mean1 * s) : 0;
    ratios.push({ scale: s, ratio, meanNear: mean1, meanFar: meanS });
  }

  // Self-similarity: how close ratios are to each other
  let similarity = 0;
  if (ratios.length >= 2) {
    const meanRatio = ratios.reduce((a,r) => a + r.ratio, 0) / ratios.length;
    const variance = ratios.reduce((a,r) => a + (r.ratio - meanRatio) ** 2, 0) / ratios.length;
    similarity = 1 / (1 + Math.sqrt(variance) * 10);
  }

  return { similarity, scales, ratios };
}

// ── Resonance Scanner ──
// Scan (round × scale) for anomalous fractal structure
// Anomaly = deviation from expected random-walk diffusion
function computeResonanceScanner(roundStates) {
  const N = roundStates.length;
  if (N < 4) return { matrix: [], anomalyRounds: [], anomalyScales: [], maxAnomaly: 0 };

  // Scales for Hamming ball radius
  const scales = [4, 8, 16, 32, 64, 96, 128];
  // Round windows
  const roundWindows = [];
  for (let start = 0; start < N; start += 8) {
    const end = Math.min(start + 8, N);
    if (end - start >= 4) roundWindows.push({ start, end, label: `R${start}-${end}` });
  }

  const matrix = [];
  let maxAnomaly = 0;
  const anomalyRounds = new Set();
  const anomalyScales = new Set();

  for (const rw of roundWindows) {
    const row = [];
    const windowStates = roundStates.slice(rw.start, rw.end);

    // Compute local Hamming distance distribution
    const dists = [];
    for (let i = 0; i < windowStates.length; i++) {
      for (let j = i + 1; j < windowStates.length; j++) {
        let d = 0;
        for (let w = 0; w < 8; w++) {
          d += popcount32(windowStates[i][w] ^ windowStates[j][w]);
        }
        dists.push(d);
      }
    }

    const meanDist = dists.length > 0 ? dists.reduce((a,b) => a+b, 0) / dists.length : 0;
    // Expected for random 256-bit strings: 128
    const expectedDist = 128;
    // Variance for random: 64
    const expectedStd = 8;

    for (const s of scales) {
      // Count states within Hamming radius s
      let inBall = 0;
      let total = 0;
      for (const d of dists) {
        total++;
        if (d <= s) inBall++;
      }

      const observedDensity = total > 0 ? inBall / total : 0;
      // Expected density for random: CDF of binomial(256, 0.5) at s
      // Approximation using normal CDF
      const zScore = s >= 128 ? 1 : (s < 128 ? normCDF((s - expectedDist) / expectedStd) : 0.5);
      const expectedDensity = zScore;

      // Anomaly score: deviation from expected
      const anomaly = Math.abs(observedDensity - expectedDensity) * 10;
      row.push(anomaly);

      if (anomaly > maxAnomaly) maxAnomaly = anomaly;
      if (anomaly > 3) {
        anomalyRounds.add(rw.label);
        anomalyScales.add(s);
      }
    }
    matrix.push({ round: rw.label, values: row });
  }

  return {
    matrix,
    scales,
    anomalyRounds: Array.from(anomalyRounds),
    anomalyScales: Array.from(anomalyScales),
    maxAnomaly
  };
}

// ── Helper functions ──
function popcount32(x) {
  x = x - ((x >>> 1) & 0x55555555);
  x = (x & 0x33333333) + ((x >>> 2) & 0x33333333);
  return (((x + (x >>> 4)) & 0x0F0F0F0F) * 0x01010101) >>> 24;
}

function normCDF(x) {
  // Approximation of standard normal CDF
  const a1 = 0.254829592, a2 = -0.284496736, a3 = 1.421413741;
  const a4 = -1.453152027, a5 = 1.061405429, p = 0.3275911;
  const sign = x < 0 ? -1 : 1;
  x = Math.abs(x) / Math.SQRT2;
  const t = 1 / (1 + p * x);
  const y = 1 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * Math.exp(-x * x);
  return 0.5 * (1 + sign * y);
}

// ── Full Fractal Analysis Pipeline ──
function runFullFractalAnalysis(roundStates) {
  const boxCounting = computeDiscreteBoxCounting(roundStates);
  const walshHadamard = computeWalshHadamard(roundStates);
  const selfSimilarity = computeSelfSimilarity(roundStates);
  const resonance = computeResonanceScanner(roundStates);

  return { boxCounting, walshHadamard, selfSimilarity, resonance };
}

window.computeDiscreteBoxCounting = computeDiscreteBoxCounting;
window.computeWalshHadamard = computeWalshHadamard;
window.computeSelfSimilarity = computeSelfSimilarity;
window.computeResonanceScanner = computeResonanceScanner;
window.runFullFractalAnalysis = runFullFractalAnalysis;
