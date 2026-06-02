// ═══════════════════════════════════════════════════════════
// VORTEX PRIME — SHA-256 Engine with Round-by-Round Capture
// ═══════════════════════════════════════════════════════════

const SHA256_K = new Uint32Array([
  0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
  0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
  0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
  0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
  0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
  0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
  0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
  0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
]);

function rotr(x, n) { return ((x >>> n) | (x << (32 - n))) >>> 0; }
function ch(x, y, z) { return ((x & y) ^ (~x & z)) >>> 0; }
function maj(x, y, z) { return ((x & y) ^ (x & z) ^ (y & z)) >>> 0; }
function sigma0(x) { return (rotr(x,2) ^ rotr(x,13) ^ rotr(x,22)) >>> 0; }
function sigma1(x) { return (rotr(x,6) ^ rotr(x,11) ^ rotr(x,25)) >>> 0; }
function gamma0(x) { return (rotr(x,7) ^ rotr(x,18) ^ (x >>> 3)) >>> 0; }
function gamma1(x) { return (rotr(x,17) ^ rotr(x,19) ^ (x >>> 10)) >>> 0; }

const SHA256_IV = new Uint32Array([0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19]);

class SHA256Engine {
  constructor() {
    this.roundStates = [];
    this.messageSchedule = null;
  }

  // Pad message to multiple of 512 bits
  padMessage(msgBytes) {
    const bitLen = msgBytes.length * 8;
    const paddedLen = Math.ceil((msgBytes.length + 9) / 64) * 64;
    const padded = new Uint8Array(paddedLen);
    padded.set(msgBytes);
    padded[msgBytes.length] = 0x80;
    const lenOffset = paddedLen - 8;
    for (let i = 7; i >= 0; i--) {
      padded[lenOffset + 7 - i] = (bitLen >>> (i * 8)) & 0xff;
    }
    return padded;
  }

  // Compute SHA-256 with full round-by-round state capture
  hashWithStates(inputBytes) {
    this.roundStates = [];
    const padded = this.padMessage(inputBytes);
    const blocks = padded.length / 64;

    let H = new Uint32Array(SHA256_IV);

    for (let b = 0; b < blocks; b++) {
      const W = new Uint32Array(64);
      const block = padded.subarray(b * 64, (b + 1) * 64);

      // Prepare message schedule
      for (let t = 0; t < 16; t++) {
        W[t] = (block[t*4] << 24) | (block[t*4+1] << 16) | (block[t*4+2] << 8) | block[t*4+3];
      }
      for (let t = 16; t < 64; t++) {
        W[t] = (gamma1(W[t-2]) + W[t-7] + gamma0(W[t-15]) + W[t-16]) >>> 0;
      }
      this.messageSchedule = W;

      let a = H[0], b = H[1], c = H[2], d = H[3];
      let e = H[4], f = H[5], g = H[6], hh = H[7];

      // Round 0 = initial state
      this.roundStates.push(new Uint32Array([a, b, c, d, e, f, g, hh]));

      for (let t = 0; t < 64; t++) {
        const T1 = (hh + sigma1(e) + ch(e,f,g) + SHA256_K[t] + W[t]) >>> 0;
        const T2 = (sigma0(a) + maj(a,b,c)) >>> 0;
        hh = g; g = f; f = e;
        e = (d + T1) >>> 0;
        d = c; c = b; b = a;
        a = (T1 + T2) >>> 0;
        this.roundStates.push(new Uint32Array([a, b, c, d, e, f, g, hh]));
      }

      H[0] = (H[0] + a) >>> 0;
      H[1] = (H[1] + b) >>> 0;
      H[2] = (H[2] + c) >>> 0;
      H[3] = (H[3] + d) >>> 0;
      H[4] = (H[4] + e) >>> 0;
      H[5] = (H[5] + f) >>> 0;
      H[6] = (H[6] + g) >>> 0;
      H[7] = (H[7] + hh) >>> 0;
    }

    // Final state
    const hashBytes = new Uint8Array(32);
    for (let i = 0; i < 8; i++) {
      hashBytes[i*4]   = (H[i] >>> 24) & 0xff;
      hashBytes[i*4+1] = (H[i] >>> 16) & 0xff;
      hashBytes[i*4+2] = (H[i] >>> 8) & 0xff;
      hashBytes[i*4+3] = H[i] & 0xff;
    }

    return { hash: hashBytes, hashHex: this.toHex(hashBytes), H, roundStates: this.roundStates, messageSchedule: this.messageSchedule };
  }

  // SHA-256d (double hash) as used in Bitcoin
  hash256d(inputBytes) {
    const first = this.hashWithStates(inputBytes);
    const second = this.hashWithStates(first.hash);
    return { ...second, firstHash: first.hashHex, firstStates: first.roundStates, secondStates: second.roundStates };
  }

  // Compute bit differences between two states
  computeBitDiff(state1, state2) {
    let diffs = 0;
    for (let i = 0; i < 8; i++) {
      diffs += this.popcount(state1[i] ^ state2[i]);
    }
    return diffs;
  }

  // Bit difference profile across all rounds
  computeDiffusionProfile(referenceStates, testStates) {
    const profile = [];
    const maxRounds = Math.min(referenceStates.length, testStates.length);
    for (let r = 0; r < maxRounds; r++) {
      profile.push(this.computeBitDiff(referenceStates[r], testStates[r]));
    }
    return profile;
  }

  // Extract individual bits from state as array[8][32]
  stateToBitGrid(state) {
    const grid = [];
    for (let w = 0; w < 8; w++) {
      const row = [];
      for (let b = 31; b >= 0; b--) {
        row.push((state[w] >>> b) & 1);
      }
      grid.push(row);
    }
    return grid;
  }

  // Full bit grid for all rounds
  computeAllBitGrids() {
    return this.roundStates.map(s => this.stateToBitGrid(s));
  }

  // Popcount
  popcount(x) {
    x = x - ((x >>> 1) & 0x55555555);
    x = (x & 0x33333333) + ((x >>> 2) & 0x33333333);
    return (((x + (x >>> 4)) & 0x0F0F0F0F) * 0x01010101) >>> 24;
  }

  toHex(bytes) {
    return Array.from(bytes).map(b => b.toString(16).padStart(2,'0')).join('');
  }

  // Hex string to bytes
  hexToBytes(hex) {
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < hex.length; i += 2) {
      bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
    }
    return bytes;
  }
}

// Avalanche analysis: flip each input bit and measure diffusion
function computeAvalanche(engine, inputBytes) {
  const ref = engine.hashWithStates(inputBytes);
  const results = [];
  const totalInputBits = inputBytes.length * 8;

  for (let bit = 0; bit < Math.min(totalInputBits, 256); bit++) {
    const modified = new Uint8Array(inputBytes);
    const byteIdx = Math.floor(bit / 8);
    const bitIdx = 7 - (bit % 8);
    modified[byteIdx] ^= (1 << bitIdx);

    const test = engine.hashWithStates(modified);
    const diffProfile = engine.computeDiffusionProfile(ref.roundStates, test.roundStates);
    results.push({ bit, diffProfile, finalDiff: diffProfile[diffProfile.length - 1] });
  }

  // Average diffusion per round
  const avgDiffusion = new Array(65).fill(0);
  for (const r of results) {
    for (let i = 0; i < r.diffProfile.length; i++) {
      avgDiffusion[i] += r.diffProfile[i];
    }
  }
  for (let i = 0; i < avgDiffusion.length; i++) {
    avgDiffusion[i] /= results.length;
  }

  return { reference: ref, results, avgDiffusion };
}

// Export
window.SHA256Engine = SHA256Engine;
window.computeAvalanche = computeAvalanche;
