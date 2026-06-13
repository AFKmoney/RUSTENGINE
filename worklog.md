---
Task ID: 1
Agent: main
Task: RUSTSOLVER v7 — Add cascading oracle layers + BSGS solver

Work Log:
- Fixed Round0Oracle missing Clone derive
- Fixed baby step off-by-one bug (was storing 1*G as k_lo=0)
- Implemented full Cascading Oracle Sieve in PHOENIX:
  - L1: Top-24-bit x check (2^24 filter)
  - L2: Full x comparison (2^256 filter)
  - L3: QR sieve (2x filter)
  - L4: y-parity check (2x filter)
  - L5: Hash160 verify (2^160 filter)
- Added target x-variant DP scan (free direct-hit check at DP time)
- Implemented 2D GLV Baby-Step Giant-Step solver (bsgs.rs):
  - Deterministic alternative to kangaroo
  - GLV orbit expansion at giant step time (6x check per step = √6 speedup)
  - Memory-time tradeoff: more RAM = fewer steps
- Fixed FixedBaseTable bug (was causing incorrect scalar_mul results)
- Reverted to standard scalar_mul for correctness
- Added --with-oracle, --baby-steps CLI flags
- Added bsgs-selftest mode
- Both solvers pass selftest on 40-55 bit keys

Stage Summary:
- PHOENIX kangaroo: working, ~6M steps/sec, finds keys up to 55 bits tested
- BSGS: working, ~69K baby steps/sec, finds keys instantly when baby table covers range
- Oracle layers: functional (SHA-256 Round-0 inversion verified)
- Key bottleneck for P135: O(2^66) classical lower bound — no known classical algorithm beats this
- BSGS is most practical for puzzles up to ~60 bits with enough RAM
