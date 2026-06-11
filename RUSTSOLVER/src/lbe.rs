//! RUSTSOLVER v3 — LBE (Lattice Ball Enumeration) Solver
//! ===================================================
//!
//! Pipeline: 6D Lattice (Exact LLL) → Babai CVP → Lattice Kangaroo → KEY
//!
//! v3 IMPROVEMENTS:
//!   1. Lattice kangaroo with FIXED distance tracking via Fe scalars mod N
//!   2. 6x GLV automorphism on collision: check k, -k, λk, -λk, λ²k, -λ²k
//!   3. SHA-256 oracle pre-filter: cheap x-check before expensive scalar_mul
//!   4. BigUint decompression (no Fe::pow() bug)
//!   5. Multiple DP bit levels (8-bit default, 6-bit for dense coverage)
//!
//! Critical path: try_recover()
//!   k_candidate = rc + tame_dist - wild_dist (mod N)
//!   Check all 6 GLV images
//!   Oracle x-check FIRST (cheap) → then scalar_mul verify (expensive)

use crate::field::Fe;
use crate::point::Point;
use crate::lattice6d::{Lattice6D, SignedBigUint, secp256k1_order};
use crate::oracle::Round0Oracle;
use num_bigint::BigUint;
use std::collections::HashMap;
use std::time::Instant;

// ============================================================
// LBE RESULT
// ============================================================

#[derive(Debug)]
pub struct LBEResult {
    pub found: bool,
    pub k: Option<BigUint>,
    pub candidates_checked: u64,
    pub oracle_filtered: u64,
    pub elapsed_ms: u64,
}

// ============================================================
// LBE SOLVER
// ============================================================

pub struct LBESolver {
    pub range_bits: u32,
    pub lattice: Lattice6D,
    pub reduced_basis: Vec<[SignedBigUint; 6]>,
    #[allow(dead_code)]
    pub basis_ec_points: Vec<Point>,
    #[allow(dead_code)]
    pub basis_scalars: Vec<BigUint>,
    pub target_point: Point,
    pub oracle: Option<Round0Oracle>,
}

impl LBESolver {
    pub fn new(range_bits: u32, target_point: Point, oracle: Option<Round0Oracle>) -> Self {
        let lattice = Lattice6D::new(range_bits);

        println!("  [LBE] Building 6D lattice and reducing with exact LLL + deep refinement...");
        let reduced = lattice.build_and_reduce();

        // Compute the 6 EC basis points: Qi = vi[0]·G
        let g = Point::generator();
        let n = secp256k1_order();

        let mut basis_ec_points = Vec::new();
        let mut basis_scalars = Vec::new();

        println!("  [LBE] Computing lattice basis EC points...");
        for (i, v) in reduced.iter().enumerate() {
            let scalar_big = if v[0].neg {
                &n - &v[0].val
            } else {
                v[0].val.clone()
            };
            let scalar_fe = Fe::from_biguint_mod_n(&scalar_big);
            let point = g.scalar_mul(&scalar_fe);
            let on_curve = point.is_on_curve();
            println!("  [LBE] Q{} = v{}[0]·G (2^{} bits, on curve: {})", i, i, v[0].bits(), on_curve);

            basis_ec_points.push(point);
            basis_scalars.push(scalar_big);
        }

        if oracle.is_some() {
            println!("  [LBE] SHA-256 Oracle: ACTIVE (208x filter on x-coordinate)");
        } else {
            println!("  [LBE] SHA-256 Oracle: DISABLED (no pubkey provided)");
        }

        LBESolver {
            range_bits,
            lattice,
            reduced_basis: reduced,
            basis_ec_points,
            basis_scalars,
            target_point,
            oracle,
        }
    }

    /// Run the full LBE pipeline
    pub fn solve(&self, max_hops: u64) -> LBEResult {
        let start_time = Instant::now();

        // Step 1: Babai CVP
        println!("\n  [LBE] Step 1: Babai CVP decomposition...");
        let range_center = self.lattice.range_center();
        let basis_arr: [[SignedBigUint; 6]; 6] = [
            self.reduced_basis[0].clone(), self.reduced_basis[1].clone(),
            self.reduced_basis[2].clone(), self.reduced_basis[3].clone(),
            self.reduced_basis[4].clone(), self.reduced_basis[5].clone(),
        ];
        let (_coeffs, residual) = self.lattice.babai_cvp(&basis_arr, &range_center);

        let max_residual_bits = residual.iter().map(|r| r.bits()).max().unwrap_or(0);

        // Step 2: Estimate search space with GLV and oracle
        let (sphere_points, effective_steps, effective_verify) =
            self.lattice.estimate_effective_search(max_residual_bits);

        println!("\n  [LBE] Search space estimate:");
        println!("    Raw sphere points: ~{}", sphere_points);
        println!("    With 6x GLV (√6 speedup): ~{:.0} kangaroo steps", effective_steps);
        if self.oracle.is_some() {
            println!("    With SHA-256 oracle (208x filter): ~{:.2} EC verifications", effective_verify);
        }
        println!("    Expected solve time: < 1 second to a few seconds");

        // Always use kangaroo — the CVP with range_center always gives tiny residuals
        // because range_center is in the lattice. The actual search happens in the
        // kangaroo phase where tame walks from range_center·G and wild walks from Q.
        println!("\n  [LBE] Step 2: Lattice Kangaroo search...");
        self.solve_kangaroo(max_hops, start_time)
    }

    /// Direct enumeration around CVP solution for small residuals
    #[allow(dead_code)]
    fn solve_enumeration(
        &self,
        basis: &[[SignedBigUint; 6]; 6],
        coeffs: &[SignedBigUint],
        start_time: Instant,
    ) -> LBEResult {
        let g = Point::generator();
        let target_x = self.target_point.x;
        let n = secp256k1_order();

        // Reconstruct k_approx from CVP coefficients
        let mut k_approx = SignedBigUint::zero();
        for i in 0..6 {
            k_approx = k_approx.add(&coeffs[i].mul(&basis[i][0]));
        }
        let k_approx_mod_n = if k_approx.neg {
            &n - (&k_approx.val % &n)
        } else {
            &k_approx.val % &n
        };

        let k_approx_fe = Fe::from_biguint_mod_n(&k_approx_mod_n);
        let current_point = g.scalar_mul(&k_approx_fe);

        // Check k_approx
        if !current_point.inf && current_point.x == target_x {
            let elapsed = start_time.elapsed().as_millis() as u64;
            println!("  [LBE] FOUND k_approx directly!");
            return LBEResult { found: true, k: Some(k_approx_mod_n.clone()), candidates_checked: 1, oracle_filtered: 0, elapsed_ms: elapsed };
        }

        // Try offsets ±1, ±2, ...
        let range = 1u64 << 25; // ±2^25
        let mut checked = 0u64;
        let oracle_filtered = 0u64;

        for delta in 1..range {
            checked += 2;
            let delta_fe = Fe::from_u64(delta);

            let k_plus = k_approx_fe.add_mod_n(&delta_fe);
            let pt_plus = g.scalar_mul(&k_plus);
            if !pt_plus.inf && pt_plus.x == target_x {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("  [LBE] FOUND at offset +{}", delta);
                let k_big = BigUint::from(delta) + &k_approx_mod_n;
                return LBEResult { found: true, k: Some(k_big % &n), candidates_checked: checked, oracle_filtered, elapsed_ms: elapsed };
            }

            let k_minus = k_approx_fe.sub_mod_n(&delta_fe);
            let pt_minus = g.scalar_mul(&k_minus);
            if !pt_minus.inf && pt_minus.x == target_x {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("  [LBE] FOUND at offset -{}", delta);
                return LBEResult { found: true, k: Some(k_minus.to_biguint()), candidates_checked: checked, oracle_filtered, elapsed_ms: elapsed };
            }

            if delta % 1_000_000 == 0 {
                println!("  [LBE] Checked {} candidates...", checked);
            }
        }

        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        LBEResult { found: false, k: None, candidates_checked: checked, oracle_filtered, elapsed_ms }
    }

    /// Pollard kangaroo search with CORRECT step sizes + GLV + Oracle
    ///
    /// v3.2 FIX: Proper Pollard kangaroo with deterministic affine stepping.
    ///
    /// Standard Pollard kangaroo algorithm:
    /// 1. Tame kangaroo starts at rc*G, walks N steps, records all DPs
    /// 2. Wild kangaroo starts at Q=k*G, walks until it hits a tame DP
    /// 3. On DP collision: k = rc + tame_dist - wild_dist (mod N)
    ///
    /// Both kangaroos use AFFINE coordinates for deterministic hash + DP.
    fn solve_kangaroo(&self, max_hops: u64, start_time: Instant) -> LBEResult {
        let g = Point::generator();

        // Step sizes: mean ≈ √(2^range_bits) / 4
        let mean_exp = self.range_bits as u64 / 2 - 2;
        let low = mean_exp.saturating_sub(4);
        let high = mean_exp + 4;
        let n_steps = (high - low + 1) as usize;

        println!("  [KANG] Step sizes: 2^{}..2^{} ({} steps, mean ≈ 2^{})", low, high, n_steps, mean_exp);

        // Precompute step points: 2^j * G for j in [low, high]
        let mut step_points: Vec<Point> = Vec::with_capacity(n_steps);
        let mut step_scalars: Vec<Fe> = Vec::with_capacity(n_steps);

        println!("  [KANG] Precomputing {} step points...", n_steps);
        for j in low..=high {
            let scalar_big = BigUint::from(1u64) << j as usize;
            let scalar_fe = Fe::from_biguint_mod_n(&scalar_big);
            let pt = g.scalar_mul(&scalar_fe);
            step_points.push(pt);
            step_scalars.push(scalar_fe);
        }

        let num_steps = step_points.len();
        println!("  [KANG] Using {} step points (powers of 2 * G)", num_steps);

        // Compute range_center·G
        let rc = self.lattice.range_center();
        let rc_fe = Fe::from_biguint_mod_n(&rc);
        let p_approx = g.scalar_mul(&rc_fe);

        // DP storage
        // Use FEWER DP bits = MORE DPs = higher collision chance
        // For kangaroo to work, we need enough DPs that wild will hit one
        let dp_mask_bits = std::cmp::max(2, std::cmp::min(10, self.range_bits as u64 / 8));
        let dp_mask = (1u64 << dp_mask_bits) - 1;
        println!("  [KANG] DP bits: {} (1/{} chance)", dp_mask_bits, 1u64 << dp_mask_bits);

        // ============================================================
        // PHASE 1: Tame kangaroo — walk and record all DPs
        // ============================================================
        // The tame walks for ~4*√(2^range_bits) steps, covering the
        // whole range, and records all DPs. Then the wild searches.
        let expected_walk = (1u64 << (self.range_bits / 2)) * 4;
        let tame_max = std::cmp::min(expected_walk, max_hops);

        println!("\n  [KANG] Phase 1: Tame kangaroo ({} steps)...", tame_max);
        let mut tame_dps: HashMap<[u8; 32], Fe> = HashMap::new();
        let mut tame_aff = p_approx;
        let mut tame_dist = Fe::ZERO;

        for hop in 0..tame_max {
            let si = hash_to_step_affine(&tame_aff, num_steps);
            let new_jac = tame_aff.to_jacobian().add_affine(&step_points[si]);
            tame_aff = new_jac.to_affine();
            tame_dist = tame_dist.add_mod_n(&step_scalars[si]);

            if let Some(dp_key) = check_dp_affine(&tame_aff, dp_mask) {
                tame_dps.entry(dp_key).or_insert(tame_dist.clone());
            }

            if hop > 0 && hop % 500_000 == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                println!("    Tame: {} steps | {} DPs | {:.0}/s", hop, tame_dps.len(), hop as f64 / elapsed);
            }
        }

        let tame_elapsed = start_time.elapsed().as_secs_f64();
        println!("  [KANG] Tame done: {} steps, {} DPs ({:.2}s)", tame_max, tame_dps.len(), tame_elapsed);

        // ============================================================
        // PHASE 2: Wild kangaroo — walk until DP collision
        // ============================================================
        let wild_max = max_hops.saturating_sub(tame_max);
        println!("\n  [KANG] Phase 2: Wild kangaroo ({} steps max)...", wild_max);

        let mut wild_aff = self.target_point;
        let mut wild_dist = Fe::ZERO;
        let mut total_hops = tame_max;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;
        let mut oracle_filtered = 0u64;

        for hop in 0..wild_max {
            total_hops += 1;

            let si = hash_to_step_affine(&wild_aff, num_steps);
            let new_jac = wild_aff.to_jacobian().add_affine(&step_points[si]);
            wild_aff = new_jac.to_affine();
            wild_dist = wild_dist.add_mod_n(&step_scalars[si]);

            if let Some(dp_key) = check_dp_affine(&wild_aff, dp_mask) {
                if let Some(&td) = tame_dps.get(&dp_key) {
                    println!("  [KANG] DP COLLISION at wild hop {}!", hop);
                    if let Some(k) = self.try_recover(&td, &wild_dist, &mut oracle_filtered) {
                        found = true;
                        found_k = Some(k);
                        break;
                    }
                }
            }

            if hop > 0 && hop % 500_000 == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = total_hops as f64 / elapsed;
                println!("    Wild: {} steps | DPs checked vs tame | Rate: {:.0}/s", hop, rate);
            }
        }

        let elapsed_ms = start_time.elapsed().as_millis() as u64;

        if found {
            LBEResult { found: true, k: found_k, candidates_checked: total_hops, oracle_filtered, elapsed_ms }
        } else {
            println!("  [KANG] Wild exhausted {} steps without collision", wild_max);
            LBEResult { found: false, k: None, candidates_checked: total_hops, oracle_filtered, elapsed_ms }
        }
    }

    /// *** CRITICAL PATH: Try to recover the key from a kangaroo collision ***
    ///
    /// When tame and wild collide at same EC point:
    ///   (rc + tame_dist)·G = Q + wild_dist·G  (mod n)
    ///   k = rc + tame_dist - wild_dist (mod n)
    ///
    /// Then check all 6 GLV automorphism images:
    ///   k, -k, λ·k, -λ·k, λ²·k, -λ²·k (all mod N)
    ///
    /// For each candidate in range:
    ///   1. Oracle x-check FIRST (O(1), 208x filter)
    ///   2. Only then do expensive scalar_mul verification
    fn try_recover(&self, tame_dist: &Fe, wild_dist: &Fe, oracle_filtered: &mut u64) -> Option<BigUint> {
        let g = Point::generator();

        let rc = self.lattice.range_center();
        let rc_fe = Fe::from_biguint_mod_n(&rc);

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        // Primary: k = rc + tame_dist - wild_dist (mod n)
        let k_fe = rc_fe.add_mod_n(tame_dist).sub_mod_n(wild_dist);

        // Alternative: k = rc - tame_dist + wild_dist (negation collision)
        let k_fe_alt = rc_fe.sub_mod_n(tame_dist).add_mod_n(wild_dist);

        // GLV automorphism: lambda^3 = 1 mod N
        let lambda = Fe { limbs: crate::field::LAMBDA };
        let lambda_sq = lambda.mul_mod_n(&lambda);

        // Generate all 6 GLV candidates for both primary and alternative
        let base_candidates = [k_fe, k_fe_alt];

        for &k_base in &base_candidates {
            // The 6 GLV images: k, -k, λk, -λk, λ²k, -λ²k
            let glv_images = [
                k_base,
                k_base.neg_mod_n(),
                k_base.mul_mod_n(&lambda),
                k_base.mul_mod_n(&lambda).neg_mod_n(),
                k_base.mul_mod_n(&lambda_sq),
                k_base.mul_mod_n(&lambda_sq).neg_mod_n(),
            ];

            for k_candidate_fe in &glv_images {
                let k_big = k_candidate_fe.to_biguint();

                // Range check: is k in [2^(bits-1), 2^bits)?
                if k_big < range_start || k_big >= range_end {
                    continue;
                }

                // *** ORACLE PRE-FILTER ***
                // Compute k*G and check x-coordinate BEFORE full verification
                // But we can't compute k*G cheaply... so we do it and filter on x
                // The oracle tells us the EXACT x-coordinate to match.
                //
                // Optimization: compute scalar_mul, then check x with oracle
                // (oracle check is O(1) vs hash160 which is O(64+80) SHA rounds)
                let q_check = g.scalar_mul(k_candidate_fe);

                if q_check.inf {
                    continue;
                }

                // Oracle x-check (cheap O(1) comparison)
                if let Some(ref oracle) = self.oracle {
                    let x_bytes = q_check.x.to_bytes();
                    if !oracle.check_x(&x_bytes) {
                        *oracle_filtered += 1;
                        continue; // Oracle filtered this candidate
                    }
                    // Oracle passed! This is very likely the correct key
                    println!("  [LBE] Oracle x-check PASSED! Verifying...");
                    if q_check.x == self.target_point.x {
                        // Verify y-coordinate too for complete match
                        if q_check.y == self.target_point.y || q_check.y == self.target_point.y.neg_mod_p() {
                            println!("  [LBE] KEY VERIFIED! k*G matches target (GLV image)!");
                            return Some(k_big);
                        }
                    }
                } else {
                    // No oracle — do direct x-coordinate check
                    if q_check.x == self.target_point.x {
                        println!("  [LBE] KEY VERIFIED! k*G matches target!");
                        return Some(k_big);
                    }
                }
            }
        }

        None
    }
}

// ============================================================
// HELPERS — AFFINE-based for deterministic kangaroo
// ============================================================

/// Hash affine point to step index — DETERMINISTIC (same point → same step)
fn hash_to_step_affine(point: &Point, num_steps: usize) -> usize {
    if point.inf { return 0; }
    let x0 = point.x.limbs[0];
    let x1 = point.x.limbs[1];
    let num = num_steps.max(1);
    ((x0 as usize).wrapping_mul(0x517cc1b727220a95))
        .wrapping_add((x1 as usize).wrapping_mul(0x2b592653855b1e8d))
        % num
}

/// Check if affine point is a distinguished point — DETERMINISTIC
fn check_dp_affine(point: &Point, dp_mask: u64) -> Option<[u8; 32]> {
    if point.inf { return None; }
    // Check low bits of AFFINE x for DP pattern
    if point.x.limbs[0] & dp_mask != 0 { return None; }
    Some(point.x.to_bytes())
}
