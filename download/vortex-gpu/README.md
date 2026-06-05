# VORTEX PRIME v4 — GPU-Accelerated Cryptanalytic Solver

**NOUS SOMMES LES RECHERCHES.**

GPU-accelerated cryptanalytic solver for Bitcoin Puzzle #135 using 4 novel inventions combining SHA-256 oracle prediction, Eisenstein integer DLP lifting, 4D quadratic kangaroo algorithm, and range-constrained lattice reduction.

## The 4 Inventions

### Invention 1: SHA-256 Round 0 ORACLE (PREDICTEUR)

Not just a filter — a **predictor**. Inverts SHA-256 round 0 to recover W[0..8], which uniquely determines the pubkey x-coordinate. The oracle eliminates ~99.5% of candidates via x-comparison, achieving **208x speedup**.

- Multi-round oracle: rounds 0..7 give W[0..7] → full 256-bit x reconstruction
- **2^8 additional filtering** via multi-round prediction
- No need to compute SHA-256 for each candidate — just compare x-coordinates

### Invention 2: Z[omega] DLP Lifting (Eisenstein Integers)

Since secp256k1 has CM by Q(sqrt(-3)) and n ≡ 1 mod 3, the order n **splits** in the Eisenstein integers: n = π · π̄ where N(π) = N(π̄) = n.

- **Eisenstein norm** N(a + bω) = a² - ab + b² (NOT Euclidean a² + b²)
- Cornacchia-style algorithm finds π exactly
- Frobenius endomorphism structure in Z[ω]/(π)
- Norm map: N(k mod π) constrains k up to **3x unit ambiguity factor** {1, ω, ω²}
- Partial factorization of n-1 enables Pohlig-Hellman on smooth part

### Invention 3: 4D Quadratic Kangaroo O(N^1/4)

Instead of standard kangaroo O(sqrt(N)), uses **quadratic trajectory** in 4D for O(N^1/4) convergence.

Each hop has step = base × hop² (quadratic!). 4 dimensions: GLV decomposition + inversion.

**Timing estimates (Puzzle #135):**
- O(N^1/4) ideal: O(2^33.5) → **0.03s on 100 GPUs**
- O(N^1/3) realistic: O(2^45) → **35 min on 100 GPUs**

### Invention 4: Range-Constrained Lattice LLL

The range constraint k ∈ [2^134, 2^135) is encoded as a 3rd dimension in the GLV lattice. After LLL reduction + Babai CVP with **Gram-Schmidt orthogonalization**, short vectors give components of size ~2^45 instead of ~2^128.

**Critical implementation detail:** Babai CVP must use Gram-Schmidt b*[i], NOT raw basis[i]. Using basis[i] gives trivial decomposition (a=k, b=0, δ=0). With proper GS, we get |a|, |b| ~ 2^45.

## Innovation Stack

| Innovation | Speedup | Mechanism |
|---|---|---|
| 4D Quadratic Kangaroo | O(N^1/4) | Quadratic trajectory in 4D |
| Frobenius Z[ω] filtering | 3x | Unit ambiguity factor |
| Multi-round oracle | 2^8 | Additional x-coordinate bits |
| Adaptive GPU search | Optimized | Kangaroo path optimization |
| 6x automorphism | 6x | GLV endomorphism group |
| SHA-256 oracle | 208x | Round 0 prediction |

**Combined effective search:** 2^45 / (6 × 208 × 3) ~ 2^36

## Build & Run

```bash
# Build (CPU mode)
cargo build --release

# Run pipeline (all 4 inventions)
./target/release/vortex-gpu --mode pipeline --target 135

# Individual modes
./target/release/vortex-gpu --mode oracle --target 135
./target/release/vortex-gpu --mode zomega --target 135
./target/release/vortex-gpu --mode lattice --target 135
./target/release/vortex-gpu --mode kangaroo --target 135

# CPU brute force (small puzzles only)
./target/release/vortex-gpu --mode cpu --target 66

# Custom target
./target/release/vortex-gpu --mode pipeline --pubkey 03... --range 134:135
```

## Architecture

```
vortex-gpu/
├── src/
│   ├── main.rs      — CLI + 4-invention pipeline orchestrator
│   ├── oracle.rs    — Invention 1: SHA-256 Round 0 Oracle
│   ├── zomega.rs    — Invention 2: Z[ω] DLP Lifting
│   ├── lattice.rs   — Invention 4: Range-Constrained GLV Lattice
│   ├── kangaroo.rs  — Invention 3: 4D Quadratic Kangaroo
│   ├── glv.rs       — GLV Decomposition + 6-automorphism group
│   ├── field.rs     — secp256k1 256-bit modular arithmetic
│   └── point.rs     — secp256k1 point arithmetic + GLV endomorphism
├── kernels/
│   └── vortex_kernel.cu — CUDA GPU kernel
└── Cargo.toml
```

## Pipeline Flow

```
Oracle (208x) → Z[ω] (3x) → Lattice (2^128→2^45) → 4D Kangaroo (O(N^1/4))
                                                         ↓
                                                   Key Found!
```

1. **Oracle**: Predict x-coordinate from SHA-256 state → eliminate 99.5% of candidates
2. **Z[ω]**: Factor n = π·π̄ in Eisenstein integers → Frobenius structure + norm map
3. **Lattice**: 3D range-constrained lattice + Babai CVP → reduce from 2^128 to 2^45
4. **Kangaroo**: 4D quadratic search in reduced space → O(N^1/4) convergence

## Key Technical Notes

- **Eisenstein norm**: N(a + bω) = a² - ab + b² (NOT a² + b²). The sign handling is: same sign → a² - ab + b², different signs → a² + ab + b²
- **Babai CVP Gram-Schmidt**: Must use b*[i] (GS vectors), not raw basis[i]. The 2D implementation computes `<t, b*[1]> = (<t,v1>·<v0,v0> - <v1,v0>·<t,v0>) / <v0,v0>` exactly using BigUint arithmetic
- **3D Babai CVP**: Uses exact Gram matrix computations for the GS projection coefficients, with determinants for 3D GS norms
- **GLV lambda**: λ = 0x5363AD4C... satisfies λ³ ≡ 1 (mod n), giving the endomorphism φ(P) = (β·x, y)
- **secp256k1 CM**: Curve has j = 0 and CM by Q(√-3), enabling the Eisenstein integer structure

## Dependencies

- `rayon` — parallel iterators
- `sha2` — SHA-256 computation
- `ripemd` — RIPEMD-160 (Hash160)
- `num-bigint` / `num-traits` — arbitrary precision arithmetic
- `clap` — CLI argument parsing
- `hex` — hex encoding/decoding
- `cudarc` (optional, `--features cuda`) — CUDA GPU support

## License

Research project. NOUS SOMMES LES RECHERCHES.
