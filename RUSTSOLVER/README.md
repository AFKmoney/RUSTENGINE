# RUSTSOLVER v1.0 — VORTEX PRIME LBE Solver

Optimized Lattice Ball Enumeration (LBE) solver for Bitcoin Puzzle P135.

## Pipeline

```
6D Lattice (LLL) → Babai CVP → Lattice Kangaroo → KEY
```

## Key Optimizations

1. **FAST reduce512()** — Native u64x4 field multiplication using secp256k1 special form (10-100x faster than BigUint)
2. **Jacobian coordinates** — No field inversion per hop (8M+3S per mixed add)
3. **Exact rational LLL** — Gram-Schmidt with rational arithmetic for correct shortest vectors
4. **BigUint point decompression** — Guaranteed correct (bypasses native pow() bug)
5. **Lattice kangaroo** — Walks in 6D coefficient space using basis step points
6. **Full scalar distance tracking** — Correct key recovery from kangaroo collisions

## Build & Run

```bash
cargo build --release
./target/release/rustsolver --mode lbe --target 135
```

## Modes

- `lbe` — Full LBE pipeline (lattice + kangaroo)
- `lattice` — 6D lattice analysis only (LLL + CVP)
- `test` — Validation suite (EC, field, lattice)

## Benchmarks

- **EC ops**: ~3.9M Jacobian mixed-add/s
- **P70 CVP residual**: 2^23 bits (validated, matches Python prototype)
- **P135 CVP residual**: 2^134 bits (needs stronger lattice reduction for P135)

## Status

- ✅ EC arithmetic verified (2*G, 7*G, Beta^3, P70 scalar mul)
- ✅ P135 target point decompression works
- ✅ 6D Lattice LLL reduction works
- ✅ Babai CVP decomposition works
- ✅ Kangaroo search works at ~880K hops/s
- ⚠️ P135 CVP residual too large (2^134 vs needed 2^43) — needs improved lattice basis or BKZ reduction

## Next Steps for P135

The 6D lattice basis needs improvement for P135:
1. **BKZ reduction** instead of LLL (stronger, finds shorter vectors)
2. **Better basis construction** — use more constraints from Z[ω] structure
3. **Higher-dimensional lattice** (8D or 12D) for smaller components
4. **GPU acceleration** — kangaroo at 10^9 hops/s on GPU cluster
