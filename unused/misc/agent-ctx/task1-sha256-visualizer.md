# Task: SHA-256 Avalanche Bit-Diffusion Visualizer

## Summary
Built a complete interactive web application that visualizes how bits propagate and diffuse through the 64 rounds of SHA-256 compression.

## Files Created

### Core Libraries
1. **`src/lib/sha256-engine.ts`** - Full SHA-256 compression implementation with round-by-round state capture
   - Implements complete message schedule (W[0..63]) and 64-round compression
   - Captures (a,b,c,d,e,f,g,h) after each round
   - Includes `verifySha256()` - verified against 3 NIST test vectors (all pass)
   - Helper functions: `flipBit()`, `getBit()`, `getWordBit()`, `sha256Full()`, `hashToHex()`

2. **`src/lib/diffusion-analyzer.ts`** - Diffusion computation engine
   - `computeDiffusion()` - compares two traces round-by-round
   - `computeFullAnalysis()` - runs SHA-256 twice (base + 1 bit flip)
   - `computeAvalancheProfile()` - full 256-bit avalanche profile
   - `findAvalanchePoint()` - finds round where diffusion hits 50%
   - `getWordHeatmapData()` - 8×64 heatmap data

### UI Components
3. **`src/components/bit-grid.tsx`** - 2D 8×32 bit grid visualization
   - Color coding: dark (0-bit), green (1-bit), orange (changed), cyan (influenced)
   - Tooltips showing word name, bit index, value, change status
   - Responsive design with hover effects

4. **`src/components/round-controls.tsx`** - Round stepper with play/pause animation
   - Slider (0-63), step forward/backward, first/last
   - Play/pause with auto-advance at configurable speed (0.5x, 1x, 2x, 5x)

5. **`src/components/diffusion-chart.tsx`** - Recharts diffusion curve
   - Line chart: diffusion % vs. round number
   - 50% reference line, avalanche zone markers (R16-R24)
   - Click-to-jump to any round
   - Gradient coloring: green → cyan → orange

6. **`src/components/word-heatmap.tsx`** - 8×64 word-level heatmap
   - Rows = words a-h, columns = rounds 0-63
   - Color intensity = diffusion percentage per word
   - Clickable cells to jump to specific round
   - Color legend

7. **`src/components/analysis-dashboard.tsx`** - Multi-panel analysis view
   - Statistics card (diffusion %, active bits, entropy, avalanche point)
   - Diffusion progress bar with gradient
   - Per-word mini bar chart
   - Diffusion curve chart
   - Word-level heatmap

8. **`src/components/input-panel.tsx`** - Input configuration
   - Hex input for 512-bit block
   - Clickable 256-bit grid to select which bit to flip
   - Preset examples: All Zeros, Random, Bitcoin Header

### Main Page
9. **`src/app/page.tsx`** - Main page layout
   - Dark cryptographic/hacker aesthetic
   - Responsive grid (5/7 split on desktop, stacked on mobile)
   - SHA-256 verification badge in header
   - Round detail panel with T1, T2, W[t], word hex values
   - Hash comparison section showing base vs modified vs diff

## Verification
- SHA-256 implementation verified against NIST test vectors:
  - SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 ✓
  - SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad ✓
  - SHA-256("abcdbcde...") = 248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1 ✓

## Lint Status
All lint checks pass with zero errors.
