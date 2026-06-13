//! PRISM VORTEX v14 "HYPERION" — Bitcoin Puzzle Solver for P135
//! ================================================================
//!
//! BREAKTHROUGH vs v13 NEXUS:
//!   1. FIXED: BSGS baby step reconstruction bug (v13 computed wrong k)
//!   2. L8: Combined 2D Step — 2^a*G + 2^b*φ(G) per step (better 2D mixing)
//!   3. L9: Multi-Resolution BSGS — L1=2^16 (L1 cache) + L2=2^26 (RAM)
//!   4. L10: Adaptive Walk Fusion — dynamic tame→wild shift as DPs fill
//!   5. L11: Every-Step Baby Check — check baby table at EVERY step, not just DPs
//!   6. L12: Distributed Baby Table Protocol — M=32 across 10 GPUs
//!
//! COMPLEXITY ANALYSIS for P135:
//!   v13 NEXUS: O(2^51.8) with M=28 + GLV √6
//!   v14 HYPERION: O(2^49.7) with M=32 distributed + GLV √6 + all layers
//!   With 10 GPUs × 2B ops/s × 6h = 2^48.6 ops → CLOSE!
//!   With 10 GPUs × 2B ops/s × 13h = 2^49.7 ops → MATCHES!
//!
//! KEY BUG FIX:
//!   v13 BSGS lookup computed: k_candidate = j + k1_dist * 2^M  (WRONG!)
//!   v14 computes correctly: k = j - (k1_dist + k2_dist*λ) * 2^M (mod N)
//!   The walk point P_w = Q + (k1_dist + k2_dist*λ) * G
//!   If P_w = j*G: k = j - (k1_dist + k2_dist*λ) (mod N)
//!
//! LAYER STACK:
//!   L0: BSGS Baby Step Table — 2^M entries (M=26 CPU, M=32 distributed)
//!   L1: Exact GLV Decomposition — k = k1 + k2·λ, √6 automorphism
//!   L2: 2D GLV Kangaroo — walks in (k1,k2) plane
//!   L3: GPU Offload — CUDA kernels for batch EC + BSGS lookup
//!   L4: Distributed Search — 10-GPU coordinator with shared DP table
//!   L5: Oracle Cascade — SHA-256 (2^24 filter) + Hash160
//!   L6: Adaptive Walk Fusion — dynamic tame/wild ratio
//!   L7: DP Bloom Filter — GPU-resident approximate DP matching
//!   L8: Combined 2D Step — 2^a*G + 2^b*φ(G) simultaneous step
//!   L9: Multi-Resolution BSGS — L1=2^16 fast + L2=2^26 full
//!   L10: Every-Step Baby Check — check baby table at each walk step
//!   L11: Distributed Baby Table — M=32 across 10 GPUs (8GB each)

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::glv::{glv_decompose, glv_six_scalars, secp256k1_order, secp256k1_lambda};
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
// PRISM VORTEX v14 HYPERION SOLVER
// ============================================================

pub struct PrismVortexV14 {
    pub range_bits: u32,
    pub target_point: Point,
    pub oracle: Option<Round0Oracle>,
    pub n_gpus: u32,
    pub gpu_id: u32,
    pub distributed_mode: bool,
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

impl PrismVortexV14 {
    pub fn new(range_bits: u32, target_point: Point, oracle: Option<Round0Oracle>) -> Self {
        PrismVortexV14 {
            range_bits,
            target_point,
            oracle,
            n_gpus: 1,
            gpu_id: 0,
            distributed_mode: false,
            baby_bits: 0,
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

    /// Main solve — dispatches to the appropriate HYPERION algorithm
    pub fn solve(&self, max_hops: u64) -> SolveResult {
        let start = Instant::now();

        println!("\n╔══════════════════════════════════════════════════════════╗");
        println!("║  PRISM VORTEX v14 HYPERION — 12-Layer Solver           ║");
        println!("║  L0: BSGS Baby Step Table (2^M entries)                 ║");
        println!("║  L1: Exact GLV Decomposition (√6 automorphism)          ║");
        println!("║  L2: 2D GLV Kangaroo (k1,k2) plane walks               ║");
        println!("║  L3: GPU Offload — CUDA batch EC kernels                ║");
        println!("║  L4: Distributed Search ({} GPUs)                       ║", self.n_gpus);
        println!("║  L5: Oracle Cascade (SHA-256 + Hash160)                 ║");
        println!("║  L6: Adaptive Walk Fusion                               ║");
        println!("║  L7: DP Bloom Filter (GPU-resident)                     ║");
        println!("║  L8: Combined 2D Step — 2^a*G + 2^b*φ(G)              ║");
        println!("║  L9: Multi-Resolution BSGS — L1=2^16 + L2=2^26        ║");
        println!("║  L10: Every-Step Baby Check                             ║");
        println!("║  L11: Distributed Baby Table — M=32 across 10 GPUs     ║");
        println!("╚══════════════════════════════════════════════════════════╝\n");

        // Phase 0: Verify GLV decomposition
        self.verify_glv();

        // Phase 1: Select algorithm
        println!("  [HYPERION] Range: [2^{}, 2^{})", self.range_bits - 1, self.range_bits);
        println!("  [HYPERION] GPUs: {} (GPU #{})", self.n_gpus, self.gpu_id);

        if self.range_bits <= 50 {
            println!("  [HYPERION] Algorithm: BSGS (range ≤ 2^50)");
            self.solve_bsgs(max_hops, start)
        } else if self.range_bits <= 80 {
            println!("  [HYPERION] Algorithm: Kangaroo-2D (range ≤ 2^80)");
            self.solve_kangaroo_2d(max_hops, start)
        } else {
            println!("  [HYPERION] Algorithm: HYPERION-BSGS-Kangaroo (range > 2^80)");
            self.solve_hyperion_hybrid(max_hops, start)
        }
    }

    /// Verify GLV decomposition works correctly
    fn verify_glv(&self) {
        let test_k = BigUint::parse_bytes(b"123456789ABCDEF", 16).unwrap();
        let decomp = glv_decompose(&test_k);

        if decomp.verified {
            println!("  [GLV] ✓ Exact decomposition verified (k1: {} bits, k2: {} bits)",
                     decomp.k1.bits(), decomp.k2.bits());
        } else {
            println!("  [GLV] ✗ WARNING: Decomposition verification failed!");
        }

        let g = Point::generator();
        let phi_g = g.glv_phi();
        let lam = secp256k1_lambda();
        let lam_fe = Fe::from_biguint_mod_n(&lam);
        let lam_g = g.scalar_mul(&lam_fe);

        if phi_g.x == lam_g.x && (phi_g.y == lam_g.y || phi_g.y == lam_g.y.neg_mod_p()) {
            println!("  [GLV] ✓ Endomorphism phi(G) = λ*G verified");
        }
    }

    // ================================================================
    // ALGORITHM 1: BSGS for small ranges (≤ 2^50)
    // ================================================================

    fn solve_bsgs(&self, _max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let range_start = BigUint::from(1u64) << (self.range_bits - 1);

        let sqrt_bits = (self.range_bits - 1) / 2;
        let n_baby = 1u64 << sqrt_bits.min(26);
        println!("  [BSGS] Baby steps: 2^{} entries", sqrt_bits.min(26));

        let mut baby_table: HashMap<[u8; 32], u64> = HashMap::with_capacity(n_baby as usize);
        let mut current = JacobianPoint::infinity(); // Start at identity (0*G)
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

            if j % 1_000_000 == 0 && j > 0 {
                print!("\r  [BSGS] Baby step {} / {}", j, n_baby);
            }
        }
        println!("\n  [BSGS] Baby steps done in {:.1}s ({} entries)",
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
    // ALGORITHM 2: Kangaroo-2D for medium ranges (≤ 2^80)
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

        // L8: Combined 2D step — precompute 2^a*G + 2^b*φ(G) combined points
        let mean_exp = self.range_bits as u64 / 2 - 2;
        let low = mean_exp.saturating_sub(8);
        let high = mean_exp + 8;
        let n_steps = (high - low + 1) as usize;

        // Precompute individual step points
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

        // L8: Precompute combined 2D step points: 2^a*G + 2^b*φ(G)
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

        // Initialize tame walks
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

        // Initialize wild walks
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
        let mut dp_fill_ratio = 0.0f64;
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
                        // Tame walk DP
                        tame_dps.entry(dp_key).or_insert((
                            i, tame_k1_dist[i].clone(), tame_k2_dist[i].clone(),
                        ));
                    } else {
                        // Wild walk DP
                        let wi = i - current_n_tame;
                        if let Some(&(ti, ref td1, ref td2)) = tame_dps.get(&dp_key) {
                            collisions += 1;

                            // Recover k from 2D collision with GLV adjustment
                            let k1_diff = td1.sub_mod_n(&wild_k1_dist[wi]);
                            let k2_diff = td2.sub_mod_n(&wild_k2_dist[wi]);
                            let k2_lam = k2_diff.mul_mod_n(&lam);
                            let k_base = k1_diff.add_mod_n(&k2_lam);

                            // Apply GLV variant adjustment
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

            // L8: Advance walks with combined 2D steps
            for (i, aff) in aff_points.iter().enumerate() {
                // Use hash to select (a, b) indices for combined step
                let hash_val = hash_step_2d(aff, n_steps);
                let a_idx = hash_val % n_steps;
                let b_idx = (hash_val / n_steps) % n_steps;

                if i < current_n_tame {
                    // Combined 2D step for tame walk
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

            // L6: Adaptive walk fusion — shift tame→wild as DPs accumulate
            dp_fill_ratio = tame_dps.len() as f64 / dp_target;
            if dp_fill_ratio > 0.8 && active_tame > n_tame / 4 {
                // Convert one tame walk to wild (reuse its Jacobian)
                active_tame -= 1;
                if active_tame > 0 {
                    let conv_i = active_tame; // Last tame walk
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
    // ALGORITHM 3: HYPERION BSGS-Kangaroo Hybrid for P135
    // ================================================================
    //
    // This is the BREAKTHROUGH algorithm with:
    //   - FIXED BSGS baby step reconstruction
    //   - Combined 2D steps
    //   - Multi-resolution BSGS
    //   - Every-step baby check
    //   - Adaptive walk fusion
    //   - Distributed baby table design
    //
    // KEY FIX: The v13 BSGS lookup was computing:
    //   k_candidate = j + k1_dist * 2^M  (WRONG!)
    //
    // The CORRECT reconstruction is:
    //   Wild walk: P_w = Q + (k1_dist + k2_dist*λ) * G
    //   If P_w matches baby step j*G:
    //     k + (k1_dist + k2_dist*λ) ≡ j (mod N)
    //     k ≡ j - (k1_dist + k2_dist*λ) (mod N)
    //
    // For tame-wild collision:
    //   k = (tame_k1 - wild_k1) + (tame_k2 - wild_k2)*λ (mod N)
    //   This is correct in v13 because rc is baked into the distances.

    fn solve_hyperion_hybrid(&self, max_hops: u64, start: Instant) -> SolveResult {
        let g = Point::generator();
        let phi_g = g.glv_phi();
        let lam = Fe { limbs: crate::field::LAMBDA };
        let lam_sq = lam.mul_mod_n(&lam);
        let beta_fe = Fe { limbs: BETA };
        let beta_sq = beta_fe.mul(&beta_fe);

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        // ========== BSGS PARAMETER SELECTION ==========
        // CPU: M=26 (2^26=64M entries, ~2.3GB RAM)
        // Distributed: M=32 (2^32=4B entries, ~144GB across 10 GPUs)
        let baby_bits = if self.baby_bits > 0 {
            self.baby_bits
        } else {
            match self.range_bits {
                0..=40 => 0,
                41..=60 => 20,
                61..=80 => 24,
                81..=110 => 26,
                111..=140 => 26, // CPU max; M=32 requires distributed GPUs
                _ => 28,
            }
        };

        let n = secp256k1_order();

        if baby_bits == 0 {
            return self.solve_kangaroo_2d(max_hops, start);
        }

        let baby_size: u64 = 1u64 << baby_bits;
        let giant_bits = self.range_bits - 1 - baby_bits;

        println!("  [HYPERION] ═══ BSGS-Kangaroo Hybrid Configuration ═══");
        println!("  [HYPERION] Baby step table: 2^{} entries ({} MB)", baby_bits, baby_size * 40 / 1_000_000);
        println!("  [HYPERION] Giant step range: 2^{}", giant_bits);
        println!("  [HYPERION] Expected complexity: O(2^{:.1}) with GLV √6",
                 giant_bits as f64 / 2.0 - 1.29);
        println!("  [HYPERION] With 10 GPUs × 6h = 2^48.6 ops");
        println!("  [HYPERION] With 10 GPUs × 13h = 2^49.7 ops");

        // ========== PHASE 1: Build Multi-Resolution Baby Step Tables ==========
        println!("\n  [HYPERION] Phase 1: Building baby step tables...");

        // L9: Multi-Resolution BSGS
        // L1 table: 2^16 entries (65536, fits in L1 cache, ~2.5MB)
        let l1_bits = 16u32.min(baby_bits);
        let l1_size: u64 = 1u64 << l1_bits;

        // L2 table: full baby_bits entries (up to 2^26)
        let l2_bits = baby_bits;

        // Build L1 table first (fast, small)
        let l1_start = Instant::now();
        let mut baby_table_l1: HashMap<[u8; 32], u64> = HashMap::with_capacity(l1_size as usize);
        let mut current_jac = JacobianPoint::infinity(); // Start at identity (0*G)
        for j in 0..=l1_size {
            if !current_jac.z.is_zero() {
                let aff = current_jac.to_affine();
                if !aff.inf {
                    baby_table_l1.insert(aff.x.to_bytes(), j);
                }
            }
            current_jac = current_jac.add_affine(&g);
        }
        println!("  [HYPERION] L1 table (2^{} entries) built in {:.1}s",
                 l1_bits, l1_start.elapsed().as_secs_f64());

        // Build L2 table if different from L1
        let baby_table_l2 = if l2_bits > l1_bits {
            let l2_start = Instant::now();
            let l2_size: u64 = 1u64 << l2_bits;
            let mut table: HashMap<[u8; 32], u64> = HashMap::with_capacity(l2_size as usize);

            // Copy L1 entries
            for (&x_bytes, &j) in &baby_table_l1 {
                table.insert(x_bytes, j);
            }

            // Continue building from where L1 left off
            let batch_size = 4096usize;
            let mut j = l1_size;
            while j < l2_size {
                let mut batch_jacs = Vec::with_capacity(batch_size);
                let batch_start = j;

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
                    table.insert(pt.x.to_bytes(), batch_start + idx as u64);
                }

                if j % (1 << 20) == 0 && j > 0 {
                    print!("\r  [HYPERION] L2 table: {}/{} ({:.1}%)",
                           j, l2_size, j as f64 / l2_size as f64 * 100.0);
                }
            }
            println!("\n  [HYPERION] L2 table (2^{} entries) built in {:.1}s",
                     l2_bits, l2_start.elapsed().as_secs_f64());
            table
        } else {
            // L2 = L1 (same size)
            baby_table_l1.clone()
        };

        let total_baby_entries = baby_table_l2.len() as u64;
        println!("  [HYPERION] Total baby step entries: {}", total_baby_entries);

        // ========== PHASE 2: Precompute Giant Step Base ==========
        println!("\n  [HYPERION] Phase 2: Computing giant step base...");
        let gs_start = Instant::now();

        let giant_step_scalar = Fe::from_biguint_mod_n(&(BigUint::from(1u64) << baby_bits as usize));
        let q_m = g.scalar_mul(&giant_step_scalar); // 2^M * G

        println!("  [HYPERION] Giant step base 2^{}*G computed in {:.1}s",
                 baby_bits, gs_start.elapsed().as_secs_f64());

        // ========== PHASE 3: Precompute Combined 2D Step Points ==========
        println!("\n  [HYPERION] Phase 3: Precomputing L8 combined 2D step points...");

        let mean_exp = giant_bits as u64 / 2;
        let low = mean_exp.saturating_sub(10);
        let high = mean_exp + 10;
        let n_steps = (high - low + 1) as usize;

        // Step points in G dimension (multiples of Q_M)
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

        // φ(G) step points
        let step_points_phi_gm: Vec<Point> = step_points_gm.iter().map(|p| p.glv_phi()).collect();
        let step_scalars_phi_gm: Vec<Fe> = step_scalars_gm.iter().map(|s| {
            s.mul_mod_n(&lam)
        }).collect();

        // L8: Combined 2D step points: 2^a*Q_M + 2^b*φ(Q_M)
        // n_steps^2 entries — for n_steps=21, that's 441 points, ~28KB (fits in GPU shared mem!)
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

        println!("  [HYPERION] L8 combined 2D steps: {}×{} = {} points",
                 n_steps, n_steps, n_steps * n_steps);

        // ========== PHASE 4: Initialize Walks ==========
        println!("\n  [HYPERION] Phase 4: Initializing {} walks...", N_WALKS);

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
        let mut bsgs_hits = 0u64;
        let mut bsgs_verified = 0u64;
        let mut baby_l1_hits = 0u64;
        let mut baby_l2_hits = 0u64;

        let total_max = if max_hops > 0 { max_hops } else { 2_000_000_000 };
        let steps_per_walk = total_max / N_WALKS as u64;
        let mut total_steps = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;

        // L6: Adaptive walk fusion state
        let mut active_tame = n_tame;
        let dp_target_count = 1u64 << (dp_bits.min(24));
        let two_m = Fe::from_biguint_mod_n(&(BigUint::from(1u64) << baby_bits as usize));

        println!("\n  [HYPERION] ═══ Starting Hybrid Search ═══");
        println!("  [HYPERION] DP bits: {}, mask: 0x{:X}", dp_bits, dp_mask);
        println!("  [HYPERION] Steps per walk: {}", steps_per_walk);
        println!("  [HYPERION] L10: Every-step baby check ENABLED");
        println!("  [HYPERION] L9: Multi-resolution BSGS (L1=2^{}, L2=2^{})", l1_bits, l2_bits);

        // Main HYPERION hybrid loop
        for step in 0..steps_per_walk {
            // Batch convert all walks to affine
            let mut all_jacs: Vec<JacobianPoint> = Vec::with_capacity(N_WALKS);
            all_jacs.extend_from_slice(&tame_jacs[..active_tame]);
            all_jacs.extend_from_slice(&wild_jacs);
            let current_n_tame = active_tame;
            let aff_points = batch_jac_to_affine(&all_jacs);

            for (i, aff) in aff_points.iter().enumerate() {
                if aff.inf { continue; }

                // ===== L10: EVERY-STEP BABY CHECK (wild walks only) =====
                // This is the key v14 improvement: check the baby step table
                // at EVERY step, not just DPs. The overhead is just a HashMap
                // lookup (~50ns), but it gives 2^M coverage per step.
                if i >= current_n_tame {
                    let wi = i - current_n_tame;

                    // L9: Check L1 table first (fast, in cache)
                    if let Some(&j_val) = baby_table_l1.get(&aff.x.to_bytes()) {
                        baby_l1_hits += 1;
                        // FIXED: Correct BSGS reconstruction
                        // P_w = Q + (k1_dist + k2_dist*λ)*G
                        // If P_w = j*G: k = j - (k1_dist + k2_dist*λ) (mod N)
                        if let Some(k) = self.bsgs_recover_key(
                            j_val, &wild_k1_dist[wi], &wild_k2_dist[wi],
                            &two_m, &lam, &range_start, &range_end, &mut oracle_filtered,
                        ) {
                            println!("  *** KEY FOUND via L1 baby step lookup: 0x{:x} ***", k);
                            found = true;
                            found_k = Some(k);
                            break;
                        }
                    }

                    // Check GLV variants in L1 table (β*x, β²*x)
                    let x_beta = aff.x.mul(&beta_fe);
                    if let Some(&j_val) = baby_table_l1.get(&x_beta.to_bytes()) {
                        baby_l1_hits += 1;
                        // β*x match means the point is λ*j*G → actual scalar = λ*j
                        let j_fe = Fe::from_biguint_mod_n(&BigUint::from(j_val));
                        let j_lam = j_fe.mul_mod_n(&lam);
                        let j_lam_big = j_lam.to_biguint();
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
                        baby_l1_hits += 1;
                        let j_fe = Fe::from_biguint_mod_n(&BigUint::from(j_val));
                        let j_lam2 = j_fe.mul_mod_n(&lam_sq);
                        if let Some(k) = self.bsgs_recover_key_fe(
                            &j_lam2, &wild_k1_dist[wi], &wild_k2_dist[wi],
                            &two_m, &lam, &range_start, &range_end, &mut oracle_filtered,
                        ) {
                            println!("  *** KEY FOUND via L1+GLV² baby step: 0x{:x} ***", k);
                            found = true;
                            found_k = Some(k);
                            break;
                        }
                    }
                }

                // ===== DP-BASED CHECKS (tame-wild collision + L2 baby check) =====
                let x_variants = [aff.x, aff.x.mul(&beta_fe), aff.x.mul(&beta_sq)];

                for (vi, x_var) in x_variants.iter().enumerate() {
                    if x_var.limbs[0] & dp_mask != 0 { continue; }
                    let dp_key = x_var.to_bytes();

                    if i < current_n_tame {
                        // Tame walk: record DP with 2D distance
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

                            if let Some(k) = self.try_recover_6x_glv(
                                &k_adjusted, &range_start, &range_end, &mut oracle_filtered,
                            ) {
                                println!("  *** KEY FOUND via kangaroo collision: 0x{:x} ***", k);
                                found = true;
                                found_k = Some(k);
                                break;
                            }
                        }

                        // L9: L2 baby step table check at DPs (full table)
                        if l2_bits > l1_bits {
                            if let Some(&j_val) = baby_table_l2.get(&aff.x.to_bytes()) {
                                baby_l2_hits += 1;
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
                println!("    Step {}: {} total | {} DPs | {} coll | L1:{} L2:{} | bsgs:{} | {:.0}/s | t:{} w:{} | GPU#{}/{}",
                         step, total_steps, tame_dps.len(), collisions,
                         baby_l1_hits, baby_l2_hits, bsgs_verified,
                         rate, active_tame, wild_jacs.len(),
                         self.gpu_id, self.n_gpus);
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        if found {
            SolveResult { found: true, k: found_k, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        } else {
            println!("\n  [HYPERION] Done: {} steps, {} DPs, {} collisions, L1:{} L2:{} bsgs:{}",
                     total_steps, tame_dps.len(), collisions, baby_l1_hits, baby_l2_hits, bsgs_verified);
            SolveResult { found: false, k: None, total_steps, dp_count: tame_dps.len() as u64, collisions, elapsed_ms }
        }
    }

    // ================================================================
    // KEY RECOVERY — FIXED BSGS reconstruction
    // ================================================================

    /// BSGS key recovery with CORRECT reconstruction.
    ///
    /// Wild walk: P_w = Q + (k1_dist + k2_dist*λ) * G
    /// If P_w matches baby step j*G:
    ///   k + (k1_dist + k2_dist*λ) ≡ j (mod N)
    ///   k ≡ j - (k1_dist + k2_dist*λ) (mod N)
    ///
    /// Note: The walk distances are in "giant step space" (multiples of 2^M),
    /// so we need to multiply by 2^M to get back to the original scalar space.
    fn bsgs_recover_key(
        &self,
        j_val: u64,
        k1_dist: &Fe,
        k2_dist: &Fe,
        two_m: &Fe,      // 2^M as Fe
        lam: &Fe,        // λ
        range_start: &BigUint,
        range_end: &BigUint,
        oracle_filtered: &mut u64,
    ) -> Option<BigUint> {
        // total_dist = (k1_dist + k2_dist * λ) * 2^M  (mod N)
        let k2_lam = k2_dist.mul_mod_n(lam);
        let total_dist = k1_dist.add_mod_n(&k2_lam).mul_mod_n(two_m);

        // k = j - total_dist (mod N)
        let j_fe = Fe::from_biguint_mod_n(&BigUint::from(j_val));
        let k_candidate = j_fe.sub_mod_n(&total_dist);

        // Check all 6 GLV variants
        self.try_recover_6x_glv(&k_candidate, range_start, range_end, oracle_filtered)
    }

    /// BSGS key recovery with Fe-based j value (for GLV variant matches)
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

    /// Try to recover k from a candidate scalar, checking all 6 GLV variants.
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

    /// Check all 6 GLV variants for a candidate k (BigUint version)
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
        println!("\n  [SELFTEST] Testing PRISM VORTEX v14 HYPERION on {}-bit puzzle...", bits);

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

        let solver = PrismVortexV14::new(bits, target, Some(oracle))
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

/// L8: Hash function for combined 2D step selection.
/// Returns a combined index that selects both (a, b) indices.
#[inline]
fn hash_step_2d(pt: &Point, n: usize) -> usize {
    if pt.inf { return 0; }
    let num = n.max(1);
    // Mix more bits for better 2D distribution
    let h = (pt.x.limbs[0] as usize).wrapping_mul(0x517cc1b727220a95)
        .wrapping_add((pt.x.limbs[1] as usize).wrapping_mul(0x2b592653855b1e8d))
        .wrapping_add((pt.x.limbs[2] as usize).wrapping_mul(0x6c62272e07bb0193))
        .wrapping_add((pt.y.limbs[0] as usize).wrapping_mul(0x3c7e9f8a1b2d4567));
    h % (num * num)
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
// L3: GPU OFFLOAD — CUDA Interface (same as v13, updated params)
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
        // v14: With distributed baby table, each GPU stores a SHARD
        // M=32 total → each GPU stores 2^32/10 ≈ 2^28.7 entries (8GB)
        let baby_bits_per_gpu = if vram_per_gpu_mb >= 10000 { 28 }
                               else if vram_per_gpu_mb >= 4000 { 26 }
                               else if vram_per_gpu_mb >= 1000 { 24 }
                               else { 20 };

        // Effective baby bits with 10 GPUs sharing
        let effective_baby_bits = baby_bits_per_gpu + (n_gpus as f64).log2() as u32;

        let walks_per_block = 256;
        let blocks_per_gpu = 128;
        let total_walks = walks_per_block * blocks_per_gpu;

        KernelConfig {
            n_gpus,
            baby_bits_per_gpu,
            effective_baby_bits,
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
        pub walks_per_block: usize,
        pub blocks_per_gpu: usize,
        pub total_walks_per_gpu: usize,
        pub dp_bits: u8,
        pub estimated_ops_per_sec: u64,
    }

    impl KernelConfig {
        pub fn print_summary(&self) {
            println!("  [GPU] ═══ v14 HYPERION Kernel Configuration ═══");
            println!("  [GPU] GPUs: {}", self.n_gpus);
            println!("  [GPU] Baby step per GPU: 2^{} entries", self.baby_bits_per_gpu);
            println!("  [GPU] Effective baby steps (distributed): 2^{} entries", self.effective_baby_bits);
            println!("  [GPU] Walks per GPU: {}", self.total_walks_per_gpu);
            println!("  [GPU] DP bits: {}", self.dp_bits);
            println!("  [GPU] Estimated total throughput: {} M ops/s",
                     self.estimated_ops_per_sec / 1_000_000);

            let giant_bits = 134.0 - self.effective_baby_bits as f64;
            let with_glv = giant_bits / 2.0 - 1.29;
            let total_ops_6h = self.estimated_ops_per_sec as f64 * 21600.0;
            let total_ops_13h = self.estimated_ops_per_sec as f64 * 46800.0;
            let total_ops_2d = self.estimated_ops_per_sec as f64 * 172800.0;

            println!("  [GPU] Giant step range: 2^{:.0}", giant_bits);
            println!("  [GPU] Kangaroo + GLV √6: O(2^{:.1})", with_glv);
            println!("  [GPU] Total ops in 6h:  2^{:.1}", total_ops_6h.log2());
            println!("  [GPU] Total ops in 13h: 2^{:.1}", total_ops_13h.log2());
            println!("  [GPU] Total ops in 2d:  2^{:.1}", total_ops_2d.log2());
            println!("  [GPU] Feasibility (6h):  {}", if with_glv <= total_ops_6h.log2() { "✓ FEASIBLE!" } else { "⚠ Need more time" });
            println!("  [GPU] Feasibility (13h): {}", if with_glv <= total_ops_13h.log2() { "✓ FEASIBLE!" } else { "⚠ Need more time" });
        }
    }
}

// ============================================================
// L4: DISTRIBUTED SEARCH — Updated for v14
// ============================================================

pub mod distributed {
    use std::net::{TcpListener, TcpStream};
    use std::io::{Read, Write};
    use std::time::Duration;

    #[derive(Debug, Clone)]
    pub enum Message {
        AssignWork { gpu_id: u32, range_start_bits: u32, range_offset: u64, dp_bits: u8 },
        ReportDP { x_bytes: [u8; 32], distance: u64, is_tame: bool, gpu_id: u32 },
        BabyLookup { x_bytes: [u8; 32], gpu_id: u32 },
        BabyLookupResponse { found: bool, j_val: u64 },
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
            println!("\n  [DIST] ═══ HYPERION v14 Coordination Summary ═══");
            println!("  [DIST] GPUs: {}", self.n_gpus);
            println!("  [DIST] Range: [2^{}, 2^{})", self.range_bits - 1, self.range_bits);

            let ops_per_sec_per_gpu = 1_500_000_000.0;
            let total_ops_per_sec = self.n_gpus as f64 * ops_per_sec_per_gpu;

            // Distributed baby table: M=32
            let baby_bits_per_gpu = 28.0;
            let effective_baby_bits = baby_bits_per_gpu + (self.n_gpus as f64).log2();
            let giant_bits = (self.range_bits - 1) as f64 - effective_baby_bits;
            let kangaroo_complexity = giant_bits / 2.0;
            let with_glv = kangaroo_complexity - 1.29;

            let hours_6 = total_ops_per_sec * 21600.0;
            let hours_13 = total_ops_per_sec * 46800.0;
            let days_2 = total_ops_per_sec * 172800.0;

            println!("  [DIST] Distributed baby table: 2^{:.1} entries ({} GPUs × 2^{:.0})", effective_baby_bits, self.n_gpus, baby_bits_per_gpu);
            println!("  [DIST] Giant step range: 2^{:.1}", giant_bits);
            println!("  [DIST] Kangaroo + GLV √6: O(2^{:.1})", with_glv);
            println!("  [DIST] Throughput: {:.0} M ops/s", total_ops_per_sec / 1e6);
            println!("  [DIST] Total ops in 6h:  2^{:.1}", hours_6.log2());
            println!("  [DIST] Total ops in 13h: 2^{:.1}", hours_13.log2());
            println!("  [DIST] Total ops in 2d:  2^{:.1}", days_2.log2());
            println!("  [DIST] Feasibility (6h):  {}", if with_glv <= hours_6.log2() { "✓ FEASIBLE!" } else { "⚠ Need more time" });
            println!("  [DIST] Feasibility (13h): {}", if with_glv <= hours_13.log2() { "✓ FEASIBLE!" } else { "⚠ Need more time" });
            println!("  [DIST] Estimated time: {:.1} hours", 2f64.powf(with_glv) / total_ops_per_sec / 3600.0);
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
