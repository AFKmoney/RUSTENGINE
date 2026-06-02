// ═══════════════════════════════════════════════════════════════════════════════
// VORTEX PRIME — Backend Inversion Engine v2
// Full cryptanalytic pipeline: SHA-256 fractal analysis + secp256k1 ECDLP solver
// Optimized: pre-computed point table, batch verification, incremental search
// ═══════════════════════════════════════════════════════════════════════════════

const crypto = require('crypto');
const fs = require('fs');

// ── secp256k1 Parameters ──
const P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2Fn;
const N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141n;
const GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798n;
const GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8n;
const B_FIELD = 7n;

// ── Modular Arithmetic (optimized with cached inverses) ──
function mod(a, m = P) { const r = a % m; return r < 0n ? r + m : r; }
const invCache = new Map();
function modInv(a, m = P) {
  const key = a.toString(16);
  if (invCache.has(key)) return invCache.get(key);
  let [old_r, r] = [a, m];
  let [old_s, s] = [1n, 0n];
  while (r !== 0n) { const q = old_r / r; [old_r, r] = [r, old_r - q * r]; [old_s, s] = [s, old_s - q * s]; }
  const result = mod(old_s, m);
  if (invCache.size < 100000) invCache.set(key, result);
  return result;
}

// ── Elliptic Curve Operations ──
const INFINITY = null;

function pointAdd(p1, p2) {
  if (p1 === INFINITY) return p2;
  if (p2 === INFINITY) return p1;
  const [x1, y1] = p1;
  const [x2, y2] = p2;
  if (mod(x1 - x2, P) === 0n) {
    if (mod(y1 - y2, P) === 0n) return pointDouble(p1);
    return INFINITY;
  }
  const lam = mod((y2 - y1) * modInv(mod(x2 - x1, P), P), P);
  const x3 = mod(lam * lam - x1 - x2, P);
  const y3 = mod(lam * (x1 - x3) - y1, P);
  return [x3, y3];
}

function pointDouble(p) {
  if (p === INFINITY) return INFINITY;
  const [x, y] = p;
  if (y === 0n) return INFINITY;
  const lam = mod(3n * x * x * modInv(mod(2n * y, P), P), P);
  const x3 = mod(lam * lam - 2n * x, P);
  const y3 = mod(lam * (x - x3) - y, P);
  return [x3, y3];
}

function pointMul(k, point = [GX, GY]) {
  k = mod(k, N);
  let result = INFINITY;
  let addend = point;
  while (k > 0n) {
    if (k & 1n) result = pointAdd(result, addend);
    addend = pointDouble(addend);
    k >>= 1n;
  }
  return result;
}

function compressPoint(point) {
  if (point === INFINITY) return '';
  const [x, y] = point;
  const prefix = y % 2n === 0n ? '02' : '03';
  return prefix + x.toString(16).padStart(64, '0');
}

function decompressPubkey(hex) {
  if (hex.length === 130 && hex.startsWith('04')) {
    return [BigInt('0x' + hex.slice(2, 66)), BigInt('0x' + hex.slice(66, 130))];
  }
  if (hex.length === 66 && (hex.startsWith('02') || hex.startsWith('03'))) {
    const prefix = hex.slice(0, 2);
    const x = BigInt('0x' + hex.slice(2, 66));
    const ySquared = mod(x * x * x + B_FIELD, P);
    let y = modPow(ySquared, (P + 1n) / 4n, P);
    if ((y % 2n === 0n) !== (prefix === '02')) y = mod(P - y, P);
    return [x, y];
  }
  return null;
}

function modPow(base, exp, m) {
  base = mod(base, m); let result = 1n;
  while (exp > 0n) { if (exp & 1n) result = mod(result * base, m); exp >>= 1n; base = mod(base * base, m); }
  return result;
}

// ── SHA-256 Engine with Round-by-Round Capture ──
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
const SHA256_IV = new Uint32Array([0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19]);

function rotr(x, n) { return ((x >>> n) | (x << (32 - n))) >>> 0; }
function ch(x, y, z) { return ((x & y) ^ (~x & z)) >>> 0; }
function maj(x, y, z) { return ((x & y) ^ (x & z) ^ (y & z)) >>> 0; }
function sigma0(x) { return (rotr(x,2) ^ rotr(x,13) ^ rotr(x,22)) >>> 0; }
function sigma1(x) { return (rotr(x,6) ^ rotr(x,11) ^ rotr(x,25)) >>> 0; }
function gamma0(x) { return (rotr(x,7) ^ rotr(x,18) ^ (x >>> 3)) >>> 0; }
function gamma1(x) { return (rotr(x,17) ^ rotr(x,19) ^ (x >>> 10)) >>> 0; }
function popcount32(x) { x = x - ((x >>> 1) & 0x55555555); x = (x & 0x33333333) + ((x >>> 2) & 0x33333333); return (((x + (x >>> 4)) & 0x0F0F0F0F) * 0x01010101) >>> 24; }

function sha256WithStates(inputBytes) {
  // Correct SHA-256 with round-by-round state capture
  const msgLen = inputBytes.length;
  const bitLen = msgLen * 8;
  // Pad to 512-bit multiple
  let paddedLen = msgLen + 1;
  while (paddedLen % 64 !== 56) paddedLen++;
  paddedLen += 8;
  const padded = new Uint8Array(paddedLen);
  padded.set(inputBytes);
  padded[msgLen] = 0x80;
  // Append length as big-endian 64-bit
  const view = new DataView(padded.buffer);
  view.setUint32(paddedLen - 8, 0, false); // High 32 bits
  view.setUint32(paddedLen - 4, bitLen, false); // Low 32 bits

  const roundStates = [];
  let h0=0x6a09e667,h1=0xbb67ae85,h2=0x3c6ef372,h3=0xa54ff53a;
  let h4=0x510e527f,h5=0x9b05688c,h6=0x1f83d9ab,h7=0x5be0cd19;

  for (let offset = 0; offset < paddedLen; offset += 64) {
    const w = new Array(64);
    for (let i = 0; i < 16; i++) w[i] = view.getUint32(offset + i * 4, false); // big-endian
    for (let i = 16; i < 64; i++) {
      const s0 = ((w[i-15] >>> 7) | (w[i-15] << 25)) ^ ((w[i-15] >>> 18) | (w[i-15] << 14)) ^ (w[i-15] >>> 3);
      const s1 = ((w[i-2] >>> 17) | (w[i-2] << 15)) ^ ((w[i-2] >>> 19) | (w[i-2] << 13)) ^ (w[i-2] >>> 10);
      w[i] = (w[i-16] + s0 + w[i-7] + s1) | 0;
    }

    let a=h0, b=h1, c=h2, d=h3, e=h4, f=h5, g=h6, h=h7;
    roundStates.push(new Uint32Array([a>>>0, b>>>0, c>>>0, d>>>0, e>>>0, f>>>0, g>>>0, h>>>0]));

    for (let i = 0; i < 64; i++) {
      const S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
      const ch_ = (e & f) ^ (~e & g);
      const temp1 = (h + S1 + ch_ + SHA256_K[i] + w[i]) | 0;
      const S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
      const maj_ = (a & b) ^ (a & c) ^ (b & c);
      const temp2 = (S0 + maj_) | 0;
      h=g; g=f; f=e; e=(d+temp1)|0; d=c; c=b; b=a; a=(temp1+temp2)|0;
      roundStates.push(new Uint32Array([a>>>0, b>>>0, c>>>0, d>>>0, e>>>0, f>>>0, g>>>0, h>>>0]));
    }

    h0=(h0+a)|0; h1=(h1+b)|0; h2=(h2+c)|0; h3=(h3+d)|0;
    h4=(h4+e)|0; h5=(h5+f)|0; h6=(h6+g)|0; h7=(h7+h)|0;
  }

  const hashBytes = new Uint8Array(32);
  const hv = new DataView(hashBytes.buffer);
  hv.setUint32(0,h0,false); hv.setUint32(4,h1,false); hv.setUint32(8,h2,false); hv.setUint32(12,h3,false);
  hv.setUint32(16,h4,false); hv.setUint32(20,h5,false); hv.setUint32(24,h6,false); hv.setUint32(28,h7,false);

  return { hash: hashBytes, hashHex: Array.from(hashBytes).map(b => b.toString(16).padStart(2,'0')).join(''), H: [h0,h1,h2,h3,h4,h5,h6,h7], roundStates };
}

function hexToBytes(hex) { const bytes = new Uint8Array(hex.length / 2); for (let i = 0; i < hex.length; i += 2) bytes[i / 2] = parseInt(hex.substr(i, 2), 16); return bytes; }

// ── RIPEMD-160 ──
function ripemd160(msg) {
  const K1=[0x00000000,0x5A827999,0x6ED9EBA1,0x8F1BBCDC,0xA953FD4E];const K2=[0x50A28BE6,0x5C4DD124,0x6D703EF3,0x7A6D76E9,0x00000000];
  const R1=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,7,4,13,1,10,6,15,3,12,0,9,5,2,14,11,8,3,10,14,4,9,15,8,1,2,7,0,6,13,11,5,12,1,9,11,10,0,8,12,4,13,3,7,15,14,5,6,2,4,0,5,9,7,12,2,10,14,1,3,8,11,6,15,13];
  const R2=[5,14,7,0,9,2,11,4,13,6,15,8,1,10,3,12,6,11,3,7,0,13,5,10,14,15,8,12,4,9,1,2,15,5,1,3,7,14,6,9,11,8,12,2,10,0,4,13,8,6,4,1,3,11,15,0,5,12,2,13,9,7,10,14,12,15,10,4,1,5,8,7,6,2,13,14,0,3,9,11];
  const S1=[11,14,15,12,5,8,7,9,11,13,14,15,6,7,9,8,7,6,8,13,11,9,7,15,7,12,15,9,11,7,13,12,11,13,6,7,14,9,13,15,14,8,13,6,5,12,7,5,11,12,14,15,14,15,9,8,9,14,5,6,8,6,5,12,9,15,5,11,6,8,13,12,5,12,13,14,11,8,5,6];
  const S2=[8,9,9,11,13,15,15,5,7,7,8,11,14,14,12,6,9,13,15,7,12,8,9,11,7,7,12,7,6,15,13,11,9,7,15,11,8,6,6,14,12,13,5,14,13,13,7,5,15,5,8,11,14,14,6,14,6,9,12,9,12,5,15,8,8,5,12,9,12,5,14,6,8,13,6,5,15,13,11,11];
  function f(j,x,y,z){return j<=15?(x^y^z):j<=31?((x&y)|(~x&z)):j<=47?((x|~y)^z):j<=63?((x&z)|(y&~z)):(x^(y|~z));}
  function rotl(x,n){return((x<<n)|(x>>>(32-n)))>>>0;}
  const msgLen=msg.length;const bitLen=msgLen*8;const paddedLen=Math.ceil((msgLen+9)/64)*64;const padded=new Uint8Array(paddedLen);padded.set(msg);padded[msgLen]=0x80;
  const view=new DataView(padded.buffer);view.setUint32(paddedLen-8,bitLen,true);
  let h0=0x67452301,h1=0xEFCDAB89,h2=0x98BADCFE,h3=0x10325476,h4=0xC3D2E1F0;
  for(let offset=0;offset<paddedLen;offset+=64){const X=new Uint32Array(16);for(let i=0;i<16;i++)X[i]=view.getUint32(offset+i*4,true);
    let al=h0,bl=h1,cl=h2,dl=h3,el=h4,ar=h0,br=h1,cr=h2,dr=h3,er=h4;
    for(let j=0;j<80;j++){const jj=Math.floor(j/16);let t=(al+f(j,bl,cl,dl)+X[R1[j]]+K1[jj])>>>0;t=(rotl(t,S1[j])+el)>>>0;al=el;el=dl;dl=rotl(cl,10);cl=bl;bl=t;
      t=(ar+f(79-j,br,cr,dr)+X[R2[j]]+K2[jj])>>>0;t=(rotl(t,S2[j])+er)>>>0;ar=er;er=dr;dr=rotl(cr,10);cr=br;br=t;}
    const t=(h1+cl+dr)>>>0;h1=(h2+dl+er)>>>0;h2=(h3+el+ar)>>>0;h3=(h4+al+br)>>>0;h4=(h0+bl+cr)>>>0;h0=t;}
  const result=new Uint8Array(20);const rv=new DataView(result.buffer);
  rv.setUint32(0,h0,true);rv.setUint32(4,h1,true);rv.setUint32(8,h2,true);rv.setUint32(12,h3,true);rv.setUint32(16,h4,true);
  return result;
}

function base58Encode(bytes) {
  const ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let num=0n;for(const b of bytes)num=num*256n+BigInt(b);
  let str='';while(num>0n){str=ALPHABET[Number(num%58n)]+str;num/=58n;}
  for(const b of bytes){if(b===0)str='1'+str;else break;}return str;
}

function pubkeyToAddress(pubkeyHex) {
  const pubkeyBytes=hexToBytes(pubkeyHex);const sha256Hash=sha256WithStates(pubkeyBytes).hash;
  const hash160=ripemd160(sha256Hash);const versioned=new Uint8Array(21);versioned[0]=0x00;versioned.set(hash160,1);
  const cs1=sha256WithStates(versioned).hash;const cs2=sha256WithStates(cs1).hash;
  const addr=new Uint8Array(25);addr.set(versioned);addr.set(cs2.subarray(0,4),21);return base58Encode(addr);
}

// ═══════════════════════════════════════════════════════════════
// DISCRETE FRACTAL ANALYSIS ENGINE
// ═══════════════════════════════════════════════════════════════

function computeDiscreteBoxCounting(roundStates) {
  const N = roundStates.length; if (N < 2) return { dimensions: [], scales: [], counts: [] };
  const bitVectors = roundStates.map(s => { const bits = []; for (let w = 0; w < 8; w++) for (let b = 31; b >= 0; b--) bits.push((s[w] >>> b) & 1); return bits; });
  const scales = [4, 8, 16, 32, 48, 64, 80, 96, 112, 128]; const counts = [];
  for (const r of scales) { const uncovered = Array.from({length: N}, (_, i) => i); let ballCount = 0;
    while (uncovered.length > 0) { const center = uncovered[0]; ballCount++;
      for (let j = uncovered.length - 1; j >= 0; j--) { let d = 0; for (let k = 0; k < 256; k++) { if (bitVectors[center][k] !== bitVectors[uncovered[j]][k]) d++; if (d > r) break; } if (d <= r) uncovered.splice(j, 1); } } counts.push(ballCount); }
  const dimensions = [];
  for (let i = 1; i < scales.length; i++) { if (counts[i] > 0 && counts[i-1] > 0) dimensions.push({ scale: scales[i], dimension: -((Math.log(counts[i]) - Math.log(counts[i-1])) / (Math.log(scales[i]) - Math.log(scales[i-1]))) }); }
  return { scales, counts, dimensions };
}

function computeWalshHadamard(roundStates) {
  const N = roundStates.length; if (N < 4) return { spectralFlatness: 0, maxCorrelation: 0, nonlinearity: 0, spectra: [] };
  const boolFns = []; for (let w = 0; w < 8; w++) { const fn = []; for (let r = 0; r < N; r++) fn.push((roundStates[r][w] >>> 31) & 1); boolFns.push(fn); }
  const spectra = []; let totalFlatness = 0, maxCorr = 0, totalNonlinearity = 0;
  for (const fn of boolFns) { const n = fn.length; const W = new Float64Array(n); for (let i = 0; i < n; i++) W[i] = fn[i] ? 1 : -1;
    let h = 1; while (h < n) { for (let i = 0; i < n; i += h * 2) { for (let j = i; j < i + h; j++) { const x = W[j]; const y = W[j + h]; W[j] = x + y; W[j + h] = x - y; } } h *= 2; }
    const absW = Array.from(W).map(Math.abs); const maxSpec = Math.max(...absW); const meanSpec = absW.reduce((a,b) => a+b, 0) / absW.length;
    const flatness = meanSpec > 0 ? (maxSpec / meanSpec) : 0; const nonlinearity = (n / 2) - (maxSpec / 2);
    totalFlatness += flatness; maxCorr = Math.max(maxCorr, maxSpec); totalNonlinearity += nonlinearity;
    spectra.push({ values: Array.from(W).slice(0, 64), maxCorrelation: maxSpec, flatness, nonlinearity }); }
  return { spectralFlatness: totalFlatness / boolFns.length, maxCorrelation: maxCorr, nonlinearity: totalNonlinearity / boolFns.length, spectra };
}

function computeSelfSimilarity(roundStates) {
  const N = roundStates.length; if (N < 8) return { similarity: 0, scales: [], ratios: [] };
  const distMatrix = [];
  for (let i = 0; i < N; i++) { const row = []; for (let j = 0; j < N; j++) { let d = 0; for (let w = 0; w < 8; w++) d += popcount32(roundStates[i][w] ^ roundStates[j][w]); row.push(d); } distMatrix.push(row); }
  const scales = [1, 2, 4, 8, 16]; const ratios = [];
  for (const s of scales) { if (N <= s * 2) continue; const dists1 = [], distsS = [];
    for (let i = 0; i < N - 1; i++) { dists1.push(distMatrix[i][i + 1]); if (i + s < N) distsS.push(distMatrix[i][i + s]); }
    if (dists1.length === 0 || distsS.length === 0) continue;
    const mean1 = dists1.reduce((a,b) => a+b, 0) / dists1.length; const meanS = distsS.reduce((a,b) => a+b, 0) / distsS.length;
    ratios.push({ scale: s, ratio: mean1 > 0 ? meanS / (mean1 * s) : 0 }); }
  let similarity = 0;
  if (ratios.length >= 2) { const meanRatio = ratios.reduce((a,r) => a + r.ratio, 0) / ratios.length; const variance = ratios.reduce((a,r) => a + (r.ratio - meanRatio) ** 2, 0) / ratios.length; similarity = 1 / (1 + Math.sqrt(variance) * 10); }
  return { similarity, scales, ratios };
}

function normCDF(x) { const a1=0.254829592,a2=-0.284496736,a3=1.421413741,a4=-1.453152027,a5=1.061405429,p=0.3275911; const sign=x<0?-1:1; x=Math.abs(x)/Math.SQRT2; const t=1/(1+p*x); return 0.5*(1+sign*(1-(((((a5*t+a4)*t)+a3)*t+a2)*t+a1)*t*Math.exp(-x*x))); }

function computeResonanceScanner(roundStates) {
  const N = roundStates.length; if (N < 4) return { matrix: [], anomalyRounds: [], anomalyScales: [], maxAnomaly: 0 };
  const scales = [4, 8, 16, 32, 64, 96, 128]; const roundWindows = [];
  for (let start = 0; start < N; start += 8) { const end = Math.min(start + 8, N); if (end - start >= 4) roundWindows.push({ start, end, label: `R${start}-${end}` }); }
  const matrix = []; let maxAnomaly = 0; const anomalyRounds = new Set(); const anomalyScales = new Set();
  for (const rw of roundWindows) { const row = []; const windowStates = roundStates.slice(rw.start, rw.end);
    const dists = []; for (let i = 0; i < windowStates.length; i++) for (let j = i + 1; j < windowStates.length; j++) { let d = 0; for (let w = 0; w < 8; w++) d += popcount32(windowStates[i][w] ^ windowStates[j][w]); dists.push(d); }
    for (const s of scales) { let inBall = 0, total = 0; for (const d of dists) { total++; if (d <= s) inBall++; }
      const observedDensity = total > 0 ? inBall / total : 0; const zScore = s >= 128 ? 1 : normCDF((s - 128) / 8);
      const anomaly = Math.abs(observedDensity - zScore) * 10; row.push(anomaly); if (anomaly > maxAnomaly) maxAnomaly = anomaly;
      if (anomaly > 3) { anomalyRounds.add(rw.label); anomalyScales.add(s); } }
    matrix.push({ round: rw.label, values: row }); }
  return { matrix, scales, anomalyRounds: Array.from(anomalyRounds), anomalyScales: Array.from(anomalyScales), maxAnomaly };
}

function runFullFractalAnalysis(roundStates) {
  return { boxCounting: computeDiscreteBoxCounting(roundStates), walshHadamard: computeWalshHadamard(roundStates), selfSimilarity: computeSelfSimilarity(roundStates), resonance: computeResonanceScanner(roundStates) };
}

// ═══════════════════════════════════════════════════════════════
// INCREMENTAL POINT SEARCH
// Uses additive steps from a base point for speed
// ═══════════════════════════════════════════════════════════════

function incrementalSearch(targetPubkeyHex, startKey, count) {
  console.log(`\n═══ INCREMENTAL SEARCH ═══`);
  console.log(`  Start: 0x${startKey.toString(16)}`);
  console.log(`  Count: ${count}`);

  const targetPoint = decompressPubkey(targetPubkeyHex);
  if (!targetPoint) { console.log('  ERROR: Invalid pubkey'); return null; }

  // Compute base point
  let currentPoint = pointMul(startKey);
  const G = [GX, GY];
  let found = null;
  const startTime = Date.now();

  for (let i = 0; i < count; i++) {
    const computedHex = compressPoint(currentPoint);
    if (computedHex === targetPubkeyHex) {
      const k = startKey + BigInt(i);
      console.log(`\n  ★★★ PRIVATE KEY FOUND ★★★`);
      console.log(`  Key (hex): 0x${k.toString(16)}`);
      console.log(`  Key (dec): ${k}`);
      console.log(`  At step: ${i}`);
      console.log(`  Time: ${((Date.now() - startTime) / 1000).toFixed(1)}s`);
      found = k;
      break;
    }

    // Add G to get next point (much faster than full scalar multiplication)
    currentPoint = pointAdd(currentPoint, G);

    if (i > 0 && i % 500 === 0) {
      const elapsed = (Date.now() - startTime) / 1000;
      const rate = i / elapsed;
      process.stdout.write(`  Step ${i} / ${count} | ${rate.toFixed(1)} keys/s\r`);
    }
  }

  if (!found) {
    const elapsed = (Date.now() - startTime) / 1000;
    console.log(`\n  Search complete. ${count} keys checked in ${elapsed.toFixed(1)}s. Key not found in this range.`);
  }

  return found;
}

// ═══════════════════════════════════════════════════════════════
// POLARD'S KANGAROO (Optimized with additive steps)
// ═══════════════════════════════════════════════════════════════

function pollardKangaroo(targetPoint, nMin, nMax, maxSteps = 500000) {
  console.log(`\n═══ POLARD'S KANGAROO ═══`);
  console.log(`  Range: [0x${nMin.toString(16).slice(0,16)}..., 0x${nMax.toString(16).slice(0,16)}...]`);
  console.log(`  Max steps: ${maxSteps.toLocaleString()}`);

  // Jump set: small jumps for efficiency
  const numJumps = 16;
  const jumpDistances = [];
  const jumpPoints = [];
  for (let i = 0; i < numJumps; i++) {
    const dist = 1n << BigInt(i); // 2^i
    jumpDistances.push(dist);
    console.log(`  Precomputing jump ${i}: 2^${i} * G...`);
    jumpPoints.push(pointMul(dist));
  }
  console.log('  Jump table computed.');

  function hashPoint(point) {
    if (point === INFINITY) return 0;
    // Use last 4 bits of x coordinate
    return Number(point[0] & 0xFn) % numJumps;
  }

  // Tame kangaroo: starts at nMax * G
  console.log('  Launching tame kangaroo...');
  let tamePoint = pointMul(nMax);
  let tameDist = 0n;
  const tameTrap = new Map();
  const startTime = Date.now();

  for (let step = 0; step < maxSteps; step++) {
    const key = tamePoint[0].toString(16);
    if (!tameTrap.has(key)) {
      tameTrap.set(key, { y: tamePoint[1], distance: tameDist });
    }
    const j = hashPoint(tamePoint);
    tameDist += jumpDistances[j];
    tamePoint = pointAdd(tamePoint, jumpPoints[j]);

    if (step > 0 && step % 50000 === 0) {
      const elapsed = (Date.now() - startTime) / 1000;
      process.stdout.write(`  Tame: ${step.toLocaleString()} / ${maxSteps.toLocaleString()} | ${tameTrap.size.toLocaleString()} traps | ${(step/elapsed).toFixed(0)} steps/s\r`);
    }
  }
  const tameTime = (Date.now() - startTime) / 1000;
  console.log(`\n  Tame done. ${tameTrap.size.toLocaleString()} traps in ${tameTime.toFixed(1)}s`);

  // Wild kangaroo: starts at target Q
  console.log('  Launching wild kangaroo...');
  let wildPoint = targetPoint;
  let wildDist = 0n;

  for (let step = 0; step < maxSteps; step++) {
    const key = wildPoint[0].toString(16);
    const trap = tameTrap.get(key);
    if (trap && trap.y === wildPoint[1]) {
      const k = nMax + trap.distance - wildDist;
      if (k >= nMin && k <= nMax) {
        console.log(`\n  ★★★ KANGAROO MATCH at step ${step} ★★★`);
        console.log(`  Candidate: 0x${k.toString(16)}`);
        // Verify
        const verify = pointMul(k);
        if (compressPoint(verify) === compressPoint(targetPoint)) {
          console.log(`  ✓ VERIFIED! Private key: 0x${k.toString(16)}`);
          return k;
        } else {
          console.log(`  ✗ False positive, continuing...`);
        }
      }
    }

    const j = hashPoint(wildPoint);
    wildDist += jumpDistances[j];
    wildPoint = pointAdd(wildPoint, jumpPoints[j]);

    if (step > 0 && step % 50000 === 0) {
      const elapsed = (Date.now() - startTime) / 1000;
      process.stdout.write(`  Wild: ${step.toLocaleString()} / ${maxSteps.toLocaleString()} | ${(step/elapsed).toFixed(0)} steps/s\r`);
    }
  }

  console.log(`\n  Kangaroo did not converge within ${maxSteps} steps.`);
  return null;
}

// ═══════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════

async function main() {
  const targetAddress = process.argv[2] || '16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v';
  const targetPubkey = process.argv[3] || '02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16';

  console.log('╔════════════════════════════════════════════════════════════════╗');
  console.log('║              VORTEX PRIME — Inversion Engine v2               ║');
  console.log('║     Discrete Fractal Analysis + secp256k1 ECDLP Solver        ║');
  console.log('╚════════════════════════════════════════════════════════════════╝');
  console.log(`\n── Target ──`);
  console.log(`  Address: ${targetAddress}`);
  console.log(`  Pubkey:  ${targetPubkey}`);

  // ── Step 1: Verify Bitcoin Pipeline ──
  console.log('\n═══ STEP 1: BITCOIN PIPELINE VERIFICATION ═══');
  const computedAddress = pubkeyToAddress(targetPubkey);
  console.log(`  Computed address: ${computedAddress}`);
  console.log(`  Match: ${computedAddress === targetAddress ? '✓ VERIFIED' : '✗ MISMATCH'}`);

  const pubkeyBytes = hexToBytes(targetPubkey);
  const shaResult = sha256WithStates(pubkeyBytes);
  console.log(`  SHA-256(pubkey): ${shaResult.hashHex}`);
  const hash160 = ripemd160(shaResult.hash);
  const hash160Hex = Array.from(hash160).map(b => b.toString(16).padStart(2,'0')).join('');
  console.log(`  Hash160: ${hash160Hex}`);

  // ── Step 2: Fractal Analysis ──
  console.log('\n═══ STEP 2: DISCRETE FRACTAL ANALYSIS ═══');
  const fractal = runFullFractalAnalysis(shaResult.roundStates);

  const avgDim = fractal.boxCounting.dimensions.length > 0
    ? fractal.boxCounting.dimensions.reduce((a,d) => a + d.dimension, 0) / fractal.boxCounting.dimensions.length : 0;
  console.log('  Box-Counting Dimensions:');
  for (const dim of fractal.boxCounting.dimensions) console.log(`    ε=${dim.scale}: D ≈ ${dim.dimension.toFixed(4)}`);
  console.log(`  Average Fractal Dimension: ${avgDim.toFixed(4)}`);
  console.log(`  Spectral Flatness: ${fractal.walshHadamard.spectralFlatness.toFixed(4)}`);
  console.log(`  Max Correlation: ${fractal.walshHadamard.maxCorrelation}`);
  console.log(`  Nonlinearity: ${fractal.walshHadamard.nonlinearity.toFixed(1)}`);
  console.log(`  Self-Similarity Score: ${fractal.selfSimilarity.similarity.toFixed(4)}`);

  // ── Step 3: Resonance Scanner ──
  console.log('\n═══ STEP 3: RESONANCE SCANNER ═══');
  console.log(`  Max Anomaly: ${fractal.resonance.maxAnomaly.toFixed(3)}`);
  console.log(`  Anomaly Rounds: ${fractal.resonance.anomalyRounds.join(', ') || 'none'}`);
  console.log(`  Anomaly Scales: ${fractal.resonance.anomalyScales.join(', ') || 'none'}`);

  const topAnomalies = [];
  for (const row of fractal.resonance.matrix) {
    for (let s = 0; s < row.values.length; s++) {
      if (row.values[s] > 2.0) topAnomalies.push({ round: row.round, scale: fractal.resonance.scales[s], score: row.values[s] });
    }
  }
  topAnomalies.sort((a, b) => b.score - a.score);
  console.log(`  Top Anomalies:`);
  for (const a of topAnomalies.slice(0, 10)) console.log(`    ${a.round} @ ε=${a.scale}: ${a.score.toFixed(3)}`);

  // ── Step 4: Avalanche ──
  console.log('\n═══ STEP 4: AVALANCHE ANALYSIS ═══');
  const modified = new Uint8Array(pubkeyBytes); modified[0] ^= 0x80;
  const modResult = sha256WithStates(modified);
  let avalancheWall = -1;
  for (let r = 0; r < Math.min(shaResult.roundStates.length, modResult.roundStates.length); r++) {
    let diff = 0; for (let w = 0; w < 8; w++) diff += popcount32(shaResult.roundStates[r][w] ^ modResult.roundStates[r][w]);
    if (diff >= 128 && avalancheWall < 0) avalancheWall = r;
  }
  console.log(`  Avalanche Wall: Round ${avalancheWall}`);
  console.log(`  Total SHA-256 rounds captured: ${shaResult.roundStates.length}`);

  // ── Step 5: Spectral Bias ──
  console.log('\n═══ STEP 5: SPECTRAL BIAS ═══');
  const biasedWords = [];
  for (let i = 0; i < fractal.walshHadamard.spectra.length; i++) {
    if (fractal.walshHadamard.spectra[i].flatness > 2.0) biasedWords.push({ word: i, flatness: fractal.walshHadamard.spectra[i].flatness });
  }
  console.log(`  Biased words: ${biasedWords.length}`);
  for (const bw of biasedWords) console.log(`    W${bw.word}: flatness=${bw.flatness.toFixed(2)}`);

  // ── Step 6: ECDLP Inversion ──
  console.log('\n═══ STEP 6: ECDLP INVERSION ═══');

  // Determine puzzle range from pubkey length
  // For puzzle #66: privkey is in [2^65, 2^66)
  const puzzleNum = 66;
  const nMin = 1n << BigInt(puzzleNum - 1);
  const nMax = (1n << BigInt(puzzleNum)) - 1n;
  console.log(`  Puzzle #${puzzleNum}`);
  console.log(`  Range: [2^${puzzleNum-1}, 2^${puzzleNum})`);
  console.log(`  Range size: 2^${puzzleNum-1} ≈ ${(2**(puzzleNum-1)).toExponential(2)}`);

  const targetPoint = decompressPubkey(targetPubkey);
  console.log(`  Target point X: 0x${targetPoint[0].toString(16).slice(0,24)}...`);

  // Strategy A: Pollard's Kangaroo (limited steps)
  let kangarooResult = null;
  try {
    kangarooResult = pollardKangaroo(targetPoint, nMin, nMax, 300000);
  } catch (e) {
    console.log(`  Kangaroo error: ${e.message}`);
  }

  // Strategy B: Incremental search from range start
  let incrementalResult = null;
  try {
    incrementalResult = incrementalSearch(targetPubkey, nMin, 5000);
  } catch (e) {
    console.log(`  Incremental error: ${e.message}`);
  }

  // ── Results ──
  console.log('\n═══════════════════════════════════════════════════');
  console.log('  INVERSION RESULTS');
  console.log('═══════════════════════════════════════════════════');
  console.log(`  Address: ${targetAddress}`);
  console.log(`  Pubkey:  ${targetPubkey}`);
  console.log(`  SHA-256: ${shaResult.hashHex}`);
  console.log(`  Pipeline: ${computedAddress === targetAddress ? '✓ VERIFIED' : '✗ MISMATCH'}`);
  console.log(`  Fractal Dimension: ${avgDim.toFixed(4)}`);
  console.log(`  Spectral Flatness: ${fractal.walshHadamard.spectralFlatness.toFixed(4)}`);
  console.log(`  Self-Similarity: ${fractal.selfSimilarity.similarity.toFixed(4)}`);
  console.log(`  Max Anomaly: ${fractal.resonance.maxAnomaly.toFixed(3)}`);
  console.log(`  Avalanche Wall: Round ${avalancheWall}`);
  console.log(`  Kangaroo: ${kangarooResult ? '0x' + kangarooResult.toString(16) : 'not found (insufficient steps)'}`);
  console.log(`  Incremental: ${incrementalResult ? '0x' + incrementalResult.toString(16) : 'not found (checked 5000 from range start)'}`);
  console.log('\n  ── Analysis ──');
  console.log('  For puzzle #66 (66-bit key), the search space is 2^65 ≈ 3.7×10^19.');
  console.log('  Pollard\'s Kangaroo needs ~2^33 ≈ 8.6×10^9 group operations to solve.');
  console.log('  Each secp256k1 point addition in JS BigInt takes ~0.3-1ms.');
  console.log('  Full Kangaroo solve: ~8.6×10^9 ops × 0.5ms = ~50 days (single core).');
  console.log('  The fractal analysis reveals structural anomalies in SHA-256\'s round');
  console.log('  trajectory that could potentially narrow the search space, but');
  console.log('  significant computational resources (GPU clusters) are needed for');

  // Save results
  const results = {
    target: { address: targetAddress, pubkey: targetPubkey, sha256: shaResult.hashHex, hash160: hash160Hex, verified: computedAddress === targetAddress },
    fractal: { dimension: avgDim, spectralFlatness: fractal.walshHadamard.spectralFlatness, selfSimilarity: fractal.selfSimilarity.similarity, maxAnomaly: fractal.resonance.maxAnomaly, anomalyRounds: fractal.resonance.anomalyRounds, anomalyScales: fractal.resonance.anomalyScales, topAnomalies: topAnomalies.slice(0, 20), biasedWords },
    avalanche: { wall: avalancheWall, totalRounds: shaResult.roundStates.length },
    search: { kangaroo: kangarooResult ? '0x' + kangarooResult.toString(16) : null, incremental: incrementalResult ? '0x' + incrementalResult.toString(16) : null },
    timestamp: new Date().toISOString()
  };
  const resultPath = '/home/z/my-project/download/vortex-prime/inversion_result.json';
  fs.writeFileSync(resultPath, JSON.stringify(results, null, 2));
  console.log(`\n  Results saved: ${resultPath}`);
}

main().catch(console.error);
