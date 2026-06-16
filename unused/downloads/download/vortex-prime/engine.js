// ═══════════════════════════════════════════════════════════
// VORTEX PRIME — SHA-256 Engine with Round-by-Round Capture
// Corrected implementation using DataView for big-endian reads
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

const SHA256_IV = [0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];

class SHA256Engine {
  constructor() {
    this.roundStates = [];
    this.messageSchedule = null;
  }

  // Compute SHA-256 with full round-by-round state capture
  hashWithStates(inputBytes) {
    this.roundStates = [];

    // Pad message
    const msgLen = inputBytes.length;
    const bitLen = msgLen * 8;
    let paddedLen = msgLen + 1;
    while (paddedLen % 64 !== 56) paddedLen++;
    paddedLen += 8;
    const padded = new Uint8Array(paddedLen);
    padded.set(inputBytes);
    padded[msgLen] = 0x80;
    const view = new DataView(padded.buffer);
    view.setUint32(paddedLen - 8, 0, false); // High 32 bits of length
    view.setUint32(paddedLen - 4, bitLen, false); // Low 32 bits of length

    let h0=SHA256_IV[0], h1=SHA256_IV[1], h2=SHA256_IV[2], h3=SHA256_IV[3];
    let h4=SHA256_IV[4], h5=SHA256_IV[5], h6=SHA256_IV[6], h7=SHA256_IV[7];

    for (let offset = 0; offset < paddedLen; offset += 64) {
      // Message schedule
      const w = new Array(64);
      for (let i = 0; i < 16; i++) {
        w[i] = view.getUint32(offset + i * 4, false); // big-endian
      }
      for (let i = 16; i < 64; i++) {
        const s0 = ((w[i-15] >>> 7) | (w[i-15] << 25)) ^ ((w[i-15] >>> 18) | (w[i-15] << 14)) ^ (w[i-15] >>> 3);
        const s1 = ((w[i-2] >>> 17) | (w[i-2] << 15)) ^ ((w[i-2] >>> 19) | (w[i-2] << 13)) ^ (w[i-2] >>> 10);
        w[i] = (w[i-16] + s0 + w[i-7] + s1) | 0;
      }
      this.messageSchedule = w;

      let a=h0, b=h1, c=h2, d=h3, e=h4, f=h5, g=h6, h=h7;
      // Round 0 = initial state
      this.roundStates.push(new Uint32Array([a>>>0, b>>>0, c>>>0, d>>>0, e>>>0, f>>>0, g>>>0, h>>>0]));

      for (let i = 0; i < 64; i++) {
        const S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
        const ch = (e & f) ^ (~e & g);
        const temp1 = (h + S1 + ch + SHA256_K[i] + w[i]) | 0;
        const S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
        const maj = (a & b) ^ (a & c) ^ (b & c);
        const temp2 = (S0 + maj) | 0;
        h=g; g=f; f=e; e=(d+temp1)|0; d=c; c=b; b=a; a=(temp1+temp2)|0;
        this.roundStates.push(new Uint32Array([a>>>0, b>>>0, c>>>0, d>>>0, e>>>0, f>>>0, g>>>0, h>>>0]));
      }

      h0=(h0+a)|0; h1=(h1+b)|0; h2=(h2+c)|0; h3=(h3+d)|0;
      h4=(h4+e)|0; h5=(h5+f)|0; h6=(h6+g)|0; h7=(h7+h)|0;
    }

    // Final hash
    const hashBytes = new Uint8Array(32);
    const hv = new DataView(hashBytes.buffer);
    hv.setUint32(0,h0,false); hv.setUint32(4,h1,false); hv.setUint32(8,h2,false); hv.setUint32(12,h3,false);
    hv.setUint32(16,h4,false); hv.setUint32(20,h5,false); hv.setUint32(24,h6,false); hv.setUint32(28,h7,false);

    return { hash: hashBytes, hashHex: this.toHex(hashBytes), H: [h0,h1,h2,h3,h4,h5,h6,h7], roundStates: this.roundStates, messageSchedule: this.messageSchedule };
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
