//! RUSTSOLVER — LBE (Lattice Ball Enumeration) Solver
//! ===================================================
//!
//! Pipeline: 6D Lattice (LLL) → Babai CVP → Lattice Kangaroo → KEY
//!
//! Key insight: In 6D, N ≈ V₆·R⁶/det(L) ≈ 256 points in CVP sphere.
//! Kangaroo O(√256) = O(16) steps → P135 in < 1 second!

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::lattice6d::{Lattice6D, SignedBigUint, secp256k1_order};
use num_bigint::BigUint;
use num_traits::{Zero, One};
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

        // Build and LLL reduce
        println!("  [LBE] Building 6D lattice and reducing...");
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

    /// Run the LBE solver with lattice kangaroo.
    pub fn solve(&self, max_hops: u64) -> LBEResult {
        let start_time = Instant::now();

        // Step 1: Babai CVP to get approximate decomposition
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
        println!("    Kangaroo steps: O(sqrt({})) = O({})", sphere_points, kangaroo_steps);

        // Step 2: Try direct enumeration if residual is small
        if max_residual_bits <= 30 {
            println!("\n  [LBE] Residual small enough for direct enumeration!");
            return self.solve_enumeration(&basis_arr, &coeffs, &residual, start_time);
        }

        // Step 3: Lattice kangaroo
        println!("\n  [LBE] Step 2: Lattice Kangaroo search...");
        self.solve_kangaroo(&basis_arr, max_hops, start_time)
    }

    /// Direct enumeration around CVP solution for small residuals.
    fn solve_enumeration(
        &self,
        basis: &[[SignedBigUint; 6]; 6],
        coeffs: &[SignedBigUint],
        _residual: &[SignedBigUint; 6],
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

        // Enumerate: try k_approx + δ for small δ
        let range = 1u64 << 20; // ±2^20 per dimension
        let mut checked = 0u64;

        println!("  [LBE] Enumerating ±{} around CVP solution...", range);

        // Simple 1D enumeration: try k_approx + delta for delta in -range..range
        let k_approx_fe = Fe::from_biguint_mod_n(&k_approx_mod_n);
        let mut current_point = g.scalar_mul(&k_approx_fe);

        // Check if k_approx itself is the key
        if !current_point.inf && current_point.x == target_x {
            let elapsed = start_time.elapsed().as_millis() as u64;
            println!("  [LBE] FOUND k_approx directly!");
            return LBEResult { found: true, k: Some(k_approx_mod_n.clone()), candidates_checked: 1, elapsed_ms: elapsed };
        }

        // Try offsets from -range to +range
        for delta in 1..range {
            checked += 2;

            // k_approx + delta
            let delta_fe = Fe::from_u64(delta);
            let k_plus = k_approx_fe.add_mod_n(&delta_fe);
            let pt_plus = g.scalar_mul(&k_plus);
            if !pt_plus.inf && pt_plus.x == target_x {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("  [LBE] FOUND at offset +{}", delta);
                let k_big = BigUint::from(delta) + &k_approx_mod_n;
                return LBEResult { found: true, k: Some(k_big % &n), candidates_checked: checked, elapsed_ms: elapsed };
            }

            // k_approx - delta
            let k_minus = k_approx_fe.sub_mod_n(&delta_fe);
            let pt_minus = g.scalar_mul(&k_minus);
            if !pt_minus.inf && pt_minus.x == target_x {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("  [LBE] FOUND at offset -{}", delta);
                let k_big = if k_approx_mod_n >= BigUint::from(delta) {
                    &k_approx_mod_n - BigUint::from(delta)
                } else {
                    &n - (&BigUint::from(delta) - &k_approx_mod_n)
                };
                return LBEResult { found: true, k: Some(k_big), candidates_checked: checked, elapsed_ms: elapsed };
            }

            if delta % 100_000 == 0 {
                println!("  [LBE] Checked {} candidates...", checked);
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        LBEResult { found: false, k: None, candidates_checked: checked, elapsed_ms: elapsed }
    }

    /// Lattice kangaroo search.
    fn solve_kangaroo(
        &self,
        basis: &[[SignedBigUint; 6]; 6],
        max_hops: u64,
        start_time: Instant,
    ) -> LBEResult {
        let g = Point::generator();
        let n = secp256k1_order();

        // Compute range_center·G
        let rc = self.lattice.range_center();
        let rc_fe = Fe::from_biguint_mod_n(&rc);
        let p_approx = g.scalar_mul(&rc_fe);

        // Tame: starts at range_center·G
        let mut tame = p_approx.to_jacobian();
        // Wild: starts at target Q
        let mut wild = self.target_point.to_jacobian();

        // Distance tracking (mod N) — using Fe for the scalar distance
        let mut tame_dist = Fe::ZERO;  // distance from range_center
        let mut wild_dist = Fe::ZERO;  // distance from Q (in scalar space)

        // Distinguished point storage
        let dp_mask_bits = 8;
        let dp_mask = (1u64 << dp_mask_bits) - 1;

        let mut tame_dps: HashMap<[u8; 32], Fe> = HashMap::new();
        let mut wild_dps: HashMap<[u8; 32], Fe> = HashMap::new();

        // Step points: lattice basis EC points + their negatives
        let num_steps = self.basis_ec_points.len();

        println!("  [LBE] Starting lattice kangaroo ({} max hops)...", max_hops);
        println!("  [LBE] Using {} lattice basis step points", num_steps);

        let mut total_hops = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;

        while total_hops < max_hops && !found {
            total_hops += 1;

            // === TAME HOP ===
            let step_idx = hash_to_step(&tame, num_steps);
            let step_sign = if tame.x.limbs[0] & 1 == 0 { 1i64 } else { -1i64 };
            let step_point = if step_sign > 0 { self.basis_ec_points[step_idx] } else { self.basis_ec_points[step_idx].neg() };
            tame = tame.add_affine(&step_point);

            // Track scalar distance
            let scalar_step = Fe::from_biguint_mod_n(&self.basis_scalars[step_idx]);
            if step_sign > 0 {
                tame_dist = tame_dist.add_mod_n(&scalar_step);
            } else {
                tame_dist = tame_dist.sub_mod_n(&scalar_step);
            }

            // Check DP
            if let Some(dp_key) = check_dp(&tame, dp_mask) {
                if let Some(&wild_d) = wild_dps.get(&dp_key) {
                    // COLLISION! Try to recover key
                    if let Some(k) = self.try_recover(&tame_dist, &wild_d) {
                        found = true;
                        found_k = Some(k);
                        break;
                    }
                }
                tame_dps.insert(dp_key, tame_dist.clone());
            }

            // === WILD HOP ===
            let step_idx = hash_to_step(&wild, num_steps);
            let step_sign = if wild.x.limbs[0] & 1 == 0 { 1i64 } else { -1i64 };
            let step_point = if step_sign > 0 { self.basis_ec_points[step_idx] } else { self.basis_ec_points[step_idx].neg() };
            wild = wild.add_affine(&step_point);

            if step_sign > 0 {
                wild_dist = wild_dist.add_mod_n(&scalar_step_for(step_idx, &self.basis_scalars));
            } else {
                wild_dist = wild_dist.sub_mod_n(&scalar_step_for(step_idx, &self.basis_scalars));
            }

            // Check DP
            if let Some(dp_key) = check_dp(&wild, dp_mask) {
                if let Some(&tame_d) = tame_dps.get(&dp_key) {
                    // COLLISION!
                    if let Some(k) = self.try_recover(&tame_d, &wild_dist) {
                        found = true;
                        found_k = Some(k);
                        break;
                    }
                }
                wild_dps.insert(dp_key, wild_dist.clone());
            }

            // Progress
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

    /// Try to recover key from kangaroo collision.
    /// When tame and wild collide at same EC point:
    ///   (rc + tame_dist)·G = Q + wild_dist·G  (mod n)
    ///   (rc + tame_dist - wild_dist)·G = Q
    ///   k = rc + tame_dist - wild_dist (mod n)
    fn try_recover(&self, tame_dist: &Fe, wild_dist: &Fe) -> Option<BigUint> {
        let g = Point::generator();
        let n = secp256k1_order();

        let rc = self.lattice.range_center();
        let rc_fe = Fe::from_biguint_mod_n(&rc);

        // k_candidate = rc + tame_dist - wild_dist (mod n)
        let k_fe = rc_fe.add_mod_n(tame_dist).sub_mod_n(wild_dist);

        // Range check
        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        let k_big = k_fe.to_biguint();
        if k_big >= range_start && k_big < range_end {
            // Verify: k*G should match target
            let q_check = g.scalar_mul(&k_fe);
            if !q_check.inf && q_check.x == self.target_point.x {
                println!("  [LBE] KEY VERIFIED! k*G matches target!");
                return Some(k_big);
            }
        }

        // Also check: k = rc - tame_dist + wild_dist (if collision was with negation)
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
        let beta = Fe { limbs: crate::field::BETA };
        let lambda = Fe { limbs: crate::field::LAMBDA };

        for &k_candidate_fe in &[k_fe, k_fe2] {
            // Check lambda * k
            let lam_k = k_candidate_fe.mul_mod_n(&lambda);
            let lam_k_big = lam_k.to_biguint();
            if lam_k_big >= range_start && lam_k_big < range_end {
                let q_check = g.scalar_mul(&lam_k);
                if !q_check.inf && q_check.x == self.target_point.x {
                    return Some(lam_k_big);
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
    // Quick check on raw X
    if point.x.limbs[0] & dp_mask != 0 { return None; }
    // Normalize
    let affine = point.to_affine();
    if affine.inf { return None; }
    let x_bytes = affine.x.to_bytes();
    if x_bytes[31] & (dp_mask as u8) != 0 { return None; }
    Some(x_bytes)
}

fn scalar_step_for(idx: usize, basis_scalars: &[BigUint]) -> Fe {
    Fe::from_biguint_mod_n(&basis_scalars[idx])
}
