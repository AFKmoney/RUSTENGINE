// ═══════════════════════════════════════════════════════════════════════════════
// VORTEX PRIME — Backend Server v3
// Express + WebSocket — Full cryptanalytic pipeline
// Range: Puzzle #135 (2^134 to 2^135-1) — configurable
// ═══════════════════════════════════════════════════════════════════════════════

const express = require('express');
const http = require('http');
const { WebSocketServer } = require('ws');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');

const app = express();
const server = http.createServer(app);
const wss = new WebSocketServer({ server });

app.use(express.json());
app.use(express.static(path.join(__dirname)));

// ═══════════════════════════════════════════════════════════════════════════════
// secp256k1 PARAMETERS
// ═══════════════════════════════════════════════════════════════════════════════
const P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2Fn;
const N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141n;
const GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798n;
const GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8n;
const B_FIELD = 7n;

// ── Modular Arithmetic ──
function mod(a, m = P) { const r = a % m; return r < 0n ? r + m : r; }

const invCache = new Map();
function modInv(a, m = P) {
  const key = a.toString(16);
  if (invCache.has(key)) return invCache.get(key);
  let [old_r, r] = [a, m];
  let [old_s, s] = [1n, 0n];
  while (r !== 0n) { const q = old_r / r; [old_r, r] = [r, old_r - q * r]; [old_s, s] = [s, old_s - q * s]; }
  const result = mod(old_s, m);
  if (invCache.size < 50000) invCache.set(key, result);
  return result;
}

function modPow(base, exp, m) {
  base = mod(base, m); let result = 1n;
  while (exp > 0n) { if (exp & 1n) result = mod(result * base, m); exp >>= 1n; base = mod(base * base, m); }
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

// ═══════════════════════════════════════════════════════════════════════════════
// SHA-256 ENGINE WITH ROUND-BY-ROUND CAPTURE
// ═══════════════════════════════════════════════════════════════════════════════
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
function popcount32(x) { x = x - ((x >>> 1) & 0x55555555); x = (x & 0x33333333) + ((x >>> 2) & 0x33333333); return (((x + (x >>> 4)) & 0x0F0F0F0F) * 0x01010101) >>> 24; }

function sha256WithStates(inputBytes) {
  const msgLen = inputBytes.length;
  const bitLen = msgLen * 8;
  let paddedLen = msgLen + 1;
  while (paddedLen % 64 !== 56) paddedLen++;
  paddedLen += 8;
  const padded = new Uint8Array(paddedLen);
  padded.set(inputBytes);
  padded[msgLen] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(paddedLen - 8, 0, false);
  view.setUint32(paddedLen - 4, bitLen, false);

  const roundStates = [];
  let h0=0x6a09e667,h1=0xbb67ae85,h2=0x3c6ef372,h3=0xa54ff53a;
  let h4=0x510e527f,h5=0x9b05688c,h6=0x1f83d9ab,h7=0x5be0cd19;

  for (let offset = 0; offset < paddedLen; offset += 64) {
    const w = new Array(64);
    for (let i = 0; i < 16; i++) w[i] = view.getUint32(offset + i * 4, false);
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

// ═══════════════════════════════════════════════════════════════════════════════
// DISCRETE FRACTAL ANALYSIS ENGINE
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// INVERSION STATE
// ═══════════════════════════════════════════════════════════════════════════════

let inversionState = {
  running: false,
  puzzleNum: 135,
  nMin: 1n << 134n,
  nMax: (1n << 135n) - 1n,
  targetPubkey: null,
  targetPoint: null,
  targetAddress: null,
  sha256Hash: null,
  hash160: null,
  fractalResult: null,
  anomalyMap: null,
  stats: {
    iterations: 0,
    kangarooSteps: 0,
    incrementalSteps: 0,
    bestHamming: 256,
    startTime: null,
    keysPerSec: 0,
    found: false,
    privateKey: null
  }
};

// ═══════════════════════════════════════════════════════════════════════════════
// API ROUTES
// ═══════════════════════════════════════════════════════════════════════════════

// ── Analyze target ──
app.post('/api/analyze', (req, res) => {
  try {
    const { pubkey, hash, address, puzzleNum } = req.body;

    if (!pubkey && !hash && !address) {
      return res.status(400).json({ error: 'Provide at least pubkey, hash, or address' });
    }

    // Update puzzle range
    if (puzzleNum && puzzleNum >= 1 && puzzleNum <= 256) {
      inversionState.puzzleNum = puzzleNum;
      inversionState.nMin = 1n << BigInt(puzzleNum - 1);
      inversionState.nMax = (1n << BigInt(puzzleNum)) - 1n;
    }

    let targetPubkey = pubkey || null;
    let targetHash = hash || null;
    let computedAddress = '';
    let sha256Result = null;
    let hash160Hex = '';

    // Pipeline Bitcoin
    if (targetPubkey) {
      const pubkeyBytes = hexToBytes(targetPubkey);
      sha256Result = sha256WithStates(pubkeyBytes);

      if (!targetHash) targetHash = sha256Result.hashHex;

      const hash160 = ripemd160(sha256Result.hash);
      hash160Hex = Array.from(hash160).map(b => b.toString(16).padStart(2,'0')).join('');

      const versioned = new Uint8Array(21);
      versioned[0] = 0x00;
      versioned.set(hash160, 1);
      const cs1 = sha256WithStates(versioned).hash;
      const cs2 = sha256WithStates(cs1).hash;
      const addr = new Uint8Array(25);
      addr.set(versioned);
      addr.set(cs2.subarray(0, 4), 21);
      computedAddress = base58Encode(addr);

      inversionState.targetPubkey = targetPubkey;
      inversionState.targetAddress = computedAddress;
      inversionState.sha256Hash = sha256Result.hashHex;
      inversionState.hash160 = hash160Hex;
      inversionState.targetPoint = decompressPubkey(targetPubkey);
    } else if (targetHash) {
      const hashBytes = hexToBytes(targetHash);
      sha256Result = sha256WithStates(hashBytes);
      inversionState.sha256Hash = targetHash;
      inversionState.targetAddress = address || null;
    }

    // Fractal analysis
    const fractal = runFullFractalAnalysis(sha256Result.roundStates);
    inversionState.fractalResult = fractal;

    // Anomaly map
    const topAnomalies = [];
    const weakRounds = new Set();
    const weakScales = new Set();
    for (const row of fractal.resonance.matrix) {
      for (let s = 0; s < row.values.length; s++) {
        if (row.values[s] > 2.0) {
          topAnomalies.push({ round: row.round, scale: fractal.resonance.scales[s], score: row.values[s] });
          weakRounds.add(row.round);
          weakScales.add(fractal.resonance.scales[s]);
        }
      }
    }
    topAnomalies.sort((a, b) => b.score - a.score);

    inversionState.anomalyMap = {
      weakRounds: Array.from(weakRounds),
      weakScales: Array.from(weakScales),
      topAnomalies: topAnomalies.slice(0, 20)
    };

    // Spectral bias
    const biasedWords = [];
    for (let i = 0; i < fractal.walshHadamard.spectra.length; i++) {
      if (fractal.walshHadamard.spectra[i].flatness > 2.0) {
        biasedWords.push({ word: i, flatness: fractal.walshHadamard.spectra[i].flatness });
      }
    }

    // Avalanche analysis
    let avalancheWall = -1;
    if (targetPubkey) {
      const pubkeyBytes = hexToBytes(targetPubkey);
      const modified = new Uint8Array(pubkeyBytes); modified[0] ^= 0x80;
      const modResult = sha256WithStates(modified);
      for (let r = 0; r < Math.min(sha256Result.roundStates.length, modResult.roundStates.length); r++) {
        let diff = 0; for (let w = 0; w < 8; w++) diff += popcount32(sha256Result.roundStates[r][w] ^ modResult.roundStates[r][w]);
        if (diff >= 128 && avalancheWall < 0) avalancheWall = r;
      }
    }

    const avgDim = fractal.boxCounting.dimensions.length > 0
      ? fractal.boxCounting.dimensions.reduce((a,d) => a + d.dimension, 0) / fractal.boxCounting.dimensions.length : 0;

    res.json({
      success: true,
      target: {
        pubkey: targetPubkey,
        address: computedAddress || address,
        sha256: inversionState.sha256Hash,
        hash160: hash160Hex,
        verified: targetPubkey && computedAddress === address
      },
      pipeline: {
        pubkey: targetPubkey,
        sha256: sha256Result ? sha256Result.hashHex : targetHash,
        hash160: hash160Hex,
        address: computedAddress,
        verified: targetPubkey && address ? computedAddress === address : null
      },
      fractal: {
        dimension: avgDim,
        spectralFlatness: fractal.walshHadamard.spectralFlatness,
        selfSimilarity: fractal.selfSimilarity.similarity,
        maxAnomaly: fractal.resonance.maxAnomaly,
        anomalyRounds: inversionState.anomalyMap.weakRounds,
        anomalyScales: inversionState.anomalyMap.weakScales,
        topAnomalies: inversionState.anomalyMap.topAnomalies.slice(0, 10),
        biasedWords,
        boxCounting: {
          scales: fractal.boxCounting.scales,
          counts: fractal.boxCounting.counts,
          dimensions: fractal.boxCounting.dimensions
        },
        walshHadamard: {
          spectralFlatness: fractal.walshHadamard.spectralFlatness,
          maxCorrelation: fractal.walshHadamard.maxCorrelation,
          nonlinearity: fractal.walshHadamard.nonlinearity,
          spectra: fractal.walshHadamard.spectra.map(s => ({
            values: s.values,
            maxCorrelation: s.maxCorrelation,
            flatness: s.flatness,
            nonlinearity: s.nonlinearity
          }))
        },
        selfSimilarity: {
          similarity: fractal.selfSimilarity.similarity,
          scales: fractal.selfSimilarity.scales,
          ratios: fractal.selfSimilarity.ratios
        },
        resonance: {
          maxAnomaly: fractal.resonance.maxAnomaly,
          matrix: fractal.resonance.matrix,
          scales: fractal.resonance.scales,
          anomalyRounds: fractal.resonance.anomalyRounds,
          anomalyScales: fractal.resonance.anomalyScales
        }
      },
      avalanche: { wall: avalancheWall },
      range: {
        puzzleNum: inversionState.puzzleNum,
        nMin: '0x' + inversionState.nMin.toString(16),
        nMax: '0x' + inversionState.nMax.toString(16),
        rangeSize: '2^' + (inversionState.puzzleNum - 1),
        rangeSizeDecimal: (2n ** BigInt(inversionState.puzzleNum - 1)).toString()
      }
    });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

// ── Start inversion ──
app.post('/api/inversion/start', (req, res) => {
  if (inversionState.running) {
    return res.status(400).json({ error: 'Inversion already running' });
  }
  if (!inversionState.targetPubkey && !inversionState.sha256Hash) {
    return res.status(400).json({ error: 'Analyze a target first' });
  }

  const { puzzleNum, strategy } = req.body;
  if (puzzleNum && puzzleNum >= 1 && puzzleNum <= 256) {
    inversionState.puzzleNum = puzzleNum;
    inversionState.nMin = 1n << BigInt(puzzleNum - 1);
    inversionState.nMax = (1n << BigInt(puzzleNum)) - 1n;
  }

  inversionState.running = true;
  inversionState.stats = {
    iterations: 0,
    kangarooSteps: 0,
    incrementalSteps: 0,
    bestHamming: 256,
    startTime: Date.now(),
    keysPerSec: 0,
    found: false,
    privateKey: null,
    strategy: strategy || 'kangaroo'
  };

  res.json({ success: true, message: 'Inversion started', puzzleNum: inversionState.puzzleNum });

  // Run inversion in background
  setImmediate(() => runInversion());
});

// ── Get status ──
app.get('/api/inversion/status', (req, res) => {
  const stats = { ...inversionState.stats };
  if (stats.startTime) {
    const elapsed = (Date.now() - stats.startTime) / 1000;
    stats.elapsed = elapsed;
    stats.keysPerSec = stats.iterations > 0 ? (stats.iterations / elapsed).toFixed(1) : 0;
  }
  stats.running = inversionState.running;
  stats.puzzleNum = inversionState.puzzleNum;
  stats.rangeMin = '0x' + inversionState.nMin.toString(16);
  stats.rangeMax = '0x' + inversionState.nMax.toString(16);
  res.json(stats);
});

// ── Stop inversion ──
app.post('/api/inversion/stop', (req, res) => {
  inversionState.running = false;
  res.json({ success: true, message: 'Inversion stopped' });
});

// ── Update range ──
app.post('/api/range', (req, res) => {
  const { puzzleNum } = req.body;
  if (puzzleNum && puzzleNum >= 1 && puzzleNum <= 256) {
    inversionState.puzzleNum = puzzleNum;
    inversionState.nMin = 1n << BigInt(puzzleNum - 1);
    inversionState.nMax = (1n << BigInt(puzzleNum)) - 1n;
    res.json({
      success: true,
      puzzleNum,
      nMin: '0x' + inversionState.nMin.toString(16),
      nMax: '0x' + inversionState.nMax.toString(16),
      rangeSize: '2^' + (puzzleNum - 1)
    });
  } else {
    res.status(400).json({ error: 'Invalid puzzle number (1-256)' });
  }
});

// ═══════════════════════════════════════════════════════════════════════════════
// INVERSION ENGINE — All Strategies
// ═══════════════════════════════════════════════════════════════════════════════

function runInversion() {
  const strategy = inversionState.stats.strategy;
  console.log(`\n═══ VORTEX PRIME — Inversion Started ═══`);
  console.log(`  Strategy: ${strategy}`);
  console.log(`  Puzzle: #${inversionState.puzzleNum}`);
  console.log(`  Range: [2^${inversionState.puzzleNum-1}, 2^${inversionState.puzzleNum})`);

  if (!inversionState.targetPoint && inversionState.targetPubkey) {
    inversionState.targetPoint = decompressPubkey(inversionState.targetPubkey);
  }

  if (strategy === 'kangaroo' || strategy === 'all') {
    runPollardKangaroo();
  }
  if (strategy === 'incremental' || strategy === 'all') {
    if (inversionState.running) runIncrementalSearch();
  }
  if (strategy === 'fractal-guided' || strategy === 'all') {
    if (inversionState.running) runFractalGuidedSearch();
  }
}

// ── Pollard's Kangaroo ──
function runPollardKangaroo() {
  if (!inversionState.targetPoint) {
    console.log('  Kangaroo: No target point, skipping');
    return;
  }

  console.log(`\n═══ POLLARD'S KANGAROO ═══`);
  const nMin = inversionState.nMin;
  const nMax = inversionState.nMax;
  const target = inversionState.targetPoint;
  const maxSteps = 2000000;

  // Jump table
  const numJumps = 32;
  const jumpDistances = [];
  const jumpPoints = [];
  for (let i = 0; i < numJumps; i++) {
    const dist = 1n << BigInt(i);
    jumpDistances.push(dist);
    jumpPoints.push(pointMul(dist));
  }
  console.log('  Jump table computed (32 entries)');

  function hashPoint(pt) {
    if (pt === INFINITY) return 0;
    return Number(pt[0] & 0x1Fn) % numJumps;
  }

  // Tame kangaroo
  console.log('  Launching tame kangaroo...');
  let tamePoint = pointMul(nMax);
  let tameDist = 0n;
  const tameTrap = new Map();
  const startTime = Date.now();

  for (let step = 0; step < maxSteps && inversionState.running; step++) {
    const key = tamePoint[0].toString(16);
    if (!tameTrap.has(key)) {
      tameTrap.set(key, { y: tamePoint[1], distance: tameDist });
    }
    const j = hashPoint(tamePoint);
    tameDist += jumpDistances[j];
    tamePoint = pointAdd(tamePoint, jumpPoints[j]);

    inversionState.stats.kangarooSteps++;
    inversionState.stats.iterations++;

    if (step > 0 && step % 100000 === 0) {
      const elapsed = (Date.now() - startTime) / 1000;
      const rate = (step / elapsed).toFixed(0);
      broadcastUpdate({
        type: 'kangaroo',
        phase: 'tame',
        step,
        maxSteps,
        traps: tameTrap.size,
        rate,
        elapsed: elapsed.toFixed(1)
      });
      console.log(`  Tame: ${step.toLocaleString()} / ${maxSteps.toLocaleString()} | ${tameTrap.size.toLocaleString()} traps | ${rate} steps/s`);
    }
  }

  const tameTime = (Date.now() - startTime) / 1000;
  console.log(`  Tame done. ${tameTrap.size.toLocaleString()} traps in ${tameTime.toFixed(1)}s`);

  // Wild kangaroo
  console.log('  Launching wild kangaroo...');
  let wildPoint = target;
  let wildDist = 0n;

  for (let step = 0; step < maxSteps && inversionState.running; step++) {
    const key = wildPoint[0].toString(16);
    const trap = tameTrap.get(key);
    if (trap && trap.y === wildPoint[1]) {
      const k = nMax + trap.distance - wildDist;
      if (k >= nMin && k <= nMax) {
        console.log(`\n  ★★★ KANGAROO MATCH at step ${step} ★★★`);
        console.log(`  Candidate: 0x${k.toString(16)}`);
        const verify = pointMul(k);
        if (compressPoint(verify) === compressPoint(target)) {
          console.log(`  ✓ VERIFIED! Private key: 0x${k.toString(16)}`);
          inversionState.stats.found = true;
          inversionState.stats.privateKey = '0x' + k.toString(16);
          inversionState.running = false;
          broadcastUpdate({ type: 'found', privateKey: '0x' + k.toString(16), strategy: 'kangaroo' });
          return;
        }
      }
    }

    const j = hashPoint(wildPoint);
    wildDist += jumpDistances[j];
    wildPoint = pointAdd(wildPoint, jumpPoints[j]);

    inversionState.stats.kangarooSteps++;
    inversionState.stats.iterations++;

    if (step > 0 && step % 100000 === 0) {
      const elapsed = (Date.now() - startTime) / 1000;
      const rate = (step / elapsed).toFixed(0);
      broadcastUpdate({
        type: 'kangaroo',
        phase: 'wild',
        step,
        maxSteps,
        rate,
        elapsed: elapsed.toFixed(1)
      });
      console.log(`  Wild: ${step.toLocaleString()} / ${maxSteps.toLocaleString()} | ${rate} steps/s`);
    }
  }

  console.log(`  Kangaroo did not converge within ${maxSteps} steps.`);
  broadcastUpdate({ type: 'kangaroo', status: 'exhausted', steps: maxSteps });
}

// ── Incremental Search ──
function runIncrementalSearch() {
  if (!inversionState.targetPubkey) return;

  console.log(`\n═══ INCREMENTAL SEARCH ═══`);
  const targetPubkey = inversionState.targetPubkey;
  const startKey = inversionState.nMin;
  const count = 50000;

  let currentPoint = pointMul(startKey);
  const G = [GX, GY];
  const startTime = Date.now();

  for (let i = 0; i < count && inversionState.running; i++) {
    const computedHex = compressPoint(currentPoint);
    if (computedHex === targetPubkey) {
      const k = startKey + BigInt(i);
      console.log(`\n  ★★★ PRIVATE KEY FOUND (INCREMENTAL) ★★★`);
      console.log(`  Key: 0x${k.toString(16)}`);
      inversionState.stats.found = true;
      inversionState.stats.privateKey = '0x' + k.toString(16);
      inversionState.running = false;
      broadcastUpdate({ type: 'found', privateKey: '0x' + k.toString(16), strategy: 'incremental' });
      return;
    }

    currentPoint = pointAdd(currentPoint, G);
    inversionState.stats.incrementalSteps++;
    inversionState.stats.iterations++;

    if (i > 0 && i % 5000 === 0) {
      const elapsed = (Date.now() - startTime) / 1000;
      const rate = (i / elapsed).toFixed(0);
      broadcastUpdate({
        type: 'incremental',
        step: i,
        count,
        rate,
        key: '0x' + (startKey + BigInt(i)).toString(16)
      });
      console.log(`  Incremental: ${i} / ${count} | ${rate} keys/s | key=0x${(startKey + BigInt(i)).toString(16).slice(0,16)}...`);
    }
  }

  console.log(`  Incremental: ${count} keys checked, not found in this range.`);
  broadcastUpdate({ type: 'incremental', status: 'exhausted', checked: count });
}

// ── Fractal-Guided Search ──
function runFractalGuidedSearch() {
  if (!inversionState.targetPoint) return;

  console.log(`\n═══ FRACTAL-GUIDED SEARCH ═══`);
  const target = inversionState.targetPoint;
  const targetPubkey = inversionState.targetPubkey;
  const nMin = inversionState.nMin;
  const nMax = inversionState.nMax;
  const anomalyMap = inversionState.anomalyMap;
  const fractal = inversionState.fractalResult;

  if (!anomalyMap || !fractal) {
    console.log('  No fractal analysis available, skipping');
    return;
  }

  // Use resonance anomalies to define search sub-ranges
  // The idea: anomalies in specific rounds/scales may hint at
  // structural weaknesses that correlate with certain key patterns
  const weakRounds = anomalyMap.weakRounds;
  const weakScales = anomalyMap.weakScales;
  const biasedWords = [];
  for (let i = 0; i < fractal.walshHadamard.spectra.length; i++) {
    if (fractal.walshHadamard.spectra[i].flatness > 2.0) {
      biasedWords.push(i);
    }
  }

  console.log(`  Weak rounds: ${weakRounds.join(', ') || 'none'}`);
  console.log(`  Weak scales: ${weakScales.join(', ') || 'none'}`);
  console.log(`  Biased words: ${biasedWords.join(', ') || 'none'}`);

  // Generate candidate keys guided by fractal structure
  const batchSize = 20000;
  const startTime = Date.now();
  let checked = 0;

  // Strategy: sample from sub-ranges derived from anomaly structure
  // Use spectral biases to define interesting bit patterns
  const candidates = new Set();

  // Phase 1: Keys with pattern bits derived from anomaly structure
  for (let i = 0; i < batchSize && inversionState.running; i++) {
    let candidate = nMin;

    // Mix in anomaly-derived bits
    for (const scale of weakScales) {
      const bitPos = Number(scale % BigInt(inversionState.puzzleNum - 1));
      candidate ^= (1n << BigInt(bitPos));
    }

    // Mix in spectral bias patterns
    for (const word of biasedWords) {
      const bitOffset = word * 32;
      if (bitOffset < inversionState.puzzleNum - 1) {
        candidate ^= (BigInt(i * 7 + word * 31) << BigInt(bitOffset));
      }
    }

    // Add iteration-specific variation
    candidate ^= (BigInt(i) * 1103515245n + 12345n);
    candidate = candidate % (nMax - nMin + 1n) + nMin;

    if (candidate >= nMin && candidate <= nMax && !candidates.has(candidate.toString())) {
      candidates.add(candidate.toString());

      const point = pointMul(candidate);
      const computedHex = compressPoint(point);

      checked++;
      inversionState.stats.iterations++;

      if (computedHex === targetPubkey) {
        console.log(`\n  ★★★ PRIVATE KEY FOUND (FRACTAL-GUIDED) ★★★`);
        console.log(`  Key: 0x${candidate.toString(16)}`);
        inversionState.stats.found = true;
        inversionState.stats.privateKey = '0x' + candidate.toString(16);
        inversionState.running = false;
        broadcastUpdate({ type: 'found', privateKey: '0x' + candidate.toString(16), strategy: 'fractal-guided' });
        return;
      }

      if (checked % 2000 === 0) {
        const elapsed = (Date.now() - startTime) / 1000;
        const rate = (checked / elapsed).toFixed(1);
        broadcastUpdate({
          type: 'fractal-guided',
          checked,
          batchSize,
          rate,
          key: '0x' + candidate.toString(16).slice(0, 16) + '...'
        });
        console.log(`  Fractal: ${checked} / ${batchSize} | ${rate} keys/s`);
      }
    }
  }

  console.log(`  Fractal-guided: ${checked} candidates checked, not found.`);
  broadcastUpdate({ type: 'fractal-guided', status: 'exhausted', checked });
}

// ═══════════════════════════════════════════════════════════════════════════════
// WEBSOCKET — Real-time updates
// ═══════════════════════════════════════════════════════════════════════════════

function broadcastUpdate(data) {
  const msg = JSON.stringify(data);
  wss.clients.forEach(client => {
    if (client.readyState === 1) {
      client.send(msg);
    }
  });
}

wss.on('connection', (ws) => {
  console.log('WebSocket client connected');
  ws.send(JSON.stringify({
    type: 'status',
    running: inversionState.running,
    puzzleNum: inversionState.puzzleNum,
    stats: inversionState.stats
  }));

  ws.on('message', (msg) => {
    try {
      const data = JSON.parse(msg);
      if (data.type === 'ping') {
        ws.send(JSON.stringify({ type: 'pong' }));
      }
    } catch (e) {}
  });
});

// ═══════════════════════════════════════════════════════════════════════════════
// START SERVER
// ═══════════════════════════════════════════════════════════════════════════════

const PORT = process.env.PORT || 3000;
server.listen(PORT, () => {
  console.log('╔════════════════════════════════════════════════════════════════╗');
  console.log('║              VORTEX PRIME — Backend Server v3                 ║');
  console.log('║     Discrete Fractal Analysis + secp256k1 ECDLP Solver        ║');
  console.log('╚════════════════════════════════════════════════════════════════╝');
  console.log(`\n  Server: http://localhost:${PORT}`);
  console.log(`  WebSocket: ws://localhost:${PORT}`);
  console.log(`  Default puzzle: #${inversionState.puzzleNum}`);
  console.log(`  Range: [2^${inversionState.puzzleNum-1}, 2^${inversionState.puzzleNum})`);
  console.log(`  API: POST /api/analyze, POST /api/inversion/start`);
  console.log(`       GET  /api/inversion/status, POST /api/inversion/stop`);
  console.log(`       POST /api/range`);
  console.log('');
});
