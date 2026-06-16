# Task: Bitcoin Key Pipeline + SHA-256 Fractal Analyzer

## Agent: Main Developer

## Files Created/Modified

### 1. `src/lib/secp256k1.ts` — Elliptic Curve Math (Pure TypeScript)
- Implemented secp256k1 curve parameters (P, N, A, B, G)
- Modular arithmetic: mod(), modInverse() via Fermat's little theorem, modPow()
- Point operations: pointAdd(), pointDouble(), scalarMultiply() (double-and-add)
- Public key generation: getPublicKey() → compressed (33 bytes) and uncompressed (65 bytes)
- Key utilities: parsePublicKey(), decompressPublicKey(), validatePublicKey()
- Random key generation: generateRandomPrivateKey()
- Verification: verifySecp256k1() — all 8 test vectors pass
  - privkey=1 → known compressed pubkey ✓
  - privkey=2 → known compressed pubkey ✓  
  - privkey=3 → known compressed pubkey ✓
  - G + G = 2G (point addition) ✓
  - 2G matches privkey=2 ✓
  - N*G = point at infinity ✓
  - Uncompressed format ✓

### 2. `src/lib/bitcoin-pipeline.ts` — Bitcoin Key Derivation Pipeline
- computePipeline() / computePipelineFromBytes() — full forward pipeline
- pubkeyToSha256Block() — pads compressed pubkey (33 bytes) to SHA-256 block (64 bytes)
- input33ToSha256Block() — same padding for random 33-byte inputs (for comparison)
- Hamming distance: hammingDistanceHex()
- Key space explorer: exploreKeySpace(), computeKeySpaceDistances()
- Random comparison: generateRandomInput33(), generateRandomComparisonBatch(), averageConsecutiveHamming()

### 3. `src/components/bitcoin-pipeline-panel.tsx` — UI Component
Four sections:
- **Section A: Key Input** — private/public key input, generate random, validate, secp256k1 test badge
- **Section B: Pipeline Visualization** — visual flow diagram with SHA-256 step highlighted
- **Section C: Fractal Analysis** — runs computeFullDiscreteAnalysis on pubkey vs random input, side-by-side comparison
- **Section D: Key Space Explorer** — slider for key range, Hamming distance chart, hash heatmap, key mapping list

### 4. `src/app/page.tsx` — Updated with Tab Navigation
- Added tab bar below header with "Avalanche Visualizer" and "Bitcoin Pipeline" tabs
- Avalanche tab shows all existing content unchanged
- Bitcoin Pipeline tab shows BitcoinPipelinePanel component

## Verification
- `bun run lint` passes with zero errors
- All secp256k1 test vectors verified via npx tsx
- Pipeline produces correct SHA-256 hashes for known keys
- SHA-256 block padding is correct (0x80 at index 33, length field at end)
- Key space explorer computes correct Hamming distances
- Dev server compiles and serves pages successfully (200 status)

## Architecture Notes
- All computation is client-side (no API routes)
- secp256k1 uses BigInt throughout for 256-bit precision
- SHA-256 block padding mirrors the real Bitcoin pipeline
- Fractal analysis reuses existing computeFullDiscreteAnalysis from discrete-fractal.ts
- Comparison: SHA-256(pubkey) vs SHA-256(random 33 bytes) — this is the core research question
