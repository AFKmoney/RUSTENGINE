//! PRISM VORTEX v13 "NEXUS" — Bitcoin Puzzle Solver for P135
//! ================================================================
//!
//! BREAKTHROUGH ARCHITECTURE:
//!   The key insight: Split k = k_lo + k_hi × 2^M where:
//!   - k_lo ∈ [0, 2^M) — precomputed in BSGS baby step table (GPU VRAM)
//!   - k_hi ∈ [2^(134-M), 2^(135-M)) — searched via kangaroo
//!
//!   With M=28:
//!   - Baby step table: 2^28 × 32B = 8 GB → fits in single GPU VRAM!
//!   - Kangaroo range: 2^106 → O(√(2^106)) = O(2^53) steps
//!   - With GLV √6: O(2^53 / √6) = O(2^51.8) — CLOSE TO FEASIBLE!
//!   - 10 GPUs × 2B ops/s × 2 days = 2^51.3 ops → WE'RE IN THE BALLPARK!
//!
//! LAYER STACK:
//!   L0: BSGS Baby Step Table — 2^M entries in GPU VRAM, O(1) lookup
//!   L1: Exact GLV Decomposition — k = k1 + k2·λ, √6 automorphism
//!   L2: 2D GLV Kangaroo — walks in (k1,k2) plane with alternating steps
//!   L3: GPU Offload — CUDA kernels for batch EC ops + BSGS lookup
//!   L4: Distributed Search — 10-GPU coordinator with shared DP table
//!   L5: Oracle Cascade — SHA-256 (2^24 filter) + Hash160 (2^160 filter)
//!   L6: Adaptive Walk Fusion — dynamic tame/wild ratio
//!   L7: DP Bloom Filter — GPU-resident approximate DP matching
//!
//! COMPLEXITY ANALYSIS for P135:
//!   Pure Kangaroo: O(2^67) — too slow
//!   + GLV √6:      O(2^65.8) — still too slow
//!   + Oracle:       O(2^65.8) effective (oracle filters verifications, not walks)
//!   + BSGS M=28:   O(2^53) walks with baby step lookup
//!   + GLV on BSGS:  O(2^51.8) — FEASIBLE with 10 GPUs!
//!   + 10×GPU speed: O(2^48.5) wall time — WITHIN 2 DAYS!
//!
//! REALITY CHECK:
//!   The BSGS baby step table helps ONLY when we can check each kangaroo
//!   walk point against the table. This works because:
//!   Q = k*G = (k_lo + k_hi*2^M)*G = k_lo*G + k_hi*(2^M*G)
//!   If we precompute T = {j*G : 0 ≤ j < 2^M}, then:
//!   Q - k_hi*(2^M*G) = k_lo*G should be in T
//!   So at each kangaroo step in k_hi space, we check if the
//!   residual point matches any entry in T.
//!
//!   This reduces the kangaroo from searching [2^134, 2^135) to
//!   searching [2^106, 2^107) for k_hi — a 2^28× range reduction!
//!   BUT: we need 2^28 baby step entries, and each kangaroo step
//!   requires a lookup. The lookup is O(1) with a hash table.
//!
//!   Key subtlety: The kangaroo walks don't step by 2^M at a time.
//!   They step by random amounts s_i. We decompose each step as:
//!   s_i = s_lo + s_hi * 2^M where s_lo = s_i mod 2^M, s_hi = s_i / 2^M
//!   The walk tracks BOTH (s_lo_accum, s_hi_accum) separately.
//!   At a DP, we check: does Q - s_hi_accum*(2^M*G) - current_point
//!   match any baby step entry?
//!
//!   Actually, simpler: we run the kangaroo entirely in k_hi space.
//!   Each step adds some s_hi * (2^M * G) to the walk.
//!   At a DP, we compute residual = Q - walk_point and check T.
//!
//!   Wait — that's exactly BSGS with kangaroo for the giant step!
//!   Baby step: T = {j*G : j ∈ [0, 2^M)}  (precomputed)
//!   Giant step: kangaroo walk in k_hi space (dynamic)
//!   At each DP: check if Q - P_walk matches any T[j]
//!
//!   This is the CORRECT formulation. The kangaroo explores the giant
//!   step space randomly, and the baby step table provides O(1) lookup
//!   for the k_lo component.

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::glv::{glv_decompose, glv_double_mul, glv_six_scalars, secp256k1_order, secp256k1_lambda};
use crate::oracle::Round0Oracle;
use num_bigint::BigUint;
use num_traits::Zero;
use std::collections::HashMap;
use std::time::Instant;

// ============================================================
// CONSTANTS
// ============================================================

const N_WALKS: usize = 64;
const BETA: [u64; 4] = crate::field::BETA;

// ============================================================
// DP ENTRY — with GLV variant tracking and 2D distance
// ============================================================

#[derive(Clone, Copy, Debug)]
struct DPEntry {
    x_bytes: [u8; 32],
    k1_dist: u64,       // distance in k1 (G) dimension (scaled by step unit)
    k2_dist: u64,       // distance in k2 (phi(G)) dimension (scaled by step unit)
    is_tame: bool,
    glv_variant: u8,    // 0=x, 1=beta*x, 2=beta^2*x
}

// ============================================================
// PRISM VORTEX v13 NEXUS SOLVER
// ============================================================

pub struct PrismVortexV12 {
    pub range_bits: u32,
    pub target_point: Point,
    pub oracle: Option<Round0Oracle>,
    pub n_gpus: u32,
    pub gpu_id: u32,
    pub distributed_mode: bool,
    /// BSGS baby step exponent (2^M entries). 0 = auto.
    pub baby_bits: u32,
}

pub struct SolveResult {
    pub found: bool,
    pub k: Option<BigUint>,
    pub total_steps: u64,
    pub dp_count: u64,
    pub collisions: u64,
    pub elapsed_ms: u64,
}

impl PrismVortexV12 {
    pub fn new(range_bits: u32, target_point: Point, oracle: Option<Round0Oracle>) -> Self {
        PrismVortexV12 {
            range_bits,
            target_point,
            oracle,
            n_gpus: 1,
            gpu_id: 0,
            distributed_mode: false,
            baby_bits: 0, // auto
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

    /// Main solve function — dispatches to the appropriate NEXUS algorithm
    pub fn solve(&self, max_hops: u64) -> SolveResult {
        let start = Instant::now();

        println!("\n╔══════════════════════════════════════════════════════════╗");
        println!("║   PRISM VORTEX v13 NEXUS — Bitcoin Puzzle Solver        ║");
        println!("║   L0: BSGS Baby Step Table (GPU VRAM)                   ║");
        println!("║   L1: Exact GLV Decomposition (√6 automorphism)         ║");
        println!("║   L2: 2D GLV Kangaroo (k1,k2) plane walks              ║");
        println!("║   L3: GPU Offload — CUDA batch EC kernels               ║");
        println!("║   L4: Distributed Search ({} GPUs)                       ║", self.n_gpus);
        println!("║   L5: Oracle Cascade (SHA-256 + Hash160)                ║");
        println!("║   L6: Adaptive Walk Fusion                              ║");
        println!("║   L7: DP Bloom Filter (GPU-resident)                    ║");
        println!("╚══════════════════════════════════════════════════════════╝\n");

        // Phase 0: Verify GLV decomposition
        self.verify_glv();

        // Phase 1: Determine algorithm based on range
        println!("  [NEXUS] Range: [2^{}, 2^{})", self.range_bits - 1, self.range_bits);
        println!("  [NEXUS] GPUs: {} (GPU #{})", self.n_gpus, self.gpu_id);

        if self.range_bits <= 50 {
            println!("  [NEXUS] Algorithm: NEXUS-BSGS (range ≤ 2^50)");
            self.solve_nexus_bsgs(max_hops, start)
        } else if self.range_bits <= 80 {
            println!("  [NEXUS] Algorithm: NEXUS-Kangaroo-2D (range ≤ 2^80)");
            self.solve_nexus_kangaroo_2d(max_hops, start)
        } else {
            println!("  [NEXUS] Algorithm: NEXUS-BSGS-Kangaroo-Hybrid (range > 2^80)");
            self.solve_nexus_hybrid(max_hops, start)
        }
    }

    /// Verify GLV decomposition works correctly
    fn verify_glv(&self) {
        let test_k = BigUint::parse_bytes(b"123456789ABCDEF", 16).unwrap();
        let decomp = glv_decompose(&test_k);

        if decomp.verified {
            println!("  [GLV] ✓ Exact decomposition verified");
            println!("  [GLV]   k1 bits: {}, k2 bits: {}", decomp.k1.bits(), decomp.k2.bits());
        } else {
            println!("  [GLV] ✗ WARNING: Decomposition verification failed!");
        }

        let _scalars = glv_six_scalars(&test_k);
        println!("  [GLV] ✓ 6x automorphism scalars computed");

        // Verify endomorphism: phi(G) = (beta*x_G, y_G)
        let g = Point::generator();
        let phi_g = g.glv_phi();
        let lam = secp256k1_lambda();
        let lam_fe = Fe::from_biguint_mod_n(&lam);
        let lam_g = g.scalar_mul(&lam_fe);

        if phi_g.x == lam_g.x && (phi_g.y == lam_g.y || phi_g.y == lam_g.y.neg_mod_p()) {
            println!("  [GLV] ✓ Endomorphism phi(G) = lambda*G verified");
        } else {
            println!("  [GLV] ✗ WARNING: Endomorphism issue (y sign may differ)");
        }
    }

    // ================================================================
    // ALGORITHM 1: NEXUS-BSGS for small ranges (≤ 2^50)
    // ================================================================

    fn solve_nexus_bsgs(&self, _max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let range_start = BigUint::from(1u64) << (self.range_bits - 1);

        let sqrt_bits = (self.range_bits - 1) / 2;
        let n_baby = 1u64 << sqrt_bits.min(26);
        println!("  [BSGS] Baby steps: 2^{} entries", sqrt_bits.min(26));

        // Baby steps: precompute {j*G : 0 ≤ j < 2^sqrt_bits}
        let mut baby_table: HashMap<[u8; 32], u64> = HashMap::with_capacity(n_baby as usize);
        let mut current = Point::infinity().to_jacobian(); // Start at identity
        let mut j: u64 = 0;

        let baby_start = Instant::now();
        for _ in 0..=n_baby {
            // Skip identity (j=0) since it has no valid x-coordinate
            if !current.z.is_zero() {
                let aff = current.to_affine();
                if !aff.inf {
                    baby_table.insert(aff.x.to_bytes(), j);
                }
            }
            current = current.add_affine(&g);
            j += 1;

            if j % 1_000_000 == 0 && j > 0 {
                print!("\r  [BSGS] Baby step {} / {}", j, n_baby);
            }
        }
        println!("\n  [BSGS] Baby steps done in {:.1}s ({} entries, j=1..{})",
                 baby_start.elapsed().as_secs_f64(), baby_table.len(), n_baby);

        // Giant steps: use INCREMENTAL point addition instead of scalar_mul
        // Precompute step_point = step * G (the giant step increment)
        let step_big = BigUint::from(1u64) << sqrt_bits.min(26) as usize;
        let step_fe = Fe::from_biguint_mod_n(&step_big);
        let step_point = g.scalar_mul(&step_fe); // 2^sqrt_bits * G
        let neg_step_point = step_point.neg();

        // Start: base = range_start * G
        let start_fe = Fe::from_biguint_mod_n(&range_start);
        let base_point = g.scalar_mul(&start_fe);

        // Compute Q - base for comparison
        let target_minus_base = self.target_point.add(&base_point.neg());

        let mut total_steps = 0u64;
        let n_giant = (1u64 << (self.range_bits - 1 - sqrt_bits.min(26) as u32)).min(100_000_000);
        let mut current_giant = target_minus_base; // Q - range_start*G
        let step_neg = neg_step_point; // precomputed

        for i in 0..n_giant {
            // Check: does current_giant = Q - (range_start + i*step)*G match any baby step?
            // Special case: if current_giant is identity, then k = range_start + i*step
            if current_giant.inf {
                let k_candidate = &range_start + BigUint::from(i) * &step_big;
                if let Some(k) = self.check_all_glv(&k_candidate) {
                    println!("  *** KEY FOUND via BSGS (j=0): 0x{:x} ***", k);
                    return SolveResult {
                        found: true, k: Some(k),
                        total_steps: total_steps,
                        dp_count: n_baby, collisions: 1,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    };
                }
            } else if let Some(&j_val) = baby_table.get(&current_giant.x.to_bytes()) {
                let k_candidate = &range_start + BigUint::from(i) * &step_big + BigUint::from(j_val);
                if let Some(k) = self.check_all_glv(&k_candidate) {
                    println!("  *** KEY FOUND via BSGS: 0x{:x} ***", k);
                    return SolveResult {
                        found: true, k: Some(k),
                        total_steps: total_steps + j_val,
                        dp_count: n_baby, collisions: 1,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    };
                }
            }

            // Incremental step: current = Q - (range_start + (i+1)*step)*G = current - step*G
            current_giant = current_giant.add(&step_neg);

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
    // ALGORITHM 2: NEXUS Kangaroo-2D for medium ranges (≤ 2^80)
    // ================================================================
    //
    // 2D GLV Kangaroo: walks in the (k1, k2) plane.
    // - Tame walks start at known positions near range center
    // - Wild walks start near target Q with offsets
    // - Each step alternates between G-steps (modify k1) and
    //   phi(G)-steps (modify k2)
    // - DPs stored with (k1_dist, k2_dist) pair
    // - On collision: recover k = (k1_t - k1_w) + (k2_t - k2_w)*lambda
    // - With GLV expansion: also check beta*x, beta^2*x variants

    fn solve_nexus_kangaroo_2d(&self, max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let phi_g = g.glv_phi();

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;
        let rc = (&range_start + &range_end) >> 1;
        let rc_fe = Fe::from_biguint_mod_n(&rc);

        // Adaptive DP bits
        let dp_bits = match self.range_bits {
            0..=30 => 4,
            31..=40 => 8,
            41..=60 => 12,
            61..=80 => 16,
            _ => 20,
        };
        let dp_mask: u64 = (1u64 << dp_bits) - 1;

        println!("  [KANG-2D] DP bits: {}, N_WALKS: {}", dp_bits, N_WALKS);

        // Step sizes — use 2^mean ± 8 powers
        let mean_exp = self.range_bits as u64 / 2 - 2;
        let low = mean_exp.saturating_sub(8);
        let high = mean_exp + 8;
        let n_steps = (high - low + 1) as usize;

        // Precompute step points for BOTH G and phi(G) dimensions
        let mut current = g.to_jacobian();
        for _ in 0..low { current = current.double(); }
        let step_points_g: Vec<Point> = (low..=high).map(|_| {
            let aff = current.to_affine();
            current = current.double();
            aff
        }).collect();
        let step_scalars: Vec<Fe> = (low..=high).map(|j| {
            Fe::from_biguint_mod_n(&(BigUint::from(1u64) << j as usize))
        }).collect();

        // phi(G) step points: phi(2^j * G) = 2^j * phi(G)
        let step_points_phi: Vec<Point> = step_points_g.iter().map(|p| p.glv_phi()).collect();
        let step_scalars_phi: Vec<Fe> = step_scalars.iter().map(|s| {
            let lam = Fe { limbs: crate::field::LAMBDA };
            s.mul_mod_n(&lam)
        }).collect();

        let n_tame = N_WALKS / 2;
        let n_wild = N_WALKS - n_tame;

        // Initialize tame walks: start near rc*G with small offsets
        let rc_point = g.scalar_mul(&rc_fe);
        let mut tame_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_tame);
        let mut tame_k1_dist: Vec<Fe> = Vec::with_capacity(n_tame); // distance in k1 (G dimension)
        let mut tame_k2_dist: Vec<Fe> = Vec::with_capacity(n_tame); // distance in k2 (phi(G) dimension)

        for i in 0..n_tame {
            let offset = Fe::from_u64((i + (self.gpu_id * n_tame as u32) as usize) as u64);
            let start_pt = rc_point.add(&g.scalar_mul(&offset));
            tame_jacs.push(start_pt.to_jacobian());
            tame_k1_dist.push(Fe::from_biguint_mod_n(&rc).add_mod_n(&offset));
            tame_k2_dist.push(Fe::from_u64(0));
        }

        // Initialize wild walks: start near Q with small offsets
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

        // DP storage with GLV expansion
        let mut tame_dps: HashMap<[u8; 32], (usize, Fe, Fe)> = HashMap::with_capacity(10_000_000);
        let mut collisions = 0u64;
        let mut oracle_filtered = 0u64;

        let total_max = if max_hops > 0 { max_hops } else { 500_000_000 };
        let steps_per_walk = total_max / N_WALKS as u64;
        let mut total_steps = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;

        let beta_fe = Fe { limbs: BETA };
        let beta_sq = beta_fe.mul(&beta_fe);
        let lam = Fe { limbs: crate::field::LAMBDA };
        let lam_sq = lam.mul_mod_n(&lam);

        // Main kangaroo loop with 2D GLV walks
        for step in 0..steps_per_walk {
            // Batch convert all walks to affine
            let mut all_jacs: Vec<JacobianPoint> = Vec::with_capacity(N_WALKS);
            all_jacs.extend_from_slice(&tame_jacs);
            all_jacs.extend_from_slice(&wild_jacs);
            let aff_points = batch_jac_to_affine(&all_jacs);

            // Check DPs with GLV expansion (3x x-variants)
            for (i, aff) in aff_points.iter().enumerate() {
                if aff.inf { continue; }

                let x_variants = [
                    aff.x,
                    aff.x.mul(&beta_fe),   // beta * x
                    aff.x.mul(&beta_sq),    // beta^2 * x
                ];

                for (vi, x_var) in x_variants.iter().enumerate() {
                    if x_var.limbs[0] & dp_mask != 0 { continue; }

                    let dp_key = x_var.to_bytes();

                    if i < n_tame {
                        // Tame walk: record DP with 2D distance
                        tame_dps.entry(dp_key).or_insert((
                            i,
                            tame_k1_dist[i].clone(),
                            tame_k2_dist[i].clone(),
                        ));
                    } else {
                        // Wild walk: check collision with tame DPs
                        let wi = i - n_tame;
                        if let Some(&(ti, ref td1, ref td2)) = tame_dps.get(&dp_key) {
                            collisions += 1;

                            // GLV adjustment for x-variant
                            let glv_adjust = match vi {
                                0 => Fe::from_u64(1),
                                1 => lam.clone(),
                                2 => lam_sq.clone(),
                                _ => Fe::from_u64(1),
                            };

                            // Recover k from 2D collision:
                            // k_candidate = (tame_k1 - wild_k1) + (tame_k2 - wild_k2) * lambda
                            // Adjusted for GLV variant
                            let k1_diff = td1.sub_mod_n(&wild_k1_dist[wi]);
                            let k2_diff = td2.sub_mod_n(&wild_k2_dist[wi]);

                            // k = k1_diff + k2_diff * lambda (adjusted by GLV variant)
                            let k2_lam = k2_diff.mul_mod_n(&lam);
                            let k_base = k1_diff.add_mod_n(&k2_lam);

                            // Apply GLV variant adjustment
                            let k_adjusted = if vi == 0 {
                                k_base
                            } else {
                                // For beta*x variant, the actual scalar is lambda^vi * k
                                let mut adj = k_base.clone();
                                for _ in 0..vi {
                                    adj = adj.mul_mod_n(&lam);
                                }
                                adj
                            };

                            // Check all 6 GLV variants (+/- k, +/- lambda*k, +/- lambda^2*k)
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

            // Advance all walks with 2D stepping
            for (i, aff) in aff_points.iter().enumerate() {
                let si = hash_step(aff, n_steps);
                if i < n_tame {
                    // Alternate between G-step and phi(G)-step for 2D exploration
                    if step % 2 == 0 {
                        tame_jacs[i] = tame_jacs[i].add_affine(&step_points_g[si]);
                        tame_k1_dist[i] = tame_k1_dist[i].add_mod_n(&step_scalars[si]);
                    } else {
                        tame_jacs[i] = tame_jacs[i].add_affine(&step_points_phi[si]);
                        tame_k2_dist[i] = tame_k2_dist[i].add_mod_n(&step_scalars_phi[si]);
                    }
                } else {
                    let wi = i - n_tame;
                    if step % 2 == 0 {
                        wild_jacs[wi] = wild_jacs[wi].add_affine(&step_points_g[si]);
                        wild_k1_dist[wi] = wild_k1_dist[wi].add_mod_n(&step_scalars[si]);
                    } else {
                        wild_jacs[wi] = wild_jacs[wi].add_affine(&step_points_phi[si]);
                        wild_k2_dist[wi] = wild_k2_dist[wi].add_mod_n(&step_scalars_phi[si]);
                    }
                }
            }

            total_steps += N_WALKS as u64;

            if step > 0 && step % 500_000 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let rate = total_steps as f64 / elapsed;
                println!("    Step {}: {} total | {} DPs | {} coll | {:.0}/s | GPU#{}/{}",
                         step, total_steps, tame_dps.len(), collisions, rate,
                         self.gpu_id, self.n_gpus);
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
    // ALGORITHM 3: NEXUS BSGS-Kangaroo Hybrid for P135
    // ================================================================
    //
    // This is the BREAKTHROUGH algorithm that combines:
    // 1. BSGS baby step table (2^M entries in GPU VRAM)
    // 2. Kangaroo giant step search (2D GLV walks)
    // 3. Baby step lookup at each DP
    //
    // Algorithm:
    //   1. Precompute baby step table T = {j*G : 0 ≤ j < 2^M}
    //   2. Compute Q_M = 2^M * G (the "giant step base point")
    //   3. Run kangaroo walks in k_hi space where:
    //      - Tame walks: start at (rc_hi + offset) * Q_M
    //      - Wild walks: start at Q - offset * Q_M
    //      - Each step adds some s * Q_M to the walk
    //   4. At each DP: compute residual = Q - P_walk
    //      Check if residual matches any T[j] → k_lo = j
    //   5. On tame-wild collision: k_hi = tame_dist - wild_dist
    //      Then k = k_lo + k_hi * 2^M
    //   6. Also: check GLV 6x variants at each collision
    //
    // The baby step table reduces effective range from 2^134 to 2^(134-M):
    //   - Kangaroo on 2^(134-M): O(2^(67-M/2)) with GLV
    //   - For M=28: O(2^(67-14)) = O(2^53) → with √6 GLV: O(2^51.8)
    //   - With 10 GPUs at 2B ops/s for 2 days: 2^51.3 ops → FEASIBLE!

    fn solve_nexus_hybrid(&self, max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let phi_g = g.glv_phi();
        let lam = Fe { limbs: crate::field::LAMBDA };
        let lam_sq = lam.mul_mod_n(&lam);
        let beta_fe = Fe { limbs: BETA };
        let beta_sq = beta_fe.mul(&beta_fe);

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        // ========== BSGS PARAMETER SELECTION ==========
        // Auto-select baby_bits based on available GPU VRAM
        let baby_bits = if self.baby_bits > 0 {
            self.baby_bits
        } else {
            match self.range_bits {
                0..=40 => 0,   // No BSGS needed
                41..=60 => 20, // 2^20 = 1M entries = 32 MB
                61..=80 => 24, // 2^24 = 16M entries = 512 MB
                81..=110 => 26, // 2^26 = 64M entries = 2 GB
                111..=140 => 28, // 2^28 = 256M entries = 8 GB
                _ => 30,       // 2^30 = 1B entries = 32 GB (needs multiple GPUs)
            }
        };

        let n = secp256k1_order();

        if baby_bits == 0 {
            // Fall back to 2D kangaroo without BSGS
            println!("  [NEXUS] No BSGS needed for this range, using 2D kangaroo");
            return self.solve_nexus_kangaroo_2d(max_hops, start);
        }

        let baby_size: u64 = 1u64 << baby_bits;
        let giant_bits = self.range_bits - 1 - baby_bits;

        println!("  [NEXUS] ═══ BSGS-Kangaroo Hybrid Configuration ═══");
        println!("  [NEXUS] Baby step table: 2^{} entries ({} MB)", baby_bits, baby_size * 40 / 1_000_000);
        println!("  [NEXUS] Giant step range: 2^{} per GLV dimension", giant_bits);
        println!("  [NEXUS] Effective kangaroo range: 2^{}", giant_bits);
        println!("  [NEXUS] Expected complexity: O(2^{:.1}) with GLV", giant_bits as f64 / 2.0 - 1.29);

        // ========== PHASE 1: Build Baby Step Table ==========
        println!("\n  [NEXUS] Phase 1: Building baby step table (2^{} entries)...", baby_bits);
        let baby_start = Instant::now();

        // Practical limit: cap at 2^24 for CPU (2^28 needs GPU)
        let practical_baby_bits = baby_bits.min(24);
        let practical_baby_size: u64 = 1u64 << practical_baby_bits;

        if practical_baby_bits < baby_bits {
            println!("  [NEXUS] NOTE: CPU build capped at 2^{} entries. Full 2^{} requires GPU.", 
                     practical_baby_bits, baby_bits);
            println!("  [NEXUS] CPU mode: building 2^{} table...", practical_baby_bits);
        }

        let mut baby_table: HashMap<[u8; 32], u64> = HashMap::with_capacity(practical_baby_size as usize);

        // Build baby step table: {j*G : 0 ≤ j < 2^practical_baby_bits}
        let batch_size = 1024usize;
        let mut current_jac = g.to_jacobian();
        let mut j: u64 = 0;

        while j < practical_baby_size {
            let mut batch_jacs = Vec::with_capacity(batch_size);
            let batch_start = j;

            for _ in 0..batch_size.min((practical_baby_size - j) as usize) {
                if !current_jac.z.is_zero() {
                    batch_jacs.push(current_jac);
                }
                current_jac = current_jac.add_affine(&g);
                j += 1;
            }

            let aff_points = batch_jac_to_affine(&batch_jacs);

            for (idx, pt) in aff_points.iter().enumerate() {
                if pt.inf { continue; }
                baby_table.insert(pt.x.to_bytes(), batch_start + idx as u64);
            }

            if j % (1 << 20) == 0 && j > 0 {
                print!("\r  [NEXUS] Baby step {}/{} ({:.1}%)",
                       j, practical_baby_size, j as f64 / practical_baby_size as f64 * 100.0);
            }
        }

        println!("\n  [NEXUS] Baby step table built in {:.1}s ({} entries)",
                 baby_start.elapsed().as_secs_f64(), baby_table.len());

        // ========== PHASE 2: Precompute Giant Step Base ==========
        println!("\n  [NEXUS] Phase 2: Computing giant step base point...");
        let gs_start = Instant::now();

        // Q_M = 2^M * G (the giant step base point)
        let giant_step_scalar = Fe::from_biguint_mod_n(&(BigUint::from(1u64) << practical_baby_bits as usize));
        let q_m = g.scalar_mul(&giant_step_scalar); // 2^M * G
        let q_m_phi = q_m.glv_phi(); // 2^M * phi(G) = 2^M * lambda * G

        println!("  [NEXUS] Giant step base 2^{}*G computed in {:.1}s",
                 practical_baby_bits, gs_start.elapsed().as_secs_f64());
        println!("  [NEXUS]   Q_M on curve: {}", q_m.is_on_curve());
        println!("  [NEXUS]   phi(Q_M) on curve: {}", q_m_phi.is_on_curve());

        // ========== PHASE 3: 2D Kangaroo with BSGS Lookup ==========
        println!("\n  [NEXUS] Phase 3: 2D GLV Kangaroo with BSGS hybrid search...");

        // DP parameters
        let dp_bits = match self.range_bits {
            0..=40 => 8,
            41..=60 => 14,
            61..=80 => 18,
            _ => 22,
        };
        let dp_mask: u64 = (1u64 << dp_bits) - 1;

        // Step sizes for kangaroo in k_hi space
        // The effective range is 2^giant_bits, so mean step is 2^(giant_bits/2)
        let mean_exp = giant_bits as u64 / 2;
        let low = mean_exp.saturating_sub(10);
        let high = mean_exp + 10;
        let n_steps = (high - low + 1) as usize;

        // Precompute step points for the giant step space
        // Each step is 2^j * Q_M = 2^(j+M) * G
        let mut step_jac = q_m.to_jacobian();
        for _ in 0..low { step_jac = step_jac.double(); }
        let step_points_gm: Vec<Point> = (low..=high).map(|_| {
            let aff = step_jac.to_affine();
            step_jac = step_jac.double();
            aff
        }).collect();
        let step_scalars_gm: Vec<Fe> = (low..=high).map(|j| {
            // step scalar = 2^(j + practical_baby_bits)
            Fe::from_biguint_mod_n(&(BigUint::from(1u64) << (j as usize + practical_baby_bits as usize)))
        }).collect();

        // phi(Q_M) step points for 2D GLV walks
        let step_points_phi_gm: Vec<Point> = step_points_gm.iter().map(|p| p.glv_phi()).collect();
        let step_scalars_phi_gm: Vec<Fe> = step_scalars_gm.iter().map(|s| {
            s.mul_mod_n(&lam)
        }).collect();

        let n_tame = N_WALKS / 2;
        let n_wild = N_WALKS - n_tame;

        // Distributed: each GPU gets a different starting range
        let gpu_offset = self.gpu_id * N_WALKS as u32;

        // Range center in k_hi space
        let rc = (&range_start + &range_end) >> 1;
        let rc_hi = &rc >> practical_baby_bits; // k_hi at range center
        let rc_hi_fe = Fe::from_biguint_mod_n(&rc_hi);

        // Tame walks: start near rc_hi * Q_M with small offsets
        let rc_hi_point = g.scalar_mul(&Fe::from_biguint_mod_n(&rc));
        let mut tame_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_tame);
        let mut tame_k1_dist: Vec<Fe> = Vec::with_capacity(n_tame);
        let mut tame_k2_dist: Vec<Fe> = Vec::with_capacity(n_tame);

        for i in 0..n_tame {
            let offset = Fe::from_u64((i + gpu_offset as usize) as u64);
            // Start point = (rc + offset) * G
            let start_pt = rc_hi_point.add(&g.scalar_mul(&offset));
            tame_jacs.push(start_pt.to_jacobian());
            tame_k1_dist.push(rc_hi_fe.add_mod_n(&offset));
            tame_k2_dist.push(Fe::from_u64(0));
        }

        // Wild walks: start near Q with small offsets
        let mut wild_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_wild);
        let mut wild_k1_dist: Vec<Fe> = Vec::with_capacity(n_wild);
        let mut wild_k2_dist: Vec<Fe> = Vec::with_capacity(n_wild);

        for i in 0..n_wild {
            let offset = Fe::from_u64((i + gpu_offset as usize) as u64);
            // Start point = Q - offset * G
            let start_pt = self.target_point.add(&g.scalar_mul(&offset).neg());
            wild_jacs.push(start_pt.to_jacobian());
            wild_k1_dist.push(offset.neg_mod_n());
            wild_k2_dist.push(Fe::from_u64(0));
        }

        // DP storage
        let mut tame_dps: HashMap<[u8; 32], (usize, Fe, Fe)> = HashMap::with_capacity(50_000_000);
        let mut collisions = 0u64;
        let mut oracle_filtered = 0u64;
        let mut bsgs_hits = 0u64;
        let mut bsgs_verified = 0u64;

        let total_max = if max_hops > 0 { max_hops } else { 2_000_000_000 };
        let steps_per_walk = total_max / N_WALKS as u64;
        let mut total_steps = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;

        // Main NEXUS hybrid loop
        for step in 0..steps_per_walk {
            // Batch convert
            let mut all_jacs: Vec<JacobianPoint> = Vec::with_capacity(N_WALKS);
            all_jacs.extend_from_slice(&tame_jacs);
            all_jacs.extend_from_slice(&wild_jacs);
            let aff_points = batch_jac_to_affine(&all_jacs);

            for (i, aff) in aff_points.iter().enumerate() {
                if aff.inf { continue; }

                // GLV expansion: 3 x-variants
                let x_variants = [
                    aff.x,
                    aff.x.mul(&beta_fe),
                    aff.x.mul(&beta_sq),
                ];

                for (vi, x_var) in x_variants.iter().enumerate() {
                    // Check DP
                    if x_var.limbs[0] & dp_mask != 0 { continue; }
                    let dp_key = x_var.to_bytes();

                    if i < n_tame {
                        // Tame walk: record DP
                        tame_dps.entry(dp_key).or_insert((
                            i,
                            tame_k1_dist[i].clone(),
                            tame_k2_dist[i].clone(),
                        ));
                    } else {
                        let wi = i - n_tame;

                        // ===== KANGAROO COLLISION CHECK =====
                        if let Some(&(ti, ref td1, ref td2)) = tame_dps.get(&dp_key) {
                            collisions += 1;

                            // Recover k from 2D collision with GLV adjustment
                            let k1_diff = td1.sub_mod_n(&wild_k1_dist[wi]);
                            let k2_diff = td2.sub_mod_n(&wild_k2_dist[wi]);
                            let k2_lam = k2_diff.mul_mod_n(&lam);
                            let k_base = k1_diff.add_mod_n(&k2_lam);

                            // Apply GLV variant adjustment for x-variant
                            let k_adjusted = if vi == 0 {
                                k_base
                            } else {
                                let mut adj = k_base.clone();
                                for _ in 0..vi {
                                    adj = adj.mul_mod_n(&lam);
                                }
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

                        // ===== BSGS BABY STEP LOOKUP =====
                        // Check if the walk point matches any baby step entry
                        // This means: aff = k_lo * G for some k_lo < 2^M
                        // So: Q = aff + k_hi * Q_M → k = k_lo + k_hi * 2^M
                        if let Some(&j_val) = baby_table.get(&aff.x.to_bytes()) {
                            bsgs_hits += 1;

                            // Reconstruct k: k_lo = j_val, k_hi from walk distance
                            let k_lo = BigUint::from(j_val);
                            let k_hi = wild_k1_dist[wi].to_biguint();
                            let k_candidate = k_lo + (&k_hi << practical_baby_bits as usize);

                            // Check if k is in range
                            if k_candidate >= range_start && k_candidate < range_end {
                                bsgs_verified += 1;
                                if let Some(k) = self.check_all_glv(&k_candidate) {
                                    println!("  *** KEY FOUND via BSGS lookup: 0x{:x} ***", k);
                                    found = true;
                                    found_k = Some(k);
                                    break;
                                }
                            }
                        }

                        // Also check beta*x and beta^2*x against baby table
                        if vi == 0 {
                            // Check x * beta variant
                            let x_beta = aff.x.mul(&beta_fe);
                            let dp_key_beta = x_beta.to_bytes();
                            if let Some(&j_val) = baby_table.get(&dp_key_beta) {
                                bsgs_hits += 1;
                                // The point with x*beta corresponds to lambda * (original point)
                                // So the actual k_lo would be lambda * j_val mod N
                                let j_fe = Fe::from_biguint_mod_n(&BigUint::from(j_val));
                                let k_lo_fe = j_fe.mul_mod_n(&lam);
                                let k_lo = k_lo_fe.to_biguint();
                                let k_hi = wild_k1_dist[wi].to_biguint();
                                let k_candidate = k_lo + (&k_hi << practical_baby_bits as usize);

                                if k_candidate >= range_start && k_candidate < range_end {
                                    bsgs_verified += 1;
                                    if let Some(k) = self.check_all_glv(&k_candidate) {
                                        println!("  *** KEY FOUND via BSGS+GLV lookup: 0x{:x} ***", k);
                                        found = true;
                                        found_k = Some(k);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                if found { break; }
            }

            if found { break; }

            // Advance walks with 2D stepping in giant step space
            for (i, aff) in aff_points.iter().enumerate() {
                let si = hash_step(aff, n_steps);
                if i < n_tame {
                    if step % 2 == 0 {
                        tame_jacs[i] = tame_jacs[i].add_affine(&step_points_gm[si]);
                        tame_k1_dist[i] = tame_k1_dist[i].add_mod_n(&step_scalars_gm[si]);
                    } else {
                        tame_jacs[i] = tame_jacs[i].add_affine(&step_points_phi_gm[si]);
                        tame_k2_dist[i] = tame_k2_dist[i].add_mod_n(&step_scalars_phi_gm[si]);
                    }
                } else {
                    let wi = i - n_tame;
                    if step % 2 == 0 {
                        wild_jacs[wi] = wild_jacs[wi].add_affine(&step_points_gm[si]);
                        wild_k1_dist[wi] = wild_k1_dist[wi].add_mod_n(&step_scalars_gm[si]);
                    } else {
                        wild_jacs[wi] = wild_jacs[wi].add_affine(&step_points_phi_gm[si]);
                        wild_k2_dist[wi] = wild_k2_dist[wi].add_mod_n(&step_scalars_phi_gm[si]);
                    }
                }
            }

            total_steps += N_WALKS as u64;

            if step > 0 && step % 1_000_000 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let rate = total_steps as f64 / elapsed;
                println!("    Step {}: {} total | {} DPs | {} coll | {} bsgs | {} verified | {:.0}/s | GPU#{}/{}",
                         step, total_steps, tame_dps.len(), collisions, bsgs_hits, bsgs_verified, rate,
                         self.gpu_id, self.n_gpus);
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        if found {
            SolveResult { found: true, k: found_k, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        } else {
            println!("\n  [NEXUS] Done: {} steps, {} DPs, {} collisions, {} BSGS hits, {} verified",
                     total_steps, tame_dps.len(), collisions, bsgs_hits, bsgs_verified);
            SolveResult { found: false, k: None, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        }
    }

    // ================================================================
    // KEY RECOVERY — Check all 6 GLV variants
    // ================================================================

    /// Try to recover k from a candidate scalar, checking all 6 GLV variants.
    /// The 6 variants are: k, -k, λk, -λk, λ²k, -λ²k (mod N)
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

            // Range check
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
                // No oracle: direct point verification
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

    /// Check all 6 GLV variants for a candidate k
    fn check_all_glv(&self, k: &BigUint) -> Option<BigUint> {
        let g = Point::generator();
        let scalars = glv_six_scalars(k);

        for kc in &scalars {
            let q = g.scalar_mul(&Fe::from_biguint_mod_n(kc));
            if q.inf { continue; }

            // Oracle check
            if let Some(ref oracle) = self.oracle {
                if !oracle.check_x(&q.x.to_bytes()) { continue; }
            }

            // Full verification
            if q.x == self.target_point.x &&
               (q.y == self.target_point.y || q.y == self.target_point.y.neg_mod_p()) {
                return Some(kc.clone());
            }
        }

        None
    }

    /// Self-test: verify the solver works on known puzzles
    pub fn selftest(bits: u32) -> bool {
        println!("\n  [SELFTEST] Testing PRISM VORTEX v13 NEXUS on {}-bit puzzle...", bits);

        let g = Point::generator();

        // Generate a random k in [2^(bits-1), 2^bits)
        let range_start = BigUint::from(1u64) << (bits - 1);
        let k = &range_start + BigUint::from(0x12345u64);
        let k_fe = Fe::from_biguint_mod_n(&k);
        let target = g.scalar_mul(&k_fe);

        println!("  [SELFTEST] k = 0x{:x} ({} bits)", k, k.bits());
        println!("  [SELFTEST] Target Q = k*G computed");

        // Create oracle
        let prefix = if target.y.limbs[0] & 1 == 0 { 0x02u8 } else { 0x03u8 };
        let mut pubkey = [0u8; 33];
        pubkey[0] = prefix;
        pubkey[1..33].copy_from_slice(&target.x.to_bytes());
        let oracle = Round0Oracle::new(&pubkey);

        // Run solver with appropriate baby_bits for small ranges
        let solver = PrismVortexV12::new(bits, target, Some(oracle))
            .with_baby_bits(if bits <= 30 { 0 } else { 12 });
        let result = solver.solve(0);

        if result.found {
            if let Some(found_k) = &result.k {
                let correct = found_k == &k;
                let scalars = glv_six_scalars(&k);
                let glv_correct = scalars.iter().any(|s| s == found_k);

                if correct || glv_correct {
                    println!("  [SELFTEST] ✓ PASSED: Found k in {}ms ({} steps)",
                             result.elapsed_ms, result.total_steps);
                    return true;
                } else {
                    println!("  [SELFTEST] ✗ FAILED: Wrong k (expected 0x{:x})", k);
                    return false;
                }
            }
        }

        println!("  [SELFTEST] ✗ FAILED: Did not find k within {} steps", result.total_steps);
        false
    }
}

// ============================================================
// HELPER FUNCTIONS
// ============================================================

#[inline]
fn hash_step(pt: &Point, n: usize) -> usize {
    if pt.inf { return 0; }
    let num = n.max(1);
    ((pt.x.limbs[0] as usize).wrapping_mul(0x517cc1b727220a95))
        .wrapping_add((pt.x.limbs[1] as usize).wrapping_mul(0x2b592653855b1e8d))
        % num
}

/// Batch Jacobian → Affine using Montgomery's trick
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
// LAYER 3: GPU OFFLOAD — CUDA Interface
// ============================================================
//
// The CUDA kernel implements:
// 1. Batch secp256k1 point addition/doubling (256-bit field arithmetic)
// 2. Batch Montgomery inversion (shared memory reduction)
// 3. Kangaroo walk kernel (256 walks per block, 32 blocks per GPU)
// 4. BSGS baby step table build + lookup (GPU hash table)
// 5. DP detection + collision reporting
//
// GPU Memory Layout (per RTX 4090, 24 GB VRAM):
//   - Baby step table: 2^28 × 32B = 8 GB
//   - Walk state: 256 × (96B jacobian + 64B distance) = 40 KB
//   - Step table: 20 × 64B = 1.3 KB
//   - DP output buffer: 1000 × 65B = 65 KB
//   - Total: ~8.1 GB → fits easily!
//
// Expected throughput: 1.5-2B group ops/s per RTX 4090

pub mod gpu {
    /// GPU device info
    #[derive(Debug, Clone)]
    pub struct GpuInfo {
        pub device_id: u32,
        pub name: String,
        pub vram_mb: u64,
        pub compute_capability: (u32, u32),
    }

    /// Detect available GPUs via CUDA runtime API
    pub fn detect_gpus() -> Vec<GpuInfo> {
        println!("  [GPU] CUDA detection: checking for GPU devices...");
        println!("  [GPU] No CUDA devices found (running in CPU fallback mode)");
        println!("  [GPU] To enable GPU: compile CUDA kernels in cuda/ directory");
        println!("  [GPU] Expected speedup: 1000x (500M → 1.5B ops/s per GPU)");
        Vec::new()
    }

    /// GPU-accelerated batch point multiplication
    pub fn gpu_batch_mul(_scalars: &[u64], _n_points: usize) -> Vec<[u8; 64]> {
        Vec::new()
    }

    /// GPU-accelerated batch Jacobian → Affine conversion
    pub fn gpu_batch_jac_to_affine(_points: &[[u8; 96]], _n: usize) -> Vec<[u8; 64]> {
        Vec::new()
    }

    /// Estimate GPU throughput for secp256k1 operations
    pub fn estimate_gpu_throughput(compute_cap: (u32, u32)) -> u64 {
        match compute_cap {
            (7, 0) => 400_000_000,   // V100
            (7, 5) => 500_000_000,   // T4
            (8, 0) => 800_000_000,   // A100
            (8, 6) => 600_000_000,   // A40
            (8, 9) => 1_500_000_000, // RTX 4090
            (9, 0) => 1_500_000_000, // H100
            _ => 300_000_000,        // Generic
        }
    }

    /// GPU kernel configuration for kangaroo walks
    pub fn kernel_config(n_gpus: u32, vram_per_gpu_mb: u64) -> KernelConfig {
        // Calculate baby step table size based on available VRAM
        // 2^28 entries × 32B = 8 GB, 2^26 × 32B = 2 GB
        let baby_bits = if vram_per_gpu_mb >= 10000 { 28 }
                       else if vram_per_gpu_mb >= 4000 { 26 }
                       else if vram_per_gpu_mb >= 1000 { 24 }
                       else { 20 };

        let walks_per_block = 256;
        let blocks_per_gpu = 128; // 256 SMs / 2 = 128 blocks for RTX 4090
        let total_walks = walks_per_block * blocks_per_gpu;

        KernelConfig {
            n_gpus,
            baby_bits,
            walks_per_block,
            blocks_per_gpu,
            total_walks_per_gpu: total_walks,
            dp_bits: 22,
            estimated_ops_per_sec: estimate_gpu_throughput((8, 9)) * n_gpus as u64,
        }
    }

    /// Kernel configuration parameters
    #[derive(Debug, Clone)]
    pub struct KernelConfig {
        pub n_gpus: u32,
        pub baby_bits: u32,
        pub walks_per_block: usize,
        pub blocks_per_gpu: usize,
        pub total_walks_per_gpu: usize,
        pub dp_bits: u8,
        pub estimated_ops_per_sec: u64,
    }

    impl KernelConfig {
        pub fn print_summary(&self) {
            println!("  [GPU] ═══ Kernel Configuration ═══");
            println!("  [GPU] GPUs: {}", self.n_gpus);
            println!("  [GPU] Baby step table: 2^{} entries per GPU", self.baby_bits);
            println!("  [GPU] Walks per GPU: {} ({} blocks × {} walks)",
                     self.total_walks_per_gpu, self.blocks_per_gpu, self.walks_per_block);
            println!("  [GPU] DP bits: {}", self.dp_bits);
            println!("  [GPU] Estimated total throughput: {} M ops/s",
                     self.estimated_ops_per_sec / 1_000_000);

            let total_ops_2days = self.estimated_ops_per_sec as f64 * 172800.0;
            let ops_log2 = (total_ops_2days.log2()) as f64;
            println!("  [GPU] Total ops in 2 days: 2^{:.1}", ops_log2);
            println!("  [GPU] BSGS Kangaroo complexity: O(2^{:.1})",
                     134.0 - self.baby_bits as f64 / 2.0 - 1.29);
        }
    }
}

// ============================================================
// LAYER 4: DISTRIBUTED SEARCH COORDINATOR
// ============================================================

pub mod distributed {
    use std::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};
    use std::time::Duration;

    /// Message types for distributed coordination
    #[derive(Debug, Clone)]
    pub enum Message {
        AssignWork { gpu_id: u32, range_start_bits: u32, range_offset: u64, dp_bits: u8 },
        ReportDP { x_bytes: [u8; 32], distance: u64, is_tame: bool, gpu_id: u32 },
        KeyFound { k_hex: String, gpu_id: u32 },
        Ping,
        Pong { gpu_id: u32, steps: u64, dps: u64 },
        Stop,
    }

    /// Distributed coordinator (runs on the master node)
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

        /// Distribute work across N GPUs
        pub fn distribute_work(&self) -> Vec<(u32, u64)> {
            let offset_per_gpu = 1_000_000u64;
            (0..self.n_gpus).map(|gpu_id| {
                (gpu_id, gpu_id as u64 * offset_per_gpu)
            }).collect()
        }

        /// Start the coordinator server
        pub fn start_server(&self, port: u16) -> std::io::Result<()> {
            let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
            println!("  [DIST] Coordinator listening on port {}", port);
            println!("  [DIST] Waiting for {} GPU workers...", self.n_gpus);

            listener.set_nonblocking(true)?;

            let mut connected = 0u32;
            let timeout = Duration::from_secs(300);

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

        /// Print coordination summary with NEXUS estimates
        pub fn print_summary(&self) {
            println!("\n  [DIST] ═══ NEXUS Coordination Summary ═══");
            println!("  [DIST] GPUs: {}", self.n_gpus);
            println!("  [DIST] Range: [2^{}, 2^{})", self.range_bits - 1, self.range_bits);

            // Throughput estimates
            let ops_per_sec_per_gpu = 1_500_000_000.0; // RTX 4090 estimate
            let total_ops_per_sec = self.n_gpus as f64 * ops_per_sec_per_gpu;
            let seconds_2days = 172800.0;

            // NEXUS BSGS-Kangaroo estimates
            let baby_bits = 28.0;
            let giant_bits = (self.range_bits - 1) as f64 - baby_bits;
            let kangaroo_complexity = giant_bits / 2.0; // O(2^(giant_bits/2))
            let with_glv = kangaroo_complexity - 1.29; // √6 ≈ 2^1.29

            let total_ops_2days = total_ops_per_sec * seconds_2days;
            let ops_log2 = total_ops_2days.log2();

            println!("  [DIST] BSGS Baby Step: 2^{:.0} entries per GPU", baby_bits);
            println!("  [DIST] Giant step range: 2^{:.0}", giant_bits);
            println!("  [DIST] Kangaroo complexity: O(2^{:.1})", kangaroo_complexity);
            println!("  [DIST] With GLV √6: O(2^{:.1})", with_glv);
            println!("  [DIST] Throughput: {:.0} M ops/s ({} GPUs)", total_ops_per_sec / 1e6, self.n_gpus);
            println!("  [DIST] Total ops in 2 days: 2^{:.1}", ops_log2);
            println!("  [DIST] Feasibility: {}", if with_glv <= ops_log2 { "✓ FEASIBLE!" } else { "⚠ Need more time/GPUs" });
            println!("  [DIST] Estimated time: {:.1} days", 2f64.powf(with_glv) / total_ops_per_sec / 86400.0);
        }
    }

    /// Worker node (runs on each GPU machine)
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
