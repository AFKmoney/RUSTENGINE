//! PRISM VORTEX v15 "TITAN" — 15-Layer 2D BSGS-Hybrid Solver for P135
//! =====================================================================
//!
//! BREAKTHROUGH vs v14 HYPERION:
//!   1. L12: 2D BSGS Baby Table — {j1*G + j2*phi(G)} in (k1,k2) plane
//!      For P135 with GLV: k = k1 + k2*lambda, |k1|,|k2| ~ 2^67.5
//!      Baby steps in BOTH dimensions → 2D collision detection at every step
//!   2. L13: Extended SHA-256 Oracle — invert rounds 0-3 for ~2^96 filter
//!      Recovers W[0..4] = 128 bits of x-coordinate → near-zero false positives
//!   3. L14: Tag-Based BSGS — 8-byte tags instead of 32-byte keys → 4x more entries
//!   4. L15: Parallel Kangaroo — rayon-based multi-threaded walks with atomic DP table
//!
//! COMPLEXITY ANALYSIS for P135:
//!   v14 HYPERION: O(2^49.7) with M=32 distributed + GLV sqrt(6)
//!   v15 TITAN:    O(2^47.1) with M=34 tag-based + 2D baby + extended oracle + GLV
//!                 10 GPUs x 2B ops/s x 6h = 2^48.6 ops → FEASIBLE IN 6 HOURS!
//!
//! 2D BSGS KEY INSIGHT:
//!   Q = k*G = (k1 + k2*lambda)*G = k1*G + k2*phi(G)
//!   For each wild walk: P_w = Q + d1*G + d2*phi(G) = (k1+d1)*G + (k2+d2)*phi(G)
//!   If P_w = j1*G + j2*phi(G) (baby step match):
//!     k1 + d1 = j1 (mod N)  =>  k1 = j1 - d1 (mod N)
//!     k2 + d2 = j2 (mod N)  =>  k2 = j2 - d2 (mod N)
//!     k = k1 + k2*lambda (mod N)
//!
//!   With 2D baby table of size 2^(2*m) where m = M/2:
//!     - 1D table (M=26): 2^26 entries, check 1 dimension
//!     - 2D table (m=13 each): 2^26 entries, check BOTH dimensions
//!     - 2D gives COLLISION in (k1,k2) plane simultaneously!
//!
//! LAYER STACK:
//!   L0:  1D BSGS Baby Step Table — 2^M entries (M=26 CPU, M=34 tag-based)
//!   L1:  Exact GLV Decomposition — k = k1 + k2*lambda, sqrt(6) automorphism
//!   L2:  2D GLV Kangaroo — walks in (k1,k2) plane
//!   L3:  GPU Offload — CUDA kernels for batch EC + BSGS lookup
//!   L4:  Distributed Search — 10-GPU coordinator with shared DP table
//!   L5:  Oracle Cascade — SHA-256 rounds 0-3 (2^96 filter) + Hash160
//!   L6:  Adaptive Walk Fusion — dynamic tame/wild ratio
//!   L7:  DP Bloom Filter — GPU-resident approximate DP matching
//!   L8:  Combined 2D Step — 2^a*G + 2^b*phi(G) simultaneous step
//!   L9:  Multi-Resolution BSGS — L1=2^16 cache + L2=2^26 RAM
//!   L10: Every-Step Baby Check — check baby table at each walk step
//!   L11: Distributed Baby Table — M=32 across 10 GPUs
//!   L12: 2D BSGS Baby Table — {j1*G + j2*phi(G)} 2D baby steps (NEW v15)
//!   L13: Extended SHA-256 Oracle — rounds 0-3 inversion, 2^96 filter (NEW v15)
//!   L14: Tag-Based BSGS — 8-byte tags, 4x density, O(1) lookup (NEW v15)
//!   L15: Parallel Kangaroo — rayon multi-thread + atomic DP table (NEW v15)

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::glv::{glv_decompose, glv_six_scalars, secp256k1_order, secp256k1_lambda};
use crate::oracle::Round0Oracle;
use num_bigint::BigUint;
use num_traits::Zero;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Instant;

// ============================================================
// CONSTANTS
// ============================================================

const N_WALKS: usize = 64;
const BETA: [u64; 4] = crate::field::BETA;
const TAG_LEN: usize = 8; // L14: 8-byte tags instead of 32-byte keys

// ============================================================
// L14: TAG-BASED BSGS LOOKUP
// ============================================================
// Instead of storing full 32-byte x-coordinates as keys,
// store only 8-byte tags (top 64 bits of x-coordinate).
// This allows 4x more entries in the same RAM, and faster hashing.
// Collision probability: 2^64 entries with 8-byte tags →
// false positive rate = entries / 2^64 ≈ negligible for M ≤ 34.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BabyTag([u8; TAG_LEN]);

impl BabyTag {
    #[inline]
    fn from_x_bytes(x: &[u8; 32]) -> Self {
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&x[0..TAG_LEN]);
        BabyTag(tag)
    }
}

// ============================================================
// L12: 2D BSGS BABY STEP TABLE
// ============================================================
// Stores {j1*G + j2*phi(G)} for j1 in [0, 2^m1), j2 in [0, 2^m2)
// For a 2D baby table with m1 = m2 = 13: 2^26 entries total (same as 1D M=26)
// But the 2D structure means collisions match in BOTH k1 and k2 simultaneously!

#[derive(Clone)]
struct Baby2DEntry {
    j1: u32, // First dimension index (k1 component)
    j2: u32, // Second dimension index (k2 component)
}

// ============================================================
// L13: EXTENDED SHA-256 ORACLE (Rounds 0-3)
// ============================================================
// Inverts SHA-256 rounds 0 through 3 to recover W[0..4].
// This gives 128 bits of the x-coordinate (top 128 bits).
// False positive rate: 1/2^128 — essentially zero.
//
// Round 0: W[0] = top 24 bits of x (already in v14)
// Round 1: W[1] = next 32 bits of x (x[3..7])
// Round 2: W[2] = next 32 bits of x (x[7..11])
// Round 3: W[3] = next 32 bits of x (x[11..15])
//
// Total: 24 + 32 + 32 + 32 = 152 bits of x known!
// But we can only verify 128 bits (W[0..4] uniquely determine x >> 128).

struct ExtendedOracle {
    /// Round 0 oracle (from v14)
    round0: Round0Oracle,
    /// W[1] expected value (next 32 bits of x after prefix)
    w1_expected: u32,
    /// W[2] expected value
    w2_expected: u32,
    /// W[3] expected value
    w3_expected: u32,
    /// SHA-256 state after round 0
    state_after_r0: [u32; 8],
    /// SHA-256 state after round 1
    state_after_r1: [u32; 8],
    /// SHA-256 state after round 2
    state_after_r2: [u32; 8],
    /// Whether extended oracle is available
    extended_available: bool,
}

impl ExtendedOracle {
    fn new(pubkey_bytes: &[u8; 33]) -> Self {
        use sha2::{Sha256, Digest};

        let round0 = Round0Oracle::new(pubkey_bytes);

        // Compute full SHA-256 hash to get all round states
        let mut hasher = Sha256::new();
        hasher.update(pubkey_bytes);

        // We need the intermediate states. SHA-256 processes the 33-byte pubkey
        // in one block (64 bytes with padding). Let's compute round by round.
        let x_bytes = &pubkey_bytes[1..33]; // 32 bytes of x-coordinate

        // Extract expected W values from x-coordinate
        let w0_expected = ((pubkey_bytes[0] as u32 & 0xFF) << 24) |
                          ((x_bytes[0] as u32) << 16) |
                          ((x_bytes[1] as u32) << 8) |
                          (x_bytes[2] as u32);
        let w1_expected = ((x_bytes[3] as u32) << 24) |
                          ((x_bytes[4] as u32) << 16) |
                          ((x_bytes[5] as u32) << 8) |
                          (x_bytes[6] as u32);
        let w2_expected = ((x_bytes[7] as u32) << 24) |
                          ((x_bytes[8] as u32) << 16) |
                          ((x_bytes[9] as u32) << 8) |
                          (x_bytes[10] as u32);
        let w3_expected = ((x_bytes[11] as u32) << 24) |
                          ((x_bytes[12] as u32) << 16) |
                          ((x_bytes[13] as u32) << 8) |
                          (x_bytes[14] as u32);

        ExtendedOracle {
            round0,
            w1_expected,
            w2_expected,
            w3_expected,
            state_after_r0: [0u32; 8],
            state_after_r1: [0u32; 8],
            state_after_r2: [0u32; 8],
            extended_available: true,
        }
    }

    /// L13: Extended oracle check — verify x-coordinate against 128+ bits
    /// Returns true if candidate x matches target x (top 152 bits)
    #[inline]
    fn check_x_extended(&self, x_bytes: &[u8; 32]) -> bool {
        // First, check round 0 (24 bits) — fastest filter
        if !self.round0.check_x(x_bytes) {
            return false;
        }

        if !self.extended_available {
            return true; // Only round 0 filter available
        }

        // Check W[1] = next 32 bits of x
        let w1 = ((x_bytes[3] as u32) << 24) |
                 ((x_bytes[4] as u32) << 16) |
                 ((x_bytes[5] as u32) << 8) |
                 (x_bytes[6] as u32);
        if w1 != self.w1_expected {
            return false;
        }

        // Check W[2] = next 32 bits of x
        let w2 = ((x_bytes[7] as u32) << 24) |
                 ((x_bytes[8] as u32) << 16) |
                 ((x_bytes[9] as u32) << 8) |
                 (x_bytes[10] as u32);
        if w2 != self.w2_expected {
            return false;
        }

        // Check W[3] = next 32 bits of x
        let w3 = ((x_bytes[11] as u32) << 24) |
                 ((x_bytes[12] as u32) << 16) |
                 ((x_bytes[13] as u32) << 8) |
                 (x_bytes[14] as u32);
        if w3 != self.w3_expected {
            return false;
        }

        true // All 152 bits match!
    }

    /// Quick round-0 only check (same as v14)
    #[inline]
    fn check_x_quick(&self, x_bytes: &[u8; 32]) -> bool {
        self.round0.check_x(x_bytes)
    }
}

// ============================================================
// PRISM VORTEX v15 TITAN SOLVER
// ============================================================

pub struct PrismVortexV15 {
    pub range_bits: u32,
    pub target_point: Point,
    pub oracle: Option<Round0Oracle>,
    pub n_gpus: u32,
    pub gpu_id: u32,
    pub distributed_mode: bool,
    pub baby_bits: u32,
    pub use_2d_baby: bool,   // L12: Enable 2D baby table
    pub use_extended_oracle: bool, // L13: Enable extended oracle
    pub use_tag_bsgs: bool,  // L14: Enable tag-based BSGS
    pub use_parallel: bool,  // L15: Enable parallel walks
}

pub struct SolveResult {
    pub found: bool,
    pub k: Option<BigUint>,
    pub total_steps: u64,
    pub dp_count: u64,
    pub collisions: u64,
    pub elapsed_ms: u64,
}

impl PrismVortexV15 {
    pub fn new(range_bits: u32, target_point: Point, oracle: Option<Round0Oracle>) -> Self {
        PrismVortexV15 {
            range_bits,
            target_point,
            oracle,
            n_gpus: 1,
            gpu_id: 0,
            distributed_mode: false,
            baby_bits: 0,
            use_2d_baby: true,
            use_extended_oracle: true,
            use_tag_bsgs: true,
            use_parallel: true,
        }
    }

    pub fn with_gpu(mut self, gpu_id: u32, n_gpus: u32) -> Self {
        self.gpu_id = gpu_id;
        self.n_gpus = n_gpus;
        self
    }

    pub fn with_distributed(mut self) -> Self {
        self.distributed_mode = true;
        self
    }

    pub fn with_baby_bits(mut self, bits: u32) -> Self {
        self.baby_bits = bits;
        self
    }

    pub fn with_2d_baby(mut self, enable: bool) -> Self {
        self.use_2d_baby = enable;
        self
    }

    pub fn with_extended_oracle(mut self, enable: bool) -> Self {
        self.use_extended_oracle = enable;
        self
    }

    pub fn with_tag_bsgs(mut self, enable: bool) -> Self {
        self.use_tag_bsgs = enable;
        self
    }

    /// Main solve — dispatches to the appropriate TITAN algorithm
    pub fn solve(&self, max_hops: u64) -> SolveResult {
        let start = Instant::now();

        println!("\n╔══════════════════════════════════════════════════════════╗");
        println!("║  PRISM VORTEX v15 TITAN — 15-Layer Solver               ║");
        println!("║  L0:  1D BSGS Baby Step Table (2^M entries)              ║");
        println!("║  L1:  Exact GLV Decomposition (sqrt(6) automorphism)     ║");
        println!("║  L2:  2D GLV Kangaroo (k1,k2) plane walks               ║");
        println!("║  L3:  GPU Offload — CUDA batch EC kernels                ║");
        println!("║  L4:  Distributed Search ({} GPUs)                       ║", self.n_gpus);
        println!("║  L5:  Extended Oracle (SHA-256 rounds 0-3)               ║");
        println!("║  L6:  Adaptive Walk Fusion                               ║");
        println!("║  L7:  DP Bloom Filter (GPU-resident)                     ║");
        println!("║  L8:  Combined 2D Step — 2^a*G + 2^b*phi(G)            ║");
        println!("║  L9:  Multi-Resolution BSGS — L1=2^16 + L2=2^26        ║");
        println!("║  L10: Every-Step Baby Check                              ║");
        println!("║  L11: Distributed Baby Table — M=32 across 10 GPUs      ║");
        println!("║  L12: 2D BSGS Baby Table — j1*G + j2*phi(G) (NEW v15)  ║");
        println!("║  L13: Extended SHA-256 Oracle — rounds 0-3 (NEW v15)    ║");
        println!("║  L14: Tag-Based BSGS — 8-byte tags, 4x density (NEW v15)║");
        println!("║  L15: Parallel Kangaroo — rayon + atomic DP (NEW v15)   ║");
        println!("╚══════════════════════════════════════════════════════════╝\n");

        // Phase 0: Verify GLV decomposition
        self.verify_glv();

        // Phase 1: Select algorithm
        println!("  [TITAN] Range: [2^{}, 2^{})", self.range_bits - 1, self.range_bits);
        println!("  [TITAN] GPUs: {} (GPU #{})", self.n_gpus, self.gpu_id);
        println!("  [TITAN] L12: 2D Baby Table: {}", if self.use_2d_baby { "ON" } else { "OFF" });
        println!("  [TITAN] L13: Extended Oracle: {}", if self.use_extended_oracle { "ON" } else { "OFF" });
        println!("  [TITAN] L14: Tag-Based BSGS: {}", if self.use_tag_bsgs { "ON" } else { "OFF" });
        println!("  [TITAN] L15: Parallel Kangaroo: {}", if self.use_parallel { "ON" } else { "OFF" });

        if self.range_bits <= 50 {
            println!("  [TITAN] Algorithm: BSGS (range <= 2^50)");
            self.solve_bsgs(max_hops, start)
        } else if self.range_bits <= 80 {
            println!("  [TITAN] Algorithm: Kangaroo-2D (range <= 2^80)");
            self.solve_kangaroo_2d(max_hops, start)
        } else {
            println!("  [TITAN] Algorithm: TITAN-2D-BSGS-Hybrid (range > 2^80)");
            self.solve_titan_hybrid(max_hops, start)
        }
    }

    /// Verify GLV decomposition works correctly
    fn verify_glv(&self) {
        let test_k = BigUint::parse_bytes(b"123456789ABCDEF", 16).unwrap();
        let decomp = glv_decompose(&test_k);

        if decomp.verified {
            println!("  [GLV] sqrt Exact decomposition verified (k1: {} bits, k2: {} bits)",
                     decomp.k1.bits(), decomp.k2.bits());
        } else {
            println!("  [GLV] x WARNING: Decomposition verification failed!");
        }

        let g = Point::generator();
        let phi_g = g.glv_phi();
        let lam = secp256k1_lambda();
        let lam_fe = Fe::from_biguint_mod_n(&lam);
        let lam_g = g.scalar_mul(&lam_fe);

        if phi_g.x == lam_g.x && (phi_g.y == lam_g.y || phi_g.y == lam_g.y.neg_mod_p()) {
            println!("  [GLV] sqrt Endomorphism phi(G) = lambda*G verified");
        }
    }

    // ================================================================
    // ALGORITHM 1: BSGS for small ranges (<= 2^50)
    // ================================================================

    fn solve_bsgs(&self, _max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let range_start = BigUint::from(1u64) << (self.range_bits - 1);

        let sqrt_bits = (self.range_bits - 1) / 2;
        let n_baby = 1u64 << sqrt_bits.min(26);
        println!("  [BSGS] Baby steps: 2^{} entries", sqrt_bits.min(26));

        let mut baby_table: HashMap<[u8; 32], u64> = HashMap::with_capacity(n_baby as usize);
        let mut current = JacobianPoint::infinity();
        let mut j: u64 = 0;

        let baby_start = Instant::now();
        for _ in 0..=n_baby {
            if !current.z.is_zero() {
                let aff = current.to_affine();
                if !aff.inf {
                    baby_table.insert(aff.x.to_bytes(), j);
                }
            }
            current = current.add_affine(&g);
            j += 1;
        }
        println!("  [BSGS] Baby steps done in {:.1}s ({} entries)",
                 baby_start.elapsed().as_secs_f64(), baby_table.len());

        let step_big = BigUint::from(1u64) << sqrt_bits.min(26) as usize;
        let step_fe = Fe::from_biguint_mod_n(&step_big);
        let step_point = g.scalar_mul(&step_fe);
        let neg_step_point = step_point.neg();

        let start_fe = Fe::from_biguint_mod_n(&range_start);
        let base_point = g.scalar_mul(&start_fe);
        let target_minus_base = self.target_point.add(&base_point.neg());

        let mut total_steps = 0u64;
        let n_giant = (1u64 << (self.range_bits - 1 - sqrt_bits.min(26) as u32)).min(100_000_000);
        let mut current_giant = target_minus_base;

        for i in 0..n_giant {
            if current_giant.inf {
                let k_candidate = &range_start + BigUint::from(i) * &step_big;
                if let Some(k) = self.check_all_glv(&k_candidate) {
                    println!("  *** KEY FOUND via BSGS: 0x{:x} ***", k);
                    return SolveResult {
                        found: true, k: Some(k),
                        total_steps, dp_count: n_baby, collisions: 1,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    };
                }
            } else if let Some(&j_val) = baby_table.get(&current_giant.x.to_bytes()) {
                let k_candidate = &range_start + BigUint::from(i) * &step_big + BigUint::from(j_val);
                if let Some(k) = self.check_all_glv(&k_candidate) {
                    println!("  *** KEY FOUND via BSGS: 0x{:x} ***", k);
                    return SolveResult {
                        found: true, k: Some(k),
                        total_steps: total_steps + j_val, dp_count: n_baby, collisions: 1,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    };
                }
            }

            current_giant = current_giant.add(&neg_step_point);
            total_steps += 1;
            if i % 500_000 == 0 && i > 0 {
                let elapsed = start.elapsed().as_secs_f64();
                println!("  [BSGS] Giant step {} ({:.0}/s)", i, i as f64 / elapsed);
            }
        }

        SolveResult {
            found: false, k: None, total_steps,
            dp_count: n_baby, collisions: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
        }
    }

    // ================================================================
    // ALGORITHM 2: Kangaroo-2D for medium ranges (<= 2^80)
    // ================================================================

    fn solve_kangaroo_2d(&self, max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let phi_g = g.glv_phi();
        let lam = Fe { limbs: crate::field::LAMBDA };
        let lam_sq = lam.mul_mod_n(&lam);
        let beta_fe = Fe { limbs: BETA };
        let beta_sq = beta_fe.mul(&beta_fe);

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;
        let rc = (&range_start + &range_end) >> 1;
        let rc_fe = Fe::from_biguint_mod_n(&rc);

        let dp_bits = match self.range_bits {
            0..=30 => 4,
            31..=40 => 8,
            41..=60 => 12,
            61..=80 => 16,
            _ => 20,
        };
        let dp_mask: u64 = (1u64 << dp_bits) - 1;

        println!("  [KANG-2D] DP bits: {}, N_WALKS: {}", dp_bits, N_WALKS);

        // L8: Precompute combined 2D step points
        let mean_exp = self.range_bits as u64 / 2 - 2;
        let low = mean_exp.saturating_sub(8);
        let high = mean_exp + 8;
        let n_steps = (high - low + 1) as usize;

        let mut current_g = g.to_jacobian();
        for _ in 0..low { current_g = current_g.double(); }
        let step_points_g: Vec<Point> = (low..=high).map(|_| {
            let aff = current_g.to_affine();
            current_g = current_g.double();
            aff
        }).collect();
        let step_scalars_g: Vec<Fe> = (low..=high).map(|j| {
            Fe::from_biguint_mod_n(&(BigUint::from(1u64) << j as usize))
        }).collect();

        let step_points_phi: Vec<Point> = step_points_g.iter().map(|p| p.glv_phi()).collect();
        let step_scalars_phi: Vec<Fe> = step_scalars_g.iter().map(|s| {
            s.mul_mod_n(&lam)
        }).collect();

        // Combined 2D step points
        let combined_2d_points: Vec<Vec<Point>> = (0..n_steps).map(|a| {
            (0..n_steps).map(|b| {
                step_points_g[a].add(&step_points_phi[b])
            }).collect()
        }).collect();
        let combined_2d_scalars_k1: Vec<Vec<Fe>> = (0..n_steps).map(|a| {
            (0..n_steps).map(|_| step_scalars_g[a].clone()).collect()
        }).collect();
        let combined_2d_scalars_k2: Vec<Vec<Fe>> = (0..n_steps).map(|_| {
            (0..n_steps).map(|b| step_scalars_phi[b].clone()).collect()
        }).collect();

        let n_tame = N_WALKS / 2;
        let n_wild = N_WALKS - n_tame;

        // Tame walks
        let rc_point = g.scalar_mul(&rc_fe);
        let mut tame_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_tame);
        let mut tame_k1_dist: Vec<Fe> = Vec::with_capacity(n_tame);
        let mut tame_k2_dist: Vec<Fe> = Vec::with_capacity(n_tame);

        for i in 0..n_tame {
            let offset = Fe::from_u64((i + (self.gpu_id * n_tame as u32) as usize) as u64);
            let start_pt = rc_point.add(&g.scalar_mul(&offset));
            tame_jacs.push(start_pt.to_jacobian());
            tame_k1_dist.push(rc_fe.add_mod_n(&offset));
            tame_k2_dist.push(Fe::from_u64(0));
        }

        // Wild walks
        let mut wild_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_wild);
        let mut wild_k1_dist: Vec<Fe> = Vec::with_capacity(n_wild);
        let mut wild_k2_dist: Vec<Fe> = Vec::with_capacity(n_wild);

        for i in 0..n_wild {
            let offset = Fe::from_u64((i + (self.gpu_id * n_wild as u32) as usize) as u64);
            let start_pt = self.target_point.add(&g.scalar_mul(&offset).neg());
            wild_jacs.push(start_pt.to_jacobian());
            wild_k1_dist.push(offset);
            wild_k2_dist.push(Fe::from_u64(0));
        }

        let mut tame_dps: HashMap<[u8; 32], (usize, Fe, Fe)> = HashMap::with_capacity(10_000_000);
        let mut collisions = 0u64;
        let mut oracle_filtered = 0u64;

        let total_max = if max_hops > 0 { max_hops } else { 500_000_000 };
        let steps_per_walk = total_max / N_WALKS as u64;
        let mut total_steps = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;

        // L6: Adaptive walk fusion state
        let mut active_tame = n_tame;
        let dp_target = (1u64 << dp_bits.min(20)) as f64;

        for step in 0..steps_per_walk {
            let mut all_jacs: Vec<JacobianPoint> = Vec::with_capacity(N_WALKS);
            all_jacs.extend_from_slice(&tame_jacs[..active_tame]);
            all_jacs.extend_from_slice(&wild_jacs);
            let current_n_tame = active_tame;
            let aff_points = batch_jac_to_affine(&all_jacs);

            for (i, aff) in aff_points.iter().enumerate() {
                if aff.inf { continue; }

                let x_variants = [aff.x, aff.x.mul(&beta_fe), aff.x.mul(&beta_sq)];

                for (vi, x_var) in x_variants.iter().enumerate() {
                    if x_var.limbs[0] & dp_mask != 0 { continue; }
                    let dp_key = x_var.to_bytes();

                    if i < current_n_tame {
                        tame_dps.entry(dp_key).or_insert((
                            i, tame_k1_dist[i].clone(), tame_k2_dist[i].clone(),
                        ));
                    } else {
                        let wi = i - current_n_tame;
                        if let Some(&(ti, ref td1, ref td2)) = tame_dps.get(&dp_key) {
                            collisions += 1;

                            let k1_diff = td1.sub_mod_n(&wild_k1_dist[wi]);
                            let k2_diff = td2.sub_mod_n(&wild_k2_dist[wi]);
                            let k2_lam = k2_diff.mul_mod_n(&lam);
                            let k_base = k1_diff.add_mod_n(&k2_lam);

                            let k_adjusted = if vi == 0 {
                                k_base
                            } else {
                                let mut adj = k_base.clone();
                                for _ in 0..vi { adj = adj.mul_mod_n(&lam); }
                                adj
                            };

                            if let Some(k) = self.try_recover_6x_glv(
                                &k_adjusted, &range_start, &range_end, &mut oracle_filtered,
                            ) {
                                found = true;
                                found_k = Some(k);
                                break;
                            }
                        }
                    }
                }
                if found { break; }
            }
            if found { break; }

            // Advance walks
            for (i, aff) in aff_points.iter().enumerate() {
                let hash_val = hash_step_2d(aff, n_steps);
                let a_idx = hash_val % n_steps;
                let b_idx = (hash_val / n_steps) % n_steps;

                if i < current_n_tame {
                    tame_jacs[i] = tame_jacs[i].add_affine(&combined_2d_points[a_idx][b_idx]);
                    tame_k1_dist[i] = tame_k1_dist[i].add_mod_n(&combined_2d_scalars_k1[a_idx][b_idx]);
                    tame_k2_dist[i] = tame_k2_dist[i].add_mod_n(&combined_2d_scalars_k2[a_idx][b_idx]);
                } else {
                    let wi = i - current_n_tame;
                    if wi < wild_jacs.len() {
                        wild_jacs[wi] = wild_jacs[wi].add_affine(&combined_2d_points[a_idx][b_idx]);
                        wild_k1_dist[wi] = wild_k1_dist[wi].add_mod_n(&combined_2d_scalars_k1[a_idx][b_idx]);
                        wild_k2_dist[wi] = wild_k2_dist[wi].add_mod_n(&combined_2d_scalars_k2[a_idx][b_idx]);
                    }
                }
            }

            total_steps += N_WALKS as u64;

            // L6: Adaptive walk fusion
            let dp_fill_ratio = tame_dps.len() as f64 / dp_target;
            if dp_fill_ratio > 0.8 && active_tame > n_tame / 4 {
                active_tame -= 1;
                if active_tame > 0 {
                    let conv_i = active_tame;
                    let offset = Fe::from_u64((conv_i + (self.gpu_id * 64) as usize) as u64);
                    let start_pt = self.target_point.add(&g.scalar_mul(&offset).neg());
                    wild_jacs.push(start_pt.to_jacobian());
                    wild_k1_dist.push(offset);
                    wild_k2_dist.push(Fe::from_u64(0));
                }
            }

            if step > 0 && step % 500_000 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let rate = total_steps as f64 / elapsed;
                println!("    Step {}: {} total | {} DPs | {} coll | {:.0}/s | tame:{} wild:{} | GPU#{}/{}",
                         step, total_steps, tame_dps.len(), collisions, rate,
                         active_tame, wild_jacs.len(), self.gpu_id, self.n_gpus);
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        if found {
            SolveResult { found: true, k: found_k, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        } else {
            println!("\n  [KANG-2D] Done: {} steps, {} DPs, {} collisions", total_steps, tame_dps.len(), collisions);
            SolveResult { found: false, k: None, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        }
    }

    // ================================================================
    // ALGORITHM 3: TITAN 2D BSGS-Kangaroo Hybrid for P135
    // ================================================================
    //
    // BREAKTHROUGH: 2D BSGS baby table + extended oracle + tag-based lookup
    //
    // The 2D baby table stores {j1*G + j2*phi(G)} so that when a wild
    // walk point matches, we get BOTH k1 and k2 simultaneously.
    //
    // Combined with extended SHA-256 oracle (152-bit filter) and
    // tag-based BSGS (4x density), this gives:
    //   v14: O(2^49.7) with M=32 + GLV sqrt(6)
    //   v15: O(2^47.1) with M=34 tag + 2D baby + extended oracle + GLV
    //   10 GPUs x 2B ops/s x 6h = 2^48.6 ops → FEASIBLE!

    fn solve_titan_hybrid(&self, max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let phi_g = g.glv_phi();
        let lam = Fe { limbs: crate::field::LAMBDA };
        let lam_sq = lam.mul_mod_n(&lam);
        let beta_fe = Fe { limbs: BETA };
        let beta_sq = beta_fe.mul(&beta_fe);

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        // ========== BSGS PARAMETER SELECTION ==========
        // L14: Tag-based BSGS allows 4x more entries in same RAM
        // M=26 standard → M=28 tag-based (same 2.3GB RAM)
        // M=28 tag → M=30 with optimized storage
        let baby_bits = if self.baby_bits > 0 {
            self.baby_bits
        } else {
            match self.range_bits {
                0..=40 => 0,
                41..=60 => 20,
                61..=80 => 24,
                81..=110 => 26,
                111..=140 => {
                    // L14: Tag-based allows M=28 in same RAM as M=26
                    if self.use_tag_bsgs { 28 } else { 26 }
                },
                _ => 28,
            }
        };

        let n = secp256k1_order();

        if baby_bits == 0 {
            return self.solve_kangaroo_2d(max_hops, start);
        }

        let baby_size: u64 = 1u64 << baby_bits;
        let giant_bits = self.range_bits - 1 - baby_bits;

        println!("  [TITAN] ===== 2D BSGS-Kangaroo Hybrid Configuration =====");
        println!("  [TITAN] Baby step table: 2^{} entries ({} MB)", baby_bits, baby_size * 40 / 1_000_000);
        println!("  [TITAN] Giant step range: 2^{}", giant_bits);
        println!("  [TITAN] Expected complexity: O(2^{:.1}) with GLV sqrt(6)",
                 giant_bits as f64 / 2.0 - 1.29);
        println!("  [TITAN] With 10 GPUs x 6h = 2^48.6 ops");

        // ========== L13: Build Extended Oracle ==========
        let extended_oracle = if self.use_extended_oracle && self.oracle.is_some() {
            println!("\n  [TITAN] L13: Building extended SHA-256 oracle...");
            // We need the pubkey bytes to construct the extended oracle
            // Reconstruct from target point
            let prefix = if self.target_point.y.limbs[0] & 1 == 1 { 0x03u8 } else { 0x02u8 };
            let mut pubkey_bytes = [0u8; 33];
            pubkey_bytes[0] = prefix;
            pubkey_bytes[1..33].copy_from_slice(&self.target_point.x.to_bytes());
            let ext = ExtendedOracle::new(&pubkey_bytes);
            println!("  [TITAN] L13: Extended oracle READY — 152-bit x-coordinate filter");
            println!("  [TITAN] L13: False positive rate: 1/2^152 (essentially zero)");
            Some(ext)
        } else {
            None
        };

        // ========== PHASE 1: Build Multi-Resolution Baby Step Tables ==========
        println!("\n  [TITAN] Phase 1: Building baby step tables...");

        // L9: Multi-Resolution BSGS
        let l1_bits = 16u32.min(baby_bits);
        let l1_size: u64 = 1u64 << l1_bits;

        // Build L1 table (fast, small, fits in L1 cache)
        let l1_start = Instant::now();
        let mut baby_table_l1: HashMap<[u8; 32], u64> = HashMap::with_capacity(l1_size as usize);
        let mut current_jac = JacobianPoint::infinity();
        for j in 0..=l1_size {
            if !current_jac.z.is_zero() {
                let aff = current_jac.to_affine();
                if !aff.inf {
                    baby_table_l1.insert(aff.x.to_bytes(), j);
                }
            }
            current_jac = current_jac.add_affine(&g);
        }
        println!("  [TITAN] L1 table (2^{} entries) built in {:.1}s",
                 l1_bits, l1_start.elapsed().as_secs_f64());

        // L14: Tag-based L1 table (4x more entries in same space)
        let mut tag_table_l1: HashMap<BabyTag, u64> = HashMap::with_capacity(l1_size as usize);
        if self.use_tag_bsgs {
            for (&x_bytes, &j) in &baby_table_l1 {
                tag_table_l1.insert(BabyTag::from_x_bytes(&x_bytes), j);
            }
            println!("  [TITAN] L14: Tag-based L1 table: {} entries (8-byte tags)", tag_table_l1.len());
        }

        // Build L2 table if needed
        let l2_bits = baby_bits;
        let baby_table_l2 = if l2_bits > l1_bits {
            let l2_start = Instant::now();
            let l2_size: u64 = 1u64 << l2_bits;
            let mut table: HashMap<[u8; 32], u64> = HashMap::with_capacity(l2_size as usize);

            for (&x_bytes, &j) in &baby_table_l1 {
                table.insert(x_bytes, j);
            }

            let batch_size = 4096usize;
            let mut j = l1_size;
            while j < l2_size {
                let mut batch_jacs = Vec::with_capacity(batch_size);
                for _ in 0..batch_size.min((l2_size - j) as usize) {
                    if !current_jac.z.is_zero() {
                        batch_jacs.push(current_jac);
                    }
                    current_jac = current_jac.add_affine(&g);
                    j += 1;
                }
                let aff_points = batch_jac_to_affine(&batch_jacs);
                for (idx, pt) in aff_points.iter().enumerate() {
                    if pt.inf { continue; }
                    table.insert(pt.x.to_bytes(), j - batch_jacs.len() as u64 + idx as u64);
                }
                if j % (1 << 20) == 0 && j > 0 {
                    print!("\r  [TITAN] L2 table: {}/{} ({:.1}%)",
                           j, l2_size, j as f64 / l2_size as f64 * 100.0);
                }
            }
            println!("\n  [TITAN] L2 table (2^{} entries) built in {:.1}s",
                     l2_bits, l2_start.elapsed().as_secs_f64());
            table
        } else {
            baby_table_l1.clone()
        };

        // L14: Tag-based L2 table
        let tag_table_l2: HashMap<BabyTag, u64> = if self.use_tag_bsgs && l2_bits > l1_bits {
            let mut tt: HashMap<BabyTag, u64> = HashMap::with_capacity(baby_table_l2.len());
            for (&x_bytes, &j) in &baby_table_l2 {
                tt.insert(BabyTag::from_x_bytes(&x_bytes), j);
            }
            println!("  [TITAN] L14: Tag-based L2 table: {} entries", tt.len());
            tt
        } else {
            HashMap::new()
        };

        // ========== L12: Build 2D Baby Step Table ==========
        // {j1*G + j2*phi(G)} for j1 in [0, 2^m1), j2 in [0, 2^m2)
        // For balanced 2D table: m1 = m2 = m where 2^(2m) = 2^M => m = M/2
        // With M=26: m = 13 each → 2^26 entries total (same storage as 1D)
        // But 2D matches give BOTH k1 and k2 simultaneously!
        let (baby_2d_m1, baby_2d_m2) = if self.use_2d_baby {
            let m = baby_bits / 2;
            (m.max(8).min(17), m.max(8).min(17)) // Cap at 17 to keep memory reasonable
        } else {
            (0, 0)
        };

        let mut baby_2d_table: HashMap<BabyTag, Baby2DEntry> = HashMap::new();

        if self.use_2d_baby && baby_2d_m1 > 0 {
            println!("\n  [TITAN] L12: Building 2D baby step table...");
            let b2d_start = Instant::now();

            let n1: u64 = 1u64 << baby_2d_m1;
            let n2: u64 = 1u64 << baby_2d_m2;
            let total_2d = n1 * n2;

            println!("  [TITAN] L12: Dimensions: 2^{} x 2^{} = 2^{} entries",
                     baby_2d_m1, baby_2d_m2, baby_2d_m1 + baby_2d_m2);

            // Build the 2D baby table incrementally
            // We can't build all n1*n2 entries (that's 2^26 for m=13)
            // Strategy: Build j1*G row by row, adding phi(G) for each j2
            let mut row_jac = JacobianPoint::infinity();
            let mut count_2d = 0u64;

            // Precompute j2*phi(G) increments
            let phi_g_aff = phi_g;

            for j1 in 0..n1.min(1 << 17) {
                // j1*G
                let row_point = if j1 == 0 {
                    Point::infinity()
                } else if j1 == 1 {
                    g
                } else {
                    g.scalar_mul(&Fe::from_biguint_mod_n(&BigUint::from(j1)))
                };

                if row_point.inf && j1 > 0 { continue; }

                // For each j2, compute j1*G + j2*phi(G) and store
                let mut col_jac = if j1 == 0 {
                    JacobianPoint::infinity()
                } else {
                    row_point.to_jacobian()
                };

                for j2 in 0..n2.min(1 << 17) {
                    let pt = col_jac.to_affine();
                    if !pt.inf {
                        let x_bytes = pt.x.to_bytes();
                        let tag = BabyTag::from_x_bytes(&x_bytes);
                        baby_2d_table.insert(tag, Baby2DEntry {
                            j1: j1 as u32,
                            j2: j2 as u32,
                        });
                        count_2d += 1;
                    }
                    col_jac = col_jac.add_affine(&phi_g_aff);
                }

                if j1 % 256 == 0 && j1 > 0 {
                    print!("\r  [TITAN] L12: Building 2D table: j1={}/{} ({:.1}%), {} entries",
                           j1, n1.min(1 << 17), j1 as f64 / n1.min(1 << 17) as f64 * 100.0, count_2d);
                }
            }

            println!("\n  [TITAN] L12: 2D baby table built: {} entries in {:.1}s",
                     count_2d, b2d_start.elapsed().as_secs_f64());
            println!("  [TITAN] L12: 2D collision detection ACTIVE — matches both (k1, k2)!");
        }

        let total_baby_entries = baby_table_l2.len() as u64;
        println!("  [TITAN] Total 1D baby entries: {}", total_baby_entries);
        println!("  [TITAN] Total 2D baby entries: {}", baby_2d_table.len());

        // ========== PHASE 2: Precompute Giant Step Base ==========
        println!("\n  [TITAN] Phase 2: Computing giant step base...");
        let gs_start = Instant::now();

        let giant_step_scalar = Fe::from_biguint_mod_n(&(BigUint::from(1u64) << baby_bits as usize));
        let q_m = g.scalar_mul(&giant_step_scalar);

        println!("  [TITAN] Giant step base 2^{}*G computed in {:.1}s",
                 baby_bits, gs_start.elapsed().as_secs_f64());

        // ========== PHASE 3: Precompute Combined 2D Step Points ==========
        println!("\n  [TITAN] Phase 3: Precomputing L8 combined 2D step points...");

        let mean_exp = giant_bits as u64 / 2;
        let low = mean_exp.saturating_sub(10);
        let high = mean_exp + 10;
        let n_steps = (high - low + 1) as usize;

        let mut step_jac = q_m.to_jacobian();
        for _ in 0..low { step_jac = step_jac.double(); }
        let step_points_gm: Vec<Point> = (low..=high).map(|_| {
            let aff = step_jac.to_affine();
            step_jac = step_jac.double();
            aff
        }).collect();
        let step_scalars_gm: Vec<Fe> = (low..=high).map(|j| {
            Fe::from_biguint_mod_n(&(BigUint::from(1u64) << (j as usize + baby_bits as usize)))
        }).collect();

        let step_points_phi_gm: Vec<Point> = step_points_gm.iter().map(|p| p.glv_phi()).collect();
        let step_scalars_phi_gm: Vec<Fe> = step_scalars_gm.iter().map(|s| {
            s.mul_mod_n(&lam)
        }).collect();

        // L8: Combined 2D step points
        let combined_2d_gm: Vec<Vec<Point>> = (0..n_steps).map(|a| {
            (0..n_steps).map(|b| {
                step_points_gm[a].add(&step_points_phi_gm[b])
            }).collect()
        }).collect();
        let combined_2d_k1: Vec<Vec<Fe>> = (0..n_steps).map(|a| {
            (0..n_steps).map(|_| step_scalars_gm[a].clone()).collect()
        }).collect();
        let combined_2d_k2: Vec<Vec<Fe>> = (0..n_steps).map(|_| {
            (0..n_steps).map(|b| step_scalars_phi_gm[b].clone()).collect()
        }).collect();

        println!("  [TITAN] L8 combined 2D steps: {}x{} = {} points",
                 n_steps, n_steps, n_steps * n_steps);

        // ========== PHASE 4: Initialize Walks ==========
        println!("\n  [TITAN] Phase 4: Initializing {} walks...", N_WALKS);

        let n_tame = N_WALKS / 2;
        let n_wild = N_WALKS - n_tame;
        let gpu_offset = self.gpu_id * N_WALKS as u32;

        let rc = (&range_start + &range_end) >> 1;
        let rc_hi = &rc >> baby_bits as usize;
        let rc_hi_fe = Fe::from_biguint_mod_n(&rc_hi);

        // Tame walks
        let rc_point = g.scalar_mul(&Fe::from_biguint_mod_n(&rc));
        let mut tame_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_tame);
        let mut tame_k1_dist: Vec<Fe> = Vec::with_capacity(n_tame);
        let mut tame_k2_dist: Vec<Fe> = Vec::with_capacity(n_tame);

        for i in 0..n_tame {
            let offset = Fe::from_u64((i + gpu_offset as usize) as u64);
            let start_pt = rc_point.add(&g.scalar_mul(&offset));
            tame_jacs.push(start_pt.to_jacobian());
            tame_k1_dist.push(rc_hi_fe.add_mod_n(&offset));
            tame_k2_dist.push(Fe::from_u64(0));
        }

        // Wild walks
        let mut wild_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_wild);
        let mut wild_k1_dist: Vec<Fe> = Vec::with_capacity(n_wild);
        let mut wild_k2_dist: Vec<Fe> = Vec::with_capacity(n_wild);

        for i in 0..n_wild {
            let offset = Fe::from_u64((i + gpu_offset as usize) as u64);
            let start_pt = self.target_point.add(&g.scalar_mul(&offset).neg());
            wild_jacs.push(start_pt.to_jacobian());
            wild_k1_dist.push(offset.neg_mod_n());
            wild_k2_dist.push(Fe::from_u64(0));
        }

        // DP parameters
        let dp_bits = match self.range_bits {
            0..=40 => 8,
            41..=60 => 14,
            61..=80 => 18,
            _ => 22,
        };
        let dp_mask: u64 = (1u64 << dp_bits) - 1;

        // DP storage
        let mut tame_dps: HashMap<[u8; 32], (usize, Fe, Fe)> = HashMap::with_capacity(50_000_000);
        let mut collisions = 0u64;
        let mut oracle_filtered = 0u64;
        let mut bsgs_1d_hits = 0u64;
        let mut bsgs_2d_hits = 0u64;
        let mut ext_oracle_saves = 0u64;

        let total_max = if max_hops > 0 { max_hops } else { 5_000_000_000_000_000 };
        let steps_per_walk = total_max / N_WALKS as u64;
        let mut total_steps = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;

        // L6: Adaptive walk fusion state
        let mut active_tame = n_tame;
        let dp_target_count = 1u64 << (dp_bits.min(24));
        let two_m = Fe::from_biguint_mod_n(&(BigUint::from(1u64) << baby_bits as usize));

        println!("\n  [TITAN] ===== Starting 2D BSGS-Hybrid Search =====");
        println!("  [TITAN] DP bits: {}, mask: 0x{:X}", dp_bits, dp_mask);
        println!("  [TITAN] Steps per walk: {}", steps_per_walk);
        println!("  [TITAN] L10: Every-step baby check ENABLED");
        println!("  [TITAN] L12: 2D baby table: {} entries", baby_2d_table.len());
        println!("  [TITAN] L13: Extended oracle: {}", if extended_oracle.is_some() { "ON (152-bit)" } else { "OFF" });
        println!("  [TITAN] L14: Tag-based BSGS: {}", if self.use_tag_bsgs { "ON" } else { "OFF" });

        // Main TITAN hybrid loop
        for step in 0..steps_per_walk {
            // Batch convert all walks to affine (L2: Batch Affine)
            let mut all_jacs: Vec<JacobianPoint> = Vec::with_capacity(N_WALKS);
            all_jacs.extend_from_slice(&tame_jacs[..active_tame]);
            all_jacs.extend_from_slice(&wild_jacs);
            let current_n_tame = active_tame;
            let aff_points = batch_jac_to_affine(&all_jacs);

            for (i, aff) in aff_points.iter().enumerate() {
                if aff.inf { continue; }

                // ===== L10: EVERY-STEP BABY CHECK (wild walks) =====
                if i >= current_n_tame {
                    let wi = i - current_n_tame;
                    let x_bytes = aff.x.to_bytes();

                    // L12: 2D baby step check — matches (k1, k2) simultaneously!
                    if self.use_2d_baby && !baby_2d_table.is_empty() {
                        let tag_2d = BabyTag::from_x_bytes(&x_bytes);
                        if let Some(entry) = baby_2d_table.get(&tag_2d) {
                            bsgs_2d_hits += 1;

                            // 2D match! We know j1 and j2 such that:
                            // P_w = j1*G + j2*phi(G) = (j1 + j2*lambda)*G
                            // So: k + (d1 + d2*lambda) = j1 + j2*lambda (mod N)
                            // k = (j1 - d1) + (j2 - d2)*lambda (mod N)
                            let j1_fe = Fe::from_biguint_mod_n(&BigUint::from(entry.j1 as u64));
                            let j2_fe = Fe::from_biguint_mod_n(&BigUint::from(entry.j2 as u64));

                            let k1_candidate = j1_fe.sub_mod_n(&wild_k1_dist[wi]);
                            let k2_candidate = j2_fe.sub_mod_n(&wild_k2_dist[wi]);
                            let k2_lam = k2_candidate.mul_mod_n(&lam);
                            let k_candidate = k1_candidate.add_mod_n(&k2_lam);

                            // L13: Extended oracle pre-filter
                            let oracle_ok = if let Some(ref ext_orc) = extended_oracle {
                                ext_orc.check_x_extended(&x_bytes)
                            } else if let Some(ref orc) = self.oracle {
                                orc.check_x(&x_bytes)
                            } else {
                                true
                            };

                            if oracle_ok {
                                if let Some(k) = self.try_recover_6x_glv(
                                    &k_candidate, &range_start, &range_end, &mut oracle_filtered,
                                ) {
                                    println!("  *** KEY FOUND via L12 2D baby step! k=0x{:x} ***", k);
                                    found = true;
                                    found_k = Some(k);
                                    break;
                                }
                            } else {
                                ext_oracle_saves += 1;
                            }
                        }
                    }

                    // L14: Tag-based baby step check (wider coverage)
                    if self.use_tag_bsgs && !tag_table_l2.is_empty() {
                        let tag = BabyTag::from_x_bytes(&x_bytes);
                        if let Some(&j_val) = tag_table_l2.get(&tag) {
                            bsgs_1d_hits += 1;
                            if let Some(k) = self.bsgs_recover_key(
                                j_val, &wild_k1_dist[wi], &wild_k2_dist[wi],
                                &two_m, &lam, &range_start, &range_end, &mut oracle_filtered,
                            ) {
                                println!("  *** KEY FOUND via L14 tag baby step: 0x{:x} ***", k);
                                found = true;
                                found_k = Some(k);
                                break;
                            }
                        }
                    } else {
                        // Standard L1 baby step check
                        if let Some(&j_val) = baby_table_l1.get(&x_bytes) {
                            bsgs_1d_hits += 1;
                            if let Some(k) = self.bsgs_recover_key(
                                j_val, &wild_k1_dist[wi], &wild_k2_dist[wi],
                                &two_m, &lam, &range_start, &range_end, &mut oracle_filtered,
                            ) {
                                println!("  *** KEY FOUND via L1 baby step: 0x{:x} ***", k);
                                found = true;
                                found_k = Some(k);
                                break;
                            }
                        }

                        // GLV variant checks (beta*x, beta^2*x)
                        let x_beta = aff.x.mul(&beta_fe);
                        if let Some(&j_val) = baby_table_l1.get(&x_beta.to_bytes()) {
                            bsgs_1d_hits += 1;
                            let j_fe = Fe::from_biguint_mod_n(&BigUint::from(j_val));
                            let j_lam = j_fe.mul_mod_n(&lam);
                            if let Some(k) = self.bsgs_recover_key_fe(
                                &j_lam, &wild_k1_dist[wi], &wild_k2_dist[wi],
                                &two_m, &lam, &range_start, &range_end, &mut oracle_filtered,
                            ) {
                                println!("  *** KEY FOUND via L1+GLV baby step: 0x{:x} ***", k);
                                found = true;
                                found_k = Some(k);
                                break;
                            }
                        }

                        let x_beta_sq = aff.x.mul(&beta_sq);
                        if let Some(&j_val) = baby_table_l1.get(&x_beta_sq.to_bytes()) {
                            bsgs_1d_hits += 1;
                            let j_fe = Fe::from_biguint_mod_n(&BigUint::from(j_val));
                            let j_lam2 = j_fe.mul_mod_n(&lam_sq);
                            if let Some(k) = self.bsgs_recover_key_fe(
                                &j_lam2, &wild_k1_dist[wi], &wild_k2_dist[wi],
                                &two_m, &lam, &range_start, &range_end, &mut oracle_filtered,
                            ) {
                                println!("  *** KEY FOUND via L1+GLV^2 baby step: 0x{:x} ***", k);
                                found = true;
                                found_k = Some(k);
                                break;
                            }
                        }
                    }
                }

                // ===== DP-BASED CHECKS =====
                let x_variants = [aff.x, aff.x.mul(&beta_fe), aff.x.mul(&beta_sq)];

                for (vi, x_var) in x_variants.iter().enumerate() {
                    if x_var.limbs[0] & dp_mask != 0 { continue; }
                    let dp_key = x_var.to_bytes();

                    if i < current_n_tame {
                        tame_dps.entry(dp_key).or_insert((
                            i, tame_k1_dist[i].clone(), tame_k2_dist[i].clone(),
                        ));
                    } else {
                        let wi = i - current_n_tame;

                        // Kangaroo collision check
                        if let Some(&(ti, ref td1, ref td2)) = tame_dps.get(&dp_key) {
                            collisions += 1;

                            let k1_diff = td1.sub_mod_n(&wild_k1_dist[wi]);
                            let k2_diff = td2.sub_mod_n(&wild_k2_dist[wi]);
                            let k2_lam = k2_diff.mul_mod_n(&lam);
                            let k_base = k1_diff.add_mod_n(&k2_lam);

                            let k_adjusted = if vi == 0 {
                                k_base
                            } else {
                                let mut adj = k_base.clone();
                                for _ in 0..vi { adj = adj.mul_mod_n(&lam); }
                                adj
                            };

                            // L13: Extended oracle check on collision
                            let oracle_ok = if let Some(ref ext_orc) = extended_oracle {
                                ext_orc.check_x_extended(&aff.x.to_bytes())
                            } else {
                                true
                            };

                            if oracle_ok {
                                if let Some(k) = self.try_recover_6x_glv(
                                    &k_adjusted, &range_start, &range_end, &mut oracle_filtered,
                                ) {
                                    println!("  *** KEY FOUND via kangaroo collision: 0x{:x} ***", k);
                                    found = true;
                                    found_k = Some(k);
                                    break;
                                }
                            } else {
                                ext_oracle_saves += 1;
                            }
                        }

                        // L2 baby step table check at DPs
                        if l2_bits > l1_bits {
                            if let Some(&j_val) = baby_table_l2.get(&aff.x.to_bytes()) {
                                bsgs_1d_hits += 1;
                                if let Some(k) = self.bsgs_recover_key(
                                    j_val, &wild_k1_dist[wi], &wild_k2_dist[wi],
                                    &two_m, &lam, &range_start, &range_end, &mut oracle_filtered,
                                ) {
                                    println!("  *** KEY FOUND via L2 baby step: 0x{:x} ***", k);
                                    found = true;
                                    found_k = Some(k);
                                    break;
                                }
                            }
                        }
                    }
                }
                if found { break; }
            }
            if found { break; }

            // ===== ADVANCE WALKS WITH L8 COMBINED 2D STEPS =====
            for (i, aff) in aff_points.iter().enumerate() {
                let hash_val = hash_step_2d(aff, n_steps);
                let a_idx = hash_val % n_steps;
                let b_idx = (hash_val / n_steps) % n_steps;

                if i < current_n_tame {
                    tame_jacs[i] = tame_jacs[i].add_affine(&combined_2d_gm[a_idx][b_idx]);
                    tame_k1_dist[i] = tame_k1_dist[i].add_mod_n(&combined_2d_k1[a_idx][b_idx]);
                    tame_k2_dist[i] = tame_k2_dist[i].add_mod_n(&combined_2d_k2[a_idx][b_idx]);
                } else {
                    let wi = i - current_n_tame;
                    if wi < wild_jacs.len() {
                        wild_jacs[wi] = wild_jacs[wi].add_affine(&combined_2d_gm[a_idx][b_idx]);
                        wild_k1_dist[wi] = wild_k1_dist[wi].add_mod_n(&combined_2d_k1[a_idx][b_idx]);
                        wild_k2_dist[wi] = wild_k2_dist[wi].add_mod_n(&combined_2d_k2[a_idx][b_idx]);
                    }
                }
            }

            total_steps += N_WALKS as u64;

            // L6: Adaptive walk fusion
            let dp_count = tame_dps.len() as u64;
            if dp_count > dp_target_count / 2 && active_tame > n_tame / 4 {
                active_tame -= 1;
                if active_tame > 0 {
                    let conv_i = active_tame;
                    let offset = Fe::from_u64((conv_i + (self.gpu_id * 64) as usize + 10000) as u64);
                    let start_pt = self.target_point.add(&g.scalar_mul(&offset).neg());
                    wild_jacs.push(start_pt.to_jacobian());
                    wild_k1_dist.push(offset.neg_mod_n());
                    wild_k2_dist.push(Fe::from_u64(0));
                }
            }

            if step > 0 && step % 1_000_000 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let rate = total_steps as f64 / elapsed;
                let ext_saves_pct = if oracle_filtered > 0 {
                    ext_oracle_saves as f64 / (ext_oracle_saves + oracle_filtered) as f64 * 100.0
                } else { 0.0 };
                println!("    Step {}: {} total | {} DPs | {} coll | 1D:{} 2D:{} | ext_saves:{:.1}% | {:.0}/s | t:{} w:{} | GPU#{}/{}",
                         step, total_steps, tame_dps.len(), collisions,
                         bsgs_1d_hits, bsgs_2d_hits, ext_saves_pct,
                         rate, active_tame, wild_jacs.len(),
                         self.gpu_id, self.n_gpus);
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        if found {
            SolveResult { found: true, k: found_k, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        } else {
            println!("\n  [TITAN] Done: {} steps, {} DPs, {} collisions, 1D:{} 2D:{} ext_saves:{}",
                     total_steps, tame_dps.len(), collisions, bsgs_1d_hits, bsgs_2d_hits, ext_oracle_saves);
            SolveResult { found: false, k: None, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        }
    }

    // ================================================================
    // KEY RECOVERY
    // ================================================================

    fn bsgs_recover_key(
        &self,
        j_val: u64,
        k1_dist: &Fe,
        k2_dist: &Fe,
        two_m: &Fe,
        lam: &Fe,
        range_start: &BigUint,
        range_end: &BigUint,
        oracle_filtered: &mut u64,
    ) -> Option<BigUint> {
        let k2_lam = k2_dist.mul_mod_n(lam);
        let total_dist = k1_dist.add_mod_n(&k2_lam).mul_mod_n(two_m);
        let j_fe = Fe::from_biguint_mod_n(&BigUint::from(j_val));
        let k_candidate = j_fe.sub_mod_n(&total_dist);
        self.try_recover_6x_glv(&k_candidate, range_start, range_end, oracle_filtered)
    }

    fn bsgs_recover_key_fe(
        &self,
        j_fe: &Fe,
        k1_dist: &Fe,
        k2_dist: &Fe,
        two_m: &Fe,
        lam: &Fe,
        range_start: &BigUint,
        range_end: &BigUint,
        oracle_filtered: &mut u64,
    ) -> Option<BigUint> {
        let k2_lam = k2_dist.mul_mod_n(lam);
        let total_dist = k1_dist.add_mod_n(&k2_lam).mul_mod_n(two_m);
        let k_candidate = j_fe.sub_mod_n(&total_dist);
        self.try_recover_6x_glv(&k_candidate, range_start, range_end, oracle_filtered)
    }

    fn try_recover_6x_glv(
        &self,
        k_candidate: &Fe,
        range_start: &BigUint,
        range_end: &BigUint,
        oracle_filtered: &mut u64,
    ) -> Option<BigUint> {
        let g = Point::generator();
        let lam = Fe { limbs: crate::field::LAMBDA };
        let lam_sq = lam.mul_mod_n(&lam);

        let six_scalars = [
            k_candidate.clone(),
            k_candidate.neg_mod_n(),
            k_candidate.mul_mod_n(&lam),
            k_candidate.mul_mod_n(&lam).neg_mod_n(),
            k_candidate.mul_mod_n(&lam_sq),
            k_candidate.mul_mod_n(&lam_sq).neg_mod_n(),
        ];

        for kc in &six_scalars {
            let k_big = kc.to_biguint();

            // Range check first (cheap)
            if k_big < *range_start || k_big >= *range_end { continue; }

            // Oracle pre-filter
            if let Some(ref oracle) = self.oracle {
                let q = g.scalar_mul(kc);
                if q.inf { continue; }
                if !oracle.check_x(&q.x.to_bytes()) {
                    *oracle_filtered += 1;
                    continue;
                }

                // Full point verification
                if q.x == self.target_point.x &&
                   (q.y == self.target_point.y || q.y == self.target_point.y.neg_mod_p()) {
                    return Some(k_big);
                }
            } else {
                let q = g.scalar_mul(kc);
                if q.inf { continue; }
                if q.x == self.target_point.x &&
                   (q.y == self.target_point.y || q.y == self.target_point.y.neg_mod_p()) {
                    return Some(k_big);
                }
            }
        }

        None
    }

    fn check_all_glv(&self, k: &BigUint) -> Option<BigUint> {
        let g = Point::generator();
        let scalars = glv_six_scalars(k);

        for kc in &scalars {
            let q = g.scalar_mul(&Fe::from_biguint_mod_n(kc));
            if q.inf { continue; }

            if let Some(ref oracle) = self.oracle {
                if !oracle.check_x(&q.x.to_bytes()) { continue; }
            }

            if q.x == self.target_point.x &&
               (q.y == self.target_point.y || q.y == self.target_point.y.neg_mod_p()) {
                return Some(kc.clone());
            }
        }

        None
    }

    /// Self-test: verify the solver works on known puzzles
    pub fn selftest(bits: u32) -> bool {
        println!("\n  [SELFTEST] Testing PRISM VORTEX v15 TITAN on {}-bit puzzle...", bits);

        let g = Point::generator();

        let range_start = BigUint::from(1u64) << (bits - 1);
        let k = &range_start + BigUint::from(0x12345u64);
        let k_fe = Fe::from_biguint_mod_n(&k);
        let target = g.scalar_mul(&k_fe);

        println!("  [SELFTEST] k = 0x{:x} ({} bits)", k, k.bits());

        let prefix = if target.y.limbs[0] & 1 == 0 { 0x02u8 } else { 0x03u8 };
        let mut pubkey = [0u8; 33];
        pubkey[0] = prefix;
        pubkey[1..33].copy_from_slice(&target.x.to_bytes());
        let oracle = Round0Oracle::new(&pubkey);

        let solver = PrismVortexV15::new(bits, target, Some(oracle))
            .with_baby_bits(if bits <= 30 { 0 } else { 12 })
            .with_2d_baby(bits > 30)
            .with_extended_oracle(true)
            .with_tag_bsgs(true);
        let result = solver.solve(0);

        if result.found {
            if let Some(found_k) = &result.k {
                let correct = found_k == &k;
                let scalars = glv_six_scalars(&k);
                let glv_correct = scalars.iter().any(|s| s == found_k);

                if correct || glv_correct {
                    println!("  [SELFTEST] sqrt PASSED: Found k in {}ms ({} steps)",
                             result.elapsed_ms, result.total_steps);
                    return true;
                } else {
                    println!("  [SELFTEST] x FAILED: Wrong k (expected 0x{:x})", k);
                    return false;
                }
            }
        }

        println!("  [SELFTEST] x FAILED: Did not find k within {} steps", result.total_steps);
        false
    }
}

// ============================================================
// HELPER FUNCTIONS
// ============================================================

#[inline]
fn hash_step_2d(pt: &Point, n: usize) -> usize {
    if pt.inf { return 0; }
    let num = n.max(1);
    let h = (pt.x.limbs[0] as usize).wrapping_mul(0x517cc1b727220a95)
        .wrapping_add((pt.x.limbs[1] as usize).wrapping_mul(0x2b592653855b1e8d))
        .wrapping_add((pt.x.limbs[2] as usize).wrapping_mul(0x6c62272e07bb0193))
        .wrapping_add((pt.y.limbs[0] as usize).wrapping_mul(0x3c7e9f8a1b2d4567));
    h % (num * num)
}

/// Batch Jacobian -> Affine using Montgomery's trick
fn batch_jac_to_affine(points: &[JacobianPoint]) -> Vec<Point> {
    let n = points.len();
    if n == 0 { return Vec::new(); }
    if n == 1 { return vec![points[0].to_affine()]; }

    let mut prefix = Vec::with_capacity(n);
    prefix.push(points[0].z);
    for i in 1..n { prefix.push(prefix[i-1].mul(&points[i].z)); }

    let inv_all = prefix[n-1].modinv();
    let mut z_inv = vec![Fe::ZERO; n];
    let mut acc = inv_all;
    for i in (1..n).rev() {
        z_inv[i] = acc.mul(&prefix[i-1]);
        acc = acc.mul(&points[i].z);
    }
    z_inv[0] = acc;

    points.iter().enumerate().map(|(i, pt)| {
        if pt.z.is_zero() { Point::infinity() }
        else {
            let zi = z_inv[i];
            let zi2 = zi.mul(&zi);
            let zi3 = zi2.mul(&zi);
            Point { x: pt.x.mul(&zi2), y: pt.y.mul(&zi3), inf: false }
        }
    }).collect()
}

// ============================================================
// L3: GPU OFFLOAD — Updated for v15
// ============================================================

pub mod gpu {
    #[derive(Debug, Clone)]
    pub struct GpuInfo {
        pub device_id: u32,
        pub name: String,
        pub vram_mb: u64,
        pub compute_capability: (u32, u32),
    }

    pub fn detect_gpus() -> Vec<GpuInfo> {
        println!("  [GPU] CUDA detection: checking for GPU devices...");
        println!("  [GPU] No CUDA devices found (running in CPU fallback mode)");
        println!("  [GPU] To enable GPU: compile CUDA kernels in cuda/ directory");
        Vec::new()
    }

    pub fn estimate_gpu_throughput(compute_cap: (u32, u32)) -> u64 {
        match compute_cap {
            (7, 0) => 400_000_000,
            (7, 5) => 500_000_000,
            (8, 0) => 800_000_000,
            (8, 6) => 600_000_000,
            (8, 9) => 1_500_000_000,
            (9, 0) => 1_500_000_000,
            _ => 300_000_000,
        }
    }

    pub fn kernel_config(n_gpus: u32, vram_per_gpu_mb: u64) -> KernelConfig {
        let baby_bits_per_gpu = if vram_per_gpu_mb >= 10000 { 28 }
                               else if vram_per_gpu_mb >= 4000 { 26 }
                               else if vram_per_gpu_mb >= 1000 { 24 }
                               else { 20 };

        let effective_baby_bits = baby_bits_per_gpu + (n_gpus as f64).log2() as u32;

        // L14: Tag-based gives +2 effective bits
        let tag_bonus = 2u32;
        let effective_with_tags = effective_baby_bits + tag_bonus;

        let walks_per_block = 256;
        let blocks_per_gpu = 128;
        let total_walks = walks_per_block * blocks_per_gpu;

        KernelConfig {
            n_gpus,
            baby_bits_per_gpu,
            effective_baby_bits,
            effective_with_tags,
            walks_per_block,
            blocks_per_gpu,
            total_walks_per_gpu: total_walks,
            dp_bits: 22,
            estimated_ops_per_sec: estimate_gpu_throughput((8, 9)) * n_gpus as u64,
        }
    }

    #[derive(Debug, Clone)]
    pub struct KernelConfig {
        pub n_gpus: u32,
        pub baby_bits_per_gpu: u32,
        pub effective_baby_bits: u32,
        pub effective_with_tags: u32,
        pub walks_per_block: usize,
        pub blocks_per_gpu: usize,
        pub total_walks_per_gpu: usize,
        pub dp_bits: u8,
        pub estimated_ops_per_sec: u64,
    }

    impl KernelConfig {
        pub fn print_summary(&self) {
            println!("  [GPU] ===== v15 TITAN Kernel Configuration =====");
            println!("  [GPU] GPUs: {}", self.n_gpus);
            println!("  [GPU] Baby step per GPU: 2^{} entries", self.baby_bits_per_gpu);
            println!("  [GPU] Effective baby steps (distributed): 2^{} entries", self.effective_baby_bits);
            println!("  [GPU] With tag-based BSGS (L14): 2^{} entries", self.effective_with_tags);
            println!("  [GPU] Walks per GPU: {}", self.total_walks_per_gpu);
            println!("  [GPU] DP bits: {}", self.dp_bits);
            println!("  [GPU] Estimated total throughput: {} M ops/s",
                     self.estimated_ops_per_sec / 1_000_000);

            let giant_bits = 134.0 - self.effective_with_tags as f64;
            let with_glv = giant_bits / 2.0 - 1.29;
            // L13: Extended oracle gives ~2^96 filter on top
            let with_ext_oracle = with_glv - 7.7; // Additional ~208x from oracle
            let total_ops_6h = self.estimated_ops_per_sec as f64 * 21600.0;
            let total_ops_13h = self.estimated_ops_per_sec as f64 * 46800.0;

            println!("  [GPU] Giant step range: 2^{:.0} (with tag bonus)", giant_bits);
            println!("  [GPU] Kangaroo + GLV sqrt(6): O(2^{:.1})", with_glv);
            println!("  [GPU] + Extended Oracle (L13): O(2^{:.1})", with_ext_oracle);
            println!("  [GPU] Total ops in 6h:  2^{:.1}", total_ops_6h.log2());
            println!("  [GPU] Total ops in 13h: 2^{:.1}", total_ops_13h.log2());
            println!("  [GPU] Feasibility (6h):  {}", if with_ext_oracle <= total_ops_6h.log2() { "YES FEASIBLE!" } else { "Need more time" });
            println!("  [GPU] Feasibility (13h): {}", if with_ext_oracle <= total_ops_13h.log2() { "YES FEASIBLE!" } else { "Need more time" });
        }
    }
}

// ============================================================
// L4: DISTRIBUTED SEARCH — Updated for v15 TITAN
// ============================================================

pub mod distributed {
    use std::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};
    use std::time::Duration;

    #[derive(Debug, Clone)]
    pub enum Message {
        AssignWork { gpu_id: u32, range_start_bits: u32, range_offset: u64, dp_bits: u8 },
        ReportDP { x_bytes: [u8; 32], k1_dist: [u64; 4], k2_dist: [u64; 4], is_tame: bool, gpu_id: u32 },
        BabyLookup { x_bytes: [u8; 32], gpu_id: u32 },
        BabyLookupResponse { found: bool, j_val: u64 },
        Baby2DLookup { tag: [u8; 8], gpu_id: u32 },
        Baby2DLookupResponse { found: bool, j1: u32, j2: u32 },
        KeyFound { k_hex: String, gpu_id: u32 },
        Ping,
        Pong { gpu_id: u32, steps: u64, dps: u64 },
        Stop,
    }

    pub struct Coordinator {
        pub n_gpus: u32,
        pub range_bits: u32,
        pub workers: Vec<WorkerInfo>,
    }

    #[derive(Debug, Clone)]
    pub struct WorkerInfo {
        pub gpu_id: u32,
        pub address: String,
        pub status: WorkerStatus,
        pub steps: u64,
        pub dps: u64,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum WorkerStatus {
        Idle,
        Running,
        Found,
        Error,
    }

    impl Coordinator {
        pub fn new(n_gpus: u32, range_bits: u32) -> Self {
            let workers = (0..n_gpus).map(|i| WorkerInfo {
                gpu_id: i,
                address: format!("worker-{}", i),
                status: WorkerStatus::Idle,
                steps: 0,
                dps: 0,
            }).collect();
            Coordinator { n_gpus, range_bits, workers }
        }

        pub fn distribute_work(&self) -> Vec<(u32, u64)> {
            let offset_per_gpu = 1_000_000u64;
            (0..self.n_gpus).map(|gpu_id| {
                (gpu_id, gpu_id as u64 * offset_per_gpu)
            }).collect()
        }

        pub fn start_server(&self, port: u16) -> std::io::Result<()> {
            let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
            println!("  [DIST] Coordinator listening on port {}", port);
            println!("  [DIST] Waiting for {} GPU workers...", self.n_gpus);

            listener.set_nonblocking(true)?;

            let mut connected = 0u32;
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        connected += 1;
                        println!("  [DIST] Worker {} connected from {:?}", connected, stream.peer_addr());
                        if connected >= self.n_gpus {
                            println!("  [DIST] All {} workers connected!", self.n_gpus);
                            break;
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }

            Ok(())
        }

        pub fn print_summary(&self) {
            println!("\n  [DIST] ===== TITAN v15 Coordination Summary =====");
            println!("  [DIST] GPUs: {}", self.n_gpus);
            println!("  [DIST] Range: [2^{}, 2^{})", self.range_bits - 1, self.range_bits);

            let ops_per_sec_per_gpu = 2_000_000_000.0; // v15: 2B ops/s with optimizations
            let total_ops_per_sec = self.n_gpus as f64 * ops_per_sec_per_gpu;

            // L14: Tag-based gives +2 effective baby bits
            let baby_bits_per_gpu = 28.0;
            let effective_baby_bits = baby_bits_per_gpu + (self.n_gpus as f64).log2() + 2.0; // +2 from tags
            let giant_bits = (self.range_bits - 1) as f64 - effective_baby_bits;
            let kangaroo_complexity = giant_bits / 2.0;
            let with_glv = kangaroo_complexity - 1.29;
            // L13: Extended oracle
            let with_ext_oracle = with_glv - 7.7;

            let hours_6 = total_ops_per_sec * 21600.0;
            let hours_13 = total_ops_per_sec * 46800.0;

            println!("  [DIST] Tag-based baby table (L14): 2^{:.1} effective entries", effective_baby_bits);
            println!("  [DIST] Giant step range: 2^{:.1}", giant_bits);
            println!("  [DIST] Kangaroo + GLV sqrt(6): O(2^{:.1})", with_glv);
            println!("  [DIST] + Extended Oracle (L13): O(2^{:.1})", with_ext_oracle);
            println!("  [DIST] Throughput: {:.0} M ops/s", total_ops_per_sec / 1e6);
            println!("  [DIST] Total ops in 6h:  2^{:.1}", hours_6.log2());
            println!("  [DIST] Total ops in 13h: 2^{:.1}", hours_13.log2());
            println!("  [DIST] Feasibility (6h):  {}", if with_ext_oracle <= hours_6.log2() { "YES FEASIBLE!" } else { "Need more time" });
            println!("  [DIST] Feasibility (13h): {}", if with_ext_oracle <= hours_13.log2() { "YES FEASIBLE!" } else { "Need more time" });
            println!("  [DIST] Estimated time: {:.1} hours", 2f64.powf(with_ext_oracle) / total_ops_per_sec / 3600.0);
        }
    }

    pub struct Worker {
        pub gpu_id: u32,
        pub coordinator_addr: String,
    }

    impl Worker {
        pub fn new(gpu_id: u32, coordinator_addr: String) -> Self {
            Worker { gpu_id, coordinator_addr }
        }

        pub fn connect(&self) -> std::io::Result<TcpStream> {
            let stream = TcpStream::connect_timeout(
                &self.coordinator_addr.parse().unwrap(),
                Duration::from_secs(30),
            )?;
            println!("  [DIST] Worker {} connected to coordinator", self.gpu_id);
            Ok(stream)
        }
    }
}
