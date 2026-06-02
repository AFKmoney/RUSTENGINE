// ═══════════════════════════════════════════════════════════════════════════
// VORTEX PRIME — Inversion Engine
// Uses discrete fractal analysis (resonance anomalies, Walsh-Hadamard
// spectral biases, self-similarity structure) to guide input selection
// for SHA-256 inversion.
//
// Strategy:
// 1. Hash the known target (pubkey/address) to get reference state
// 2. Run fractal analysis on the reference trajectory
// 3. Use resonance anomalies to identify "weak" rounds & scales
// 4. Use Walsh-Hadamard biases to predict which input bits influence
//    which output bits with anomalous correlation
// 5. Construct candidate inputs guided by these anomalies
// 6. Verify candidates against the target hash
// ═══════════════════════════════════════════════════════════════════════════

class InversionEngine {
  constructor() {
    this.sha256 = new SHA256Engine();
    this.targetHash = null;
    this.targetStates = null;
    this.fractalResult = null;
    this.inversionLog = [];
    this.candidates = [];
    this.running = false;
    this.iteration = 0;
    this.bestHamming = 256;
    this.bestCandidate = null;
    this.anomalyMap = null;
  }

  // ── Step 1: Initialize with target ──
  // Target = the hash we want to invert (SHA-256 of the pubkey)
  initTarget(pubkeyHex) {
    const pubkeyBytes = this.sha256.hexToBytes(pubkeyHex);
    const result = this.sha256.hashWithStates(pubkeyBytes);
    this.targetHash = result.hashHex;
    this.targetStates = result.roundStates;

    // Run fractal analysis on target trajectory
    this.fractalResult = runFullFractalAnalysis(this.targetStates);

    // Build anomaly map from resonance scanner
    this.anomalyMap = this._buildAnomalyMap();

    this.inversionLog = [];
    this.candidates = [];
    this.iteration = 0;
    this.bestHamming = 256;
    this.bestCandidate = null;

    return {
      targetHash: this.targetHash,
      fractalResult: this.fractalResult,
      anomalyMap: this.anomalyMap,
      resonanceAnomalies: this.fractalResult.resonance.anomalyRounds,
      spectralBias: this._extractSpectralBias(),
      selfSimScore: this.fractalResult.selfSimilarity.similarity
    };
  }

  // ── Build anomaly map from resonance scanner ──
  // Returns: { weakRounds: [round indices], weakScales: [scale values],
  //            anomalyMatrix: 2D array, topAnomalies: [{round, scale, score}] }
  _buildAnomalyMap() {
    const res = this.fractalResult.resonance;
    if (!res || !res.matrix || res.matrix.length === 0) {
      return { weakRounds: [], weakScales: [], anomalyMatrix: [], topAnomalies: [] };
    }

    const topAnomalies = [];
    const weakRounds = new Set();
    const weakScales = new Set();

    for (const row of res.matrix) {
      for (let s = 0; s < row.values.length; s++) {
        if (row.values[s] > 2.0) { // Threshold for anomaly
          topAnomalies.push({
            round: row.round,
            scale: res.scales[s],
            score: row.values[s]
          });
          weakRounds.add(row.round);
          weakScales.add(res.scales[s]);
        }
      }
    }

    topAnomalies.sort((a, b) => b.score - a.score);

    return {
      weakRounds: Array.from(weakRounds),
      weakScales: Array.from(weakScales),
      anomalyMatrix: res.matrix,
      topAnomalies: topAnomalies.slice(0, 20)
    };
  }

  // ── Extract spectral bias from Walsh-Hadamard ──
  // Which input bits have strongest influence on output bits
  _extractSpectralBias() {
    const wh = this.fractalResult.walshHadamard;
    if (!wh || !wh.spectra) return { biasedWords: [], maxCorrelation: 0 };

    const biasedWords = [];
    for (let i = 0; i < wh.spectra.length; i++) {
      if (wh.spectra[i].flatness > 2.0) {
        biasedWords.push({
          word: i,
          flatness: wh.spectra[i].flatness,
          maxCorrelation: wh.spectra[i].maxCorrelation
        });
      }
    }

    return { biasedWords, maxCorrelation: wh.maxCorrelation };
  }

  // ── Step 2: Generate guided candidate inputs ──
  // Uses anomaly structure to bias the search
  generateCandidate(iteration) {
    const candidate = new Uint8Array(32); // 256-bit input space

    // Strategy depends on what anomalies we found
    const hasAnomalies = this.anomalyMap.topAnomalies.length > 0;
    const hasSpectralBias = this._extractSpectralBias().biasedWords.length > 0;

    if (hasAnomalies && iteration < 1000) {
      // Phase 1: Exploit resonance anomalies
      // Focus perturbation on weak scales
      const weakScales = this.anomalyMap.weakScales;
      const primaryScale = weakScales.length > 0 ? weakScales[0] : 64;

      // Generate input with structured bit pattern at the weak scale
      for (let i = 0; i < 32; i++) {
        // Create bits with periodicity matching weak scale
        const bitPeriod = Math.max(1, Math.floor(primaryScale / 8));
        candidate[i] = ((iteration * 7 + i * bitPeriod) ^ (iteration >> 3)) & 0xff;
      }

      // XOR with spectral bias pattern
      if (hasSpectralBias) {
        const bias = this._extractSpectralBias();
        for (const bw of bias.biasedWords) {
          candidate[bw.word * 4] ^= (iteration & 0xff);
          candidate[bw.word * 4 + 1] ^= ((iteration >> 4) & 0xff);
        }
      }
    } else if (hasSpectralBias) {
      // Phase 2: Exploit Walsh-Hadamard spectral biases
      const bias = this._extractSpectralBias();
      for (let i = 0; i < 32; i++) {
        candidate[i] = (iteration * 31 + i * 17) & 0xff;
      }
      for (const bw of bias.biasedWords) {
        // Flip bits in biased words based on iteration
        const mask = ((iteration * bw.flatness) | 1) & 0xff;
        candidate[bw.word * 4] ^= mask;
      }
    } else {
      // Phase 3: Fractal-guided random walk
      // Use self-similarity structure to walk the Hamming space
      const simScore = this.fractalResult.selfSimilarity.similarity;

      if (this.bestCandidate && simScore > 0.1) {
        // Mutate best candidate with Hamming distance guided by self-similarity
        candidate.set(this.bestCandidate);
        const mutationRadius = Math.max(1, Math.floor(8 * (1 - simScore)));
        for (let m = 0; m < mutationRadius; m++) {
          const byteIdx = (iteration * 7 + m * 13) % 32;
          const bitIdx = (iteration * 3 + m * 11) % 8;
          candidate[byteIdx] ^= (1 << bitIdx);
        }
      } else {
        // Pseudo-random with deterministic seed
        for (let i = 0; i < 32; i++) {
          candidate[i] = ((iteration * 1103515245 + 12345 + i * 7919) >>> 0) & 0xff;
        }
      }
    }

    return candidate;
  }

  // ── Step 3: Evaluate candidate against target ──
  evaluateCandidate(candidateBytes) {
    const result = this.sha256.hashWithStates(candidateBytes);
    const candidateHash = result.hashHex;

    // Compute Hamming distance to target hash
    const targetBytes = this.sha256.hexToBytes(this.targetHash);
    let hammingDist = 0;
    for (let i = 0; i < 32; i++) {
      const xor = candidateBytes[i] ^ targetBytes[i];
      hammingDist += this.sha256.popcount(xor);
    }

    // Run fractal analysis on candidate trajectory
    const candidateFractal = runFullFractalAnalysis(result.roundStates);

    // Compare fractal signatures
    const resonanceDiff = this._compareResonance(
      this.fractalResult.resonance,
      candidateFractal.resonance
    );

    const spectralDiff = this._compareSpectra(
      this.fractalResult.walshHadamard,
      candidateFractal.walshHadamard
    );

    return {
      candidateHash,
      hammingDist,
      resonanceDiff,
      spectralDiff,
      roundStates: result.roundStates,
      fractalResult: candidateFractal
    };
  }

  // ── Compare resonance signatures ──
  _compareResonance(ref, test) {
    if (!ref.matrix || !test.matrix || ref.matrix.length === 0 || test.matrix.length === 0) {
      return { totalDiff: Infinity, minDiff: Infinity };
    }

    let totalDiff = 0;
    let minDiff = Infinity;
    const len = Math.min(ref.matrix.length, test.matrix.length);

    for (let i = 0; i < len; i++) {
      const refRow = ref.matrix[i];
      const testRow = test.matrix[i];
      const sLen = Math.min(refRow.values.length, testRow.values.length);

      for (let j = 0; j < sLen; j++) {
        const d = Math.abs(refRow.values[j] - testRow.values[j]);
        totalDiff += d;
        if (d < minDiff) minDiff = d;
      }
    }

    return { totalDiff, minDiff };
  }

  // ── Compare Walsh-Hadamard spectra ──
  _compareSpectra(ref, test) {
    if (!ref.spectra || !test.spectra || ref.spectra.length === 0) {
      return { correlation: 0, flatnessDiff: Infinity };
    }

    const len = Math.min(ref.spectra.length, test.spectra.length);
    let totalCorr = 0;

    for (let i = 0; i < len; i++) {
      const refS = ref.spectra[i].values;
      const testS = test.spectra[i].values;
      const sLen = Math.min(refS.length, testS.length);

      let dot = 0, normRef = 0, normTest = 0;
      for (let j = 0; j < sLen; j++) {
        dot += refS[j] * testS[j];
        normRef += refS[j] * refS[j];
        normTest += testS[j] * testS[j];
      }

      const denom = Math.sqrt(normRef) * Math.sqrt(normTest);
      totalCorr += denom > 0 ? dot / denom : 0;
    }

    const correlation = totalCorr / len;
    const flatnessDiff = Math.abs(ref.spectralFlatness - test.spectralFlatness);

    return { correlation, flatnessDiff };
  }

  // ── Run inversion iteration batch ──
  runBatch(batchSize, callback) {
    this.running = true;
    let batchCount = 0;

    const step = () => {
      if (!this.running || batchCount >= batchSize) {
        this.running = false;
        return;
      }

      const candidate = this.generateCandidate(this.iteration);
      const eval_ = this.evaluateCandidate(candidate);

      // Track best
      if (eval_.hammingDist < this.bestHamming) {
        this.bestHamming = eval_.hammingDist;
        this.bestCandidate = new Uint8Array(candidate);
      }

      this.candidates.push({
        iteration: this.iteration,
        hash: eval_.candidateHash,
        hamming: eval_.hammingDist,
        resonanceDiff: eval_.resonanceDiff.totalDiff,
        spectralCorr: eval_.spectralDiff.correlation
      });

      this.inversionLog.push({
        iter: this.iteration,
        hamming: eval_.hammingDist,
        bestHamming: this.bestHamming,
        resonanceDiff: eval_.resonanceDiff.totalDiff,
        spectralCorr: eval_.spectralDiff.correlation
      });

      this.iteration++;
      batchCount++;

      if (callback) {
        callback({
          iteration: this.iteration,
          hamming: eval_.hammingDist,
          bestHamming: this.bestHamming,
          totalCandidates: this.candidates.length,
          lastHash: eval_.candidateHash,
          resonanceDiff: eval_.resonanceDiff.totalDiff,
          spectralCorr: eval_.spectralDiff.correlation,
          found: eval_.hammingDist === 0
        });
      }

      if (eval_.hammingDist === 0) {
        this.running = false;
        return;
      }

      // Yield to UI every 10 iterations
      if (batchCount % 10 === 0) {
        requestAnimationFrame(step);
      } else {
        step();
      }
    };

    step();
  }

  stop() {
    this.running = false;
  }

  // ── Get convergence statistics ──
  getStats() {
    if (this.inversionLog.length === 0) {
      return { total: 0, bestHamming: 256, avgHamming: 256, hammingTrend: 0 };
    }

    const last100 = this.inversionLog.slice(-100);
    const avgHamming = last100.reduce((a, l) => a + l.hamming, 0) / last100.length;

    // Compute trend (slope of Hamming distance over last 100)
    let hammingTrend = 0;
    if (last100.length >= 10) {
      const n = last100.length;
      const xMean = (n - 1) / 2;
      const yMean = avgHamming;
      let num = 0, den = 0;
      for (let i = 0; i < n; i++) {
        num += (i - xMean) * (last100[i].hamming - yMean);
        den += (i - xMean) ** 2;
      }
      hammingTrend = den > 0 ? num / den : 0;
    }

    return {
      total: this.inversionLog.length,
      bestHamming: this.bestHamming,
      avgHamming,
      hammingTrend,
      bestCandidate: this.bestCandidate ? this.sha256.toHex(this.bestCandidate) : null,
      bestCandidateHash: this.bestCandidate ? this.sha256.hashWithStates(this.bestCandidate).hashHex : null
    };
  }
}

window.InversionEngine = InversionEngine;
