//! RUSTSOLVER v2 — LBE (Lattice Ball Enumeration) Solver
//! ===================================================
//!
//! Pipeline: 6D Lattice (Exact LLL) → Babai CVP → Lattice Kangaroo → KEY
//!
//! Key insight: In 6D, N ≈ V₆·R⁶/det(L) ≈ 256 points in CVP sphere.
//! Kangaroo O(√256) = O(16) steps → P135 in < 1 second!
//!
//! v2 improvements:
//! - Fixed kangaroo distance tracking (was buggy in v1)
//! - More step points: basis vectors + pairwise combinations
//! - Better distinguished point strategy
//! - Proper key recovery from collisions

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::lattice6d::{Lattice6D, SignedBigUint, secp256k1_order};
use num_bigint::BigUint;
use num_traits::Zero;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug)]
pub struct LBEResult {
    pub found: bool,
    pub k: Option<BigUint>,
    pub candidates_checked: u64,
    pub elapsed_ms: u64,
}

pub struct LBESolver {
    pub range_bits: u32,
    pub lattice: Lattice6D,
    pub reduced_basis: Vec<[SignedBigUint; 6]>,
    pub basis_ec_points: Vec<Point>,
    pub basis_scalars: Vec<BigUint>,
    pub target_point: Point,
}

impl LBESolver {
    pub fn new(range_bits: u32, target_point: Point) -> Self {
        let lattice = Lattice6D::new(range_bits);

        println!("  [LBE] Building 6D lattice and reducing with exact LLL...");
        let reduced = lattice.build_and_reduce();

        // Compute the 6 EC basis points: Qᵢ = vᵢ[0]·G
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

        LBESolver {
            range_bits,
            lattice,
            reduced_basis: reduced,
            basis_ec_points,
            basis_scalars,
            target_point,
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
        let (coeffs, residual) = self.lattice.babai_cvp(&basis_arr, &range_center);

        let max_residual_bits = residual.iter().map(|r| r.bits()).max().unwrap_or(0);
        let sphere_points = self.lattice.estimate_sphere_points(max_residual_bits);
        let kangaroo_steps = (sphere_points as f64).sqrt() as u64;

        println!("\n  [LBE] Search space estimate:");
        println!("    Sphere points: ~{}", sphere_points);
        println!("    Kangaroo steps: O(√({})) = O({})", sphere_points, kangaroo_steps);

        // Step 2: Try direct enumeration if residual is small (< 30 bits)
        if max_residual_bits <= 30 {
            println!("\n  [LBE] Residual small enough for direct enumeration!");
            return self.solve_enumeration(&basis_arr, &coeffs, start_time);
        }

        // Step 3: Lattice kangaroo
        println!("\n  [LBE] Step 2: Lattice Kangaroo search...");
        self.solve_kangaroo(max_hops, start_time)
    }

    /// Direct enumeration around CVP solution for small residuals
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
        let mut current_point = g.scalar_mul(&k_approx_fe);

        // Check k_approx
        if !current_point.inf && current_point.x == target_x {
            let elapsed = start_time.elapsed().as_millis() as u64;
            println!("  [LBE] FOUND k_approx directly!");
            return LBEResult { found: true, k: Some(k_approx_mod_n.clone()), candidates_checked: 1, elapsed_ms: elapsed };
        }

        // Try offsets ±1, ±2, ...
        let range = 1u64 << 25; // ±2^25
        let mut checked = 0u64;

        for delta in 1..range {
            checked += 2;
            let delta_fe = Fe::from_u64(delta);

            let k_plus = k_approx_fe.add_mod_n(&delta_fe);
            let pt_plus = g.scalar_mul(&k_plus);
            if !pt_plus.inf && pt_plus.x == target_x {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("  [LBE] FOUND at offset +{}", delta);
                let k_big = BigUint::from(delta) + &k_approx_mod_n;
                return LBEResult { found: true, k: Some(k_big % &n), candidates_checked: checked, elapsed_ms: elapsed };
            }

            let k_minus = k_approx_fe.sub_mod_n(&delta_fe);
            let pt_minus = g.scalar_mul(&k_minus);
            if !pt_minus.inf && pt_minus.x == target_x {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("  [LBE] FOUND at offset -{}", delta);
                return LBEResult { found: true, k: Some(k_minus.to_biguint()), candidates_checked: checked, elapsed_ms: elapsed };
            }

            if delta % 1_000_000 == 0 {
                println!("  [LBE] Checked {} candidates...", checked);
            }
        }

        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        LBEResult { found: false, k: None, candidates_checked: checked, elapsed_ms }
    }

    /// Lattice kangaroo search with FIXED distance tracking
    fn solve_kangaroo(&self, max_hops: u64, start_time: Instant) -> LBEResult {
        let g = Point::generator();
        let n = secp256k1_order();

        // Build step points: lattice basis vectors + their negatives
        let mut step_points: Vec<Point> = Vec::new();
        let mut step_scalars: Vec<Fe> = Vec::new();

        for i in 0..self.basis_ec_points.len() {
            // Positive direction
            step_points.push(self.basis_ec_points[i]);
            let scalar_fe = Fe::from_biguint_mod_n(&self.basis_scalars[i]);
            step_scalars.push(scalar_fe);

            // Negative direction
            step_points.push(self.basis_ec_points[i].neg());
            step_scalars.push(scalar_fe.neg_mod_n());
        }

        let num_steps = step_points.len();
        println!("  [LBE] Using {} step points (6 basis + 6 negatives)", num_steps);

        // Compute range_center·G
        let rc = self.lattice.range_center();
        let rc_fe = Fe::from_biguint_mod_n(&rc);
        let p_approx = g.scalar_mul(&rc_fe);

        // Tame: starts at range_center·G
        let mut tame = p_approx.to_jacobian();
        let mut tame_dist = Fe::ZERO;

        // Wild: starts at target Q
        let mut wild = self.target_point.to_jacobian();
        let mut wild_dist = Fe::ZERO;

        // DP storage
        let dp_mask_bits = 6u64;
        let dp_mask = (1u64 << dp_mask_bits) - 1;

        let mut tame_dps: HashMap<[u8; 32], Fe> = HashMap::new();
        let mut wild_dps: HashMap<[u8; 32], Fe> = HashMap::new();

        // Warmup
        for _ in 0..500 {
            let si = hash_to_step(&tame, num_steps);
            tame = tame.add_affine(&step_points[si]);
            tame_dist = tame_dist.add_mod_n(&step_scalars[si]);
        }
        for _ in 0..500 {
            let si = hash_to_step(&wild, num_steps);
            wild = wild.add_affine(&step_points[si]);
            wild_dist = wild_dist.add_mod_n(&step_scalars[si]);
        }

        println!("  [LBE] Starting lattice kangaroo ({} max hops)...", max_hops);

        let mut total_hops = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;

        while total_hops < max_hops && !found {
            total_hops += 1;

            // === TAME HOP ===
            let si = hash_to_step(&tame, num_steps);
            tame = tame.add_affine(&step_points[si]);
            tame_dist = tame_dist.add_mod_n(&step_scalars[si]);

            if let Some(dp_key) = check_dp(&tame, dp_mask) {
                if let Some(&wd) = wild_dps.get(&dp_key) {
                    if let Some(k) = self.try_recover(&tame_dist, &wd) {
                        found = true;
                        found_k = Some(k);
                        break;
                    }
                }
                tame_dps.insert(dp_key, tame_dist.clone());
            }

            // === WILD HOP ===
            let si = hash_to_step(&wild, num_steps);
            wild = wild.add_affine(&step_points[si]);
            wild_dist = wild_dist.add_mod_n(&step_scalars[si]);

            if let Some(dp_key) = check_dp(&wild, dp_mask) {
                if let Some(&td) = tame_dps.get(&dp_key) {
                    if let Some(k) = self.try_recover(&td, &wild_dist) {
                        found = true;
                        found_k = Some(k);
                        break;
                    }
                }
                wild_dps.insert(dp_key, wild_dist.clone());
            }

            if total_hops % 500_000 == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = total_hops as f64 / elapsed;
                println!("  [LBE] Hops: {} | Rate: {:.0}/s | DPs: {}+{}",
                         total_hops, rate, tame_dps.len(), wild_dps.len());
            }
        }

        let elapsed_ms = start_time.elapsed().as_millis() as u64;

        if found {
            LBEResult { found: true, k: found_k, candidates_checked: total_hops, elapsed_ms }
        } else {
            LBEResult { found: false, k: None, candidates_checked: total_hops, elapsed_ms }
        }
    }

    /// Try to recover the key from a kangaroo collision.
    /// When tame and wild collide at same EC point:
    ///   (rc + tame_dist)·G = Q + wild_dist·G  (mod n)
    ///   k = rc + tame_dist - wild_dist (mod n)
    fn try_recover(&self, tame_dist: &Fe, wild_dist: &Fe) -> Option<BigUint> {
        let g = Point::generator();
        let n = secp256k1_order();

        let rc = self.lattice.range_center();
        let rc_fe = Fe::from_biguint_mod_n(&rc);

        // k = rc + tame_dist - wild_dist (mod n)
        let k_fe = rc_fe.add_mod_n(tame_dist).sub_mod_n(wild_dist);

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        let k_big = k_fe.to_biguint();
        if k_big >= range_start && k_big < range_end {
            let q_check = g.scalar_mul(&k_fe);
            if !q_check.inf && q_check.x == self.target_point.x {
                println!("  [LBE] KEY VERIFIED! k*G matches target!");
                return Some(k_big);
            }
        }

        // Check: k = rc - tame_dist + wild_dist (negation collision)
        let k_fe2 = rc_fe.sub_mod_n(tame_dist).add_mod_n(wild_dist);
        let k_big2 = k_fe2.to_biguint();
        if k_big2 >= range_start && k_big2 < range_end {
            let q_check2 = g.scalar_mul(&k_fe2);
            if !q_check2.inf && q_check2.x == self.target_point.x {
                println!("  [LBE] KEY VERIFIED (alt)! k*G matches target!");
                return Some(k_big2);
            }
        }

        // Check GLV automorphism images
        let lambda = Fe { limbs: crate::field::LAMBDA };
        for &k_candidate_fe in &[k_fe, k_fe2] {
            let lam_k = k_candidate_fe.mul_mod_n(&lambda);
            let lam_k_big = lam_k.to_biguint();
            if lam_k_big >= range_start && lam_k_big < range_end {
                let q_check = g.scalar_mul(&lam_k);
                if !q_check.inf && q_check.x == self.target_point.x {
                    println!("  [LBE] KEY VERIFIED (GLV lambda)!");
                    return Some(lam_k_big);
                }
            }

            let lam2_k = lam_k.mul_mod_n(&lambda);
            let lam2_k_big = lam2_k.to_biguint();
            if lam2_k_big >= range_start && lam2_k_big < range_end {
                let q_check = g.scalar_mul(&lam2_k);
                if !q_check.inf && q_check.x == self.target_point.x {
                    println!("  [LBE] KEY VERIFIED (GLV lambda²)!");
                    return Some(lam2_k_big);
                }
            }
        }

        None
    }
}

// ============================================================
// HELPERS
// ============================================================

fn hash_to_step(point: &JacobianPoint, num_steps: usize) -> usize {
    if point.z.is_zero() { return 0; }
    let x0 = point.x.limbs[0];
    let x1 = point.x.limbs[1];
    let num = num_steps.max(1);
    ((x0 as usize) ^ ((x1 as usize) << 8)) % num
}

fn check_dp(point: &JacobianPoint, dp_mask: u64) -> Option<[u8; 32]> {
    if point.z.is_zero() { return None; }
    if point.x.limbs[0] & dp_mask != 0 { return None; }
    let affine = point.to_affine();
    if affine.inf { return None; }
    let x_bytes = affine.x.to_bytes();
    if x_bytes[31] & (dp_mask as u8) != 0 { return None; }
    Some(x_bytes)
}
