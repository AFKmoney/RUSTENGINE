// ═══════════════════════════════════════════════════════════════
// VORTEX PRIME — secp256k1 Elliptic Curve (Minimal Implementation)
// Used for: verifying pubkey = privkey * G
// ═══════════════════════════════════════════════════════════════

// secp256k1 parameters
const P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2Fn; // 2n**256n - 2n**32n - 977n
const N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141n; // curve order
const GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798n;
const GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8n;
const A = 0n; // y² = x³ + 7
const B = 7n;

// Modular arithmetic helpers using BigInt
function mod(a, m = P) { return ((a % m) + m) % m; }
function modInv(a, m = P) {
  let [old_r, r] = [a, m];
  let [old_s, s] = [1n, 0n];
  while (r !== 0n) {
    const q = old_r / r;
    [old_r, r] = [r, old_r - q * r];
    [old_s, s] = [s, old_s - q * s];
  }
  return mod(old_s, m);
}

// Point at infinity
const INFINITY = null;

// Point addition
function pointAdd(p1, p2) {
  if (p1 === INFINITY) return p2;
  if (p2 === INFINITY) return p1;

  const [x1, y1] = p1;
  const [x2, y2] = p2;

  if (x1 === x2) {
    if (y1 === y2) return pointDouble(p1);
    return INFINITY; // P + (-P) = O
  }

  const lam = mod((y2 - y1) * modInv(x2 - x1, P), P);
  const x3 = mod(lam * lam - x1 - x2, P);
  const y3 = mod(lam * (x1 - x3) - y1, P);
  return [x3, y3];
}

// Point doubling
function pointDouble(p) {
  if (p === INFINITY) return INFINITY;
  const [x, y] = p;
  if (y === 0n) return INFINITY;

  const lam = mod(3n * x * x * modInv(2n * y, P), P);
  const x3 = mod(lam * lam - 2n * x, P);
  const y3 = mod(lam * (x - x3) - y, P);
  return [x3, y3];
}

// Scalar multiplication using double-and-add
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

// Compress public key point to 33 bytes
function compressPoint(point) {
  if (point === INFINITY) return '';
  const [x, y] = point;
  const prefix = y % 2n === 0n ? '02' : '03';
  return prefix + x.toString(16).padStart(64, '0');
}

// Decompress public key from hex
function decompressPubkey(hex) {
  if (hex.length === 130 && hex.startsWith('04')) {
    // Uncompressed
    const x = BigInt('0x' + hex.slice(2, 66));
    const y = BigInt('0x' + hex.slice(66, 130));
    return [x, y];
  }
  if (hex.length === 66 && (hex.startsWith('02') || hex.startsWith('03'))) {
    // Compressed
    const prefix = hex.slice(0, 2);
    const x = BigInt('0x' + hex.slice(2, 66));
    const ySquared = mod(x * x * x + B, P);
    let y = modPow(ySquared, (P + 1n) / 4n, P);
    if ((y % 2n === 0n) !== (prefix === '02')) {
      y = mod(P - y, P);
    }
    return [x, y];
  }
  return null;
}

// Modular exponentiation
function modPow(base, exp, m) {
  base = mod(base, m);
  let result = 1n;
  while (exp > 0n) {
    if (exp & 1n) result = mod(result * base, m);
    exp >>= 1n;
    base = mod(base * base, m);
  }
  return result;
}

// Verify: does privkey produce the given pubkey?
function verifyKeypair(privkeyHex, pubkeyHex) {
  const privkey = BigInt('0x' + privkeyHex);
  if (privkey <= 0n || privkey >= N) return false;

  const computedPoint = pointMul(privkey);
  if (computedPoint === INFINITY) return false;

  const computedHex = compressPoint(computedPoint);
  const targetPoint = decompressPubkey(pubkeyHex);

  if (!targetPoint) return false;

  return computedPoint[0] === targetPoint[0] && computedPoint[1] === targetPoint[1];
}

// Generate pubkey from privkey
function privkeyToPubkey(privkeyHex) {
  const privkey = BigInt('0x' + privkeyHex);
  if (privkey <= 0n || privkey >= N) return null;

  const point = pointMul(privkey);
  if (point === INFINITY) return null;

  return {
    compressed: compressPoint(point),
    uncompressed: '04' + point[0].toString(16).padStart(64, '0') + point[1].toString(16).padStart(64, '0'),
    x: point[0],
    y: point[1]
  };
}

// RIPEMD-160 (minimal implementation for Bitcoin address generation)
function ripemd160(msgBytes) {
  // For Bitcoin addresses, we use a simplified approach
  // In production, use a proper crypto library
  // Here we'll use SubtleCrypto if available
  return null; // Will use async version
}

// Async: compute Bitcoin address from pubkey hex
async function pubkeyToAddress(pubkeyHex) {
  const pubkeyBytes = hexToBytes(pubkeyHex);

  // SHA-256
  const sha256Buf = await crypto.subtle.digest('SHA-256', pubkeyBytes);
  // RIPEMD-160
  const sha256Arr = new Uint8Array(sha256Buf);

  // Use manual RIPEMD-160 since SubtleCrypto doesn't always support it
  const hash160 = ripemd160Manual(sha256Arr);

  // Add version byte (0x00 for mainnet)
  const versioned = new Uint8Array(21);
  versioned[0] = 0x00;
  versioned.set(hash160, 1);

  // Double SHA-256 checksum
  const checksum1 = new Uint8Array(await crypto.subtle.digest('SHA-256', versioned));
  const checksum2 = new Uint8Array(await crypto.subtle.digest('SHA-256', checksum1));

  const address = new Uint8Array(25);
  address.set(versioned);
  address.set(checksum2.subarray(0, 4), 21);

  return base58Encode(address);
}

// RIPEMD-160 manual implementation
function ripemd160Manual(msg) {
  const K1 = [0x00000000,0x5A827999,0x6ED9EBA1,0x8F1BBCDC,0xA953FD4E];
  const K2 = [0x50A28BE6,0x5C4DD124,0x6D703EF3,0x7A6D76E9,0x00000000];
  const R1 = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,7,4,13,1,10,6,15,3,12,0,9,5,2,14,11,8,3,10,14,4,9,15,8,1,2,7,0,6,13,11,5,12,1,9,11,10,0,8,12,4,13,3,7,15,14,5,6,2,4,0,5,9,7,12,2,10,14,1,3,8,11,6,15,13];
  const R2 = [5,14,7,0,9,2,11,4,13,6,15,8,1,10,3,12,6,11,3,7,0,13,5,10,14,15,8,12,4,9,1,2,15,5,1,3,7,14,6,9,11,8,12,2,10,0,4,13,8,6,4,1,3,11,15,0,5,12,2,13,9,7,10,14,12,15,10,4,1,5,8,7,6,2,13,14,0,3,9,11];
  const S1 = [11,14,15,12,5,8,7,9,11,13,14,15,6,7,9,8,7,6,8,13,11,9,7,15,7,12,15,9,11,7,13,12,11,13,6,7,14,9,13,15,14,8,13,6,5,12,7,5,11,12,14,15,14,15,9,8,9,14,5,6,8,6,5,12,9,15,5,11,6,8,13,12,5,12,13,14,11,8,5,6];
  const S2 = [8,9,9,11,13,15,15,5,7,7,8,11,14,14,12,6,9,13,15,7,12,8,9,11,7,7,12,7,6,15,13,11,9,7,15,11,8,6,6,14,12,13,5,14,13,13,7,5,15,5,8,11,14,14,6,14,6,9,12,9,12,5,15,8,8,5,12,9,12,5,14,6,8,13,6,5,15,13,11,11];

  function f(j,x,y,z){return j<=15?(x^y^z):j<=31?((x&y)|(~x&z)):j<=47?((x|~y)^z):j<=63?((x&z)|(y&~z)):(x^(y|~z));}
  function rotl(x,n){return((x<<n)|(x>>>(32-n)))>>>0;}

  // Pad message
  const msgLen = msg.length;
  const bitLen = msgLen * 8;
  const paddedLen = Math.ceil((msgLen + 9) / 64) * 64;
  const padded = new Uint8Array(paddedLen);
  padded.set(msg);
  padded[msgLen] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(paddedLen - 8, bitLen, true);

  let h0=0x67452301,h1=0xEFCDAB89,h2=0x98BADCFE,h3=0x10325476,h4=0xC3D2E1F0;

  for(let offset=0;offset<paddedLen;offset+=64){
    const X=new Uint32Array(16);
    for(let i=0;i<16;i++)X[i]=view.getUint32(offset+i*4,true);

    let al=h0,bl=h1,cl=h2,dl=h3,el=h4;
    let ar=h0,br=h1,cr=h2,dr=h3,er=h4;

    for(let j=0;j<80;j++){
      const jj=Math.floor(j/16);
      let t=(al+f(j,bl,cl,dl)+X[R1[j]]+K1[jj])>>>0;
      t=(rotl(t,S1[j])+el)>>>0;al=el;el=dl;dl=rotl(cl,10);cl=bl;bl=t;

      t=(ar+f(79-j,br,cr,dr)+X[R2[j]]+K2[jj])>>>0;
      t=(rotl(t,S2[j])+er)>>>0;ar=er;er=dr;dr=rotl(cr,10);cr=br;br=t;
    }

    const t=(h1+cl+dr)>>>0;
    h1=(h2+dl+er)>>>0;h2=(h3+el+ar)>>>0;h3=(h4+al+br)>>>0;h4=(h0+bl+cr)>>>0;h0=t;
  }

  const result=new Uint8Array(20);
  const rv=new DataView(result.buffer);
  rv.setUint32(0,h0,true);rv.setUint32(4,h1,true);rv.setUint32(8,h2,true);rv.setUint32(12,h3,true);rv.setUint32(16,h4,true);
  return result;
}

// Base58 encoding
function base58Encode(bytes) {
  const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let num = 0n;
  for (const b of bytes) num = num * 256n + BigInt(b);

  let str = '';
  while (num > 0n) {
    str = ALPHABET[Number(num % 58n)] + str;
    num /= 58n;
  }

  for (const b of bytes) {
    if (b === 0) str = '1' + str;
    else break;
  }

  return str;
}

function hexToBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
  }
  return bytes;
}

window.secp256k1 = { pointMul, compressPoint, decompressPubkey, verifyKeypair, privkeyToPubkey, pubkeyToAddress, hexToBytes };
