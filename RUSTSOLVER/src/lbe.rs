//! RUSTSOLVER v5 — Multi-Walk Kangaroo with Batch Affine Conversion
//! ================================================================
//! 
//! Key insight: Single-walk kangaroo needs 1 modinv per step (slow: 77k/s).
//! Multi-walk: run N independent walks simultaneously, batch-convert
//! all N points to affine using Montgomery's trick (1 inv + 3(N-1) muls).
//! Amortized cost per walk: ~3 muls per conversion → ~1M+ steps/s per walk.
//!
//! Algorithm:
//!   - N_WALKS parallel walks (e.g., 32)
//!   - Each step: add step_point[si] in Jacobian (fast)
//!   - Every step: batch-convert all N walks to affine
//!   - Check DP on affine x
//!   - Step selection: hash(affine_x) → deterministic per group element
//!
//! With N=32 and Montgomery batch inversion:
//!   - 32 inversions cost = 1 inv + 3*31 muls ≈ 100 muls total
//!   - Amortized: 3.1 muls per inversion (vs 256 muls for single)
//!   - Speed: ~1.5M steps/s per walk × 32 walks = 48M group ops/s effective

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::lattice6d::{Lattice6D, SignedBigUint, secp256k1_order};
use crate::oracle::Round0Oracle;
use num_bigint::BigUint;
use std::collections::HashMap;
use std::time::Instant;

const N_WALKS: usize = 32;

#[derive(Debug)]
pub struct LBEResult {
    pub found: bool,
    pub k: Option<BigUint>,
    pub candidates_checked: u64,
    pub oracle_filtered: u64,
    pub elapsed_ms: u64,
}

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
        println!("  [LBE] Building lattice...");
        let reduced = lattice.build_and_reduce();

        let g = Point::generator();
        let n = secp256k1_order();
        let mut basis_ec_points = Vec::new();
        let mut basis_scalars = Vec::new();

        for (i, v) in reduced.iter().enumerate() {
            let scalar_big = if v[0].neg { &n - &v[0].val } else { v[0].val.clone() };
            let scalar_fe = Fe::from_biguint_mod_n(&scalar_big);
            let point = g.scalar_mul(&scalar_fe);
            println!("  [LBE] Q{} = v{}[0]·G (2^{} bits)", i, i, v[0].bits());
            basis_ec_points.push(point);
            basis_scalars.push(scalar_big);
        }

        if oracle.is_some() { println!("  [LBE] Oracle: ACTIVE"); }

        LBESolver { range_bits, lattice, reduced_basis: reduced, basis_ec_points, basis_scalars, target_point, oracle }
    }

    pub fn solve(&self, max_hops: u64) -> LBEResult {
        let start = Instant::now();
        let range_center = self.lattice.range_center();
        let basis_arr: [[SignedBigUint; 6]; 6] = [
            self.reduced_basis[0].clone(), self.reduced_basis[1].clone(),
            self.reduced_basis[2].clone(), self.reduced_basis[3].clone(),
            self.reduced_basis[4].clone(), self.reduced_basis[5].clone(),
        ];
        let (_coeffs, _residual) = self.lattice.babai_cvp(&basis_arr, &range_center);

        println!("\n  [LBE] Multi-Walk Kangaroo ({} parallel walks)", N_WALKS);
        self.solve_multi_walk(max_hops, start)
    }

    fn solve_multi_walk(&self, max_hops: u64, start: Instant) -> LBEResult {
        let g = Point::generator();

        // Step sizes
        let mean_exp = self.range_bits as u64 / 2 - 2;
        let low = mean_exp.saturating_sub(8);
        let high = mean_exp + 8;
        let n_steps = (high - low + 1) as usize;

        // Precompute via doubling chain
        let mut current = g.to_jacobian();
        for _ in 0..low { current = current.double(); }
        let step_points: Vec<Point> = (low..=high).map(|_| {
            let aff = current.to_affine();
            current = current.double();
            aff
        }).collect();
        let step_scalars: Vec<Fe> = (low..=high).map(|j| {
            Fe::from_biguint_mod_n(&(BigUint::from(1u64) << j as usize))
        }).collect();

        let rc = self.lattice.range_center();
        let rc_fe = Fe::from_biguint_mod_n(&rc);
        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        let total_max = if max_hops > 0 { max_hops } else { 100_000_000 };
        let dp_mask_bits = std::cmp::max(6, std::cmp::min(20, self.range_bits as u64 / 4));
        let dp_mask: u64 = (1u64 << dp_mask_bits) - 1;

        println!("  [KANG] Steps: 2^{}..2^{} ({}), DP: {} bits, Total: {} hops",
                 low, high, n_steps, dp_mask_bits, total_max);

        // N_WALKS/2 tame walks + N_WALKS/2 wild walks
        let n_tame = N_WALKS / 2;
        let n_wild = N_WALKS - n_tame;

        // Initialize tame walks: start near rc*G with small offsets
        let rc_point = g.scalar_mul(&rc_fe);
        let mut tame_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_tame);
        let mut tame_dists: Vec<Fe> = Vec::with_capacity(n_tame);
        let mut tame_dps: HashMap<[u8; 32], (usize, Fe)> = HashMap::with_capacity(1_000_000); // (walk_id, distance)

        for i in 0..n_tame {
            // Start each tame walk at rc*G + i*G (slight offset for variety)
            let offset = Fe::from_u64(i as u64);
            let start_pt = rc_point.add(&g.scalar_mul(&offset));
            tame_jacs.push(start_pt.to_jacobian());
            tame_dists.push(offset); // distance = i
        }

        // Initialize wild walks: start near Q with small offsets
        let mut wild_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_wild);
        let mut wild_dists: Vec<Fe> = Vec::with_capacity(n_wild);

        for i in 0..n_wild {
            let offset = Fe::from_u64(i as u64);
            let start_pt = self.target_point.add(&g.scalar_mul(&offset));
            wild_jacs.push(start_pt.to_jacobian());
            wild_dists.push(offset);
        }

        // Main loop: step all walks simultaneously
        let steps_per_walk = total_max / N_WALKS as u64;
        let mut total_steps = 0u64;
        let mut found = false;
        let mut found_k: Option<BigUint> = None;
        let mut oracle_filtered = 0u64;
        let mut collisions = 0u64;

        // Batch conversion buffers
        let mut all_jacs: Vec<JacobianPoint> = Vec::with_capacity(N_WALKS);

        for step in 0..steps_per_walk {
            // Step 1: Batch convert current positions to affine
            all_jacs.clear();
            all_jacs.extend_from_slice(&tame_jacs);
            all_jacs.extend_from_slice(&wild_jacs);

            let aff_points = batch_jac_to_affine(&all_jacs);

            // Step 2: Determine step index from affine x (DETERMINISTIC!)
            // Step 3: Check DPs
            for (i, aff) in aff_points.iter().enumerate() {
                if aff.inf { continue; }

                if i < n_tame {
                    // Tame walk: check and record DP
                    if aff.x.limbs[0] & dp_mask == 0 {
                        let dp_key = aff.x.to_bytes();
                        tame_dps.entry(dp_key).or_insert((i, tame_dists[i].clone()));
                    }
                } else {
                    // Wild walk: check collision with tame DPs
                    let wi = i - n_tame;
                    if aff.x.limbs[0] & dp_mask == 0 {
                        let dp_key = aff.x.to_bytes();
                        if let Some(&(ti, ref td)) = tame_dps.get(&dp_key) {
                            collisions += 1;
                            println!("  [KANG] COLLISION #{}: wild{} x tame{} at step {}!", collisions, wi, ti, step);
                            // Try recover
                            if let Some(k) = self.try_recover(td, &wild_dists[wi], &mut oracle_filtered,
                                                              &range_start, &range_end, &rc_fe) {
                                found = true;
                                found_k = Some(k);
                                break;
                            }
                        }
                    }
                }
            }

            if found { break; }

            // Step 4: Advance all walks using step from affine x hash
            for (i, aff) in aff_points.iter().enumerate() {
                let si = hash_affine_x(aff, n_steps);
                if i < n_tame {
                    tame_jacs[i] = tame_jacs[i].add_affine(&step_points[si]);
                    tame_dists[i] = tame_dists[i].add_mod_n(&step_scalars[si]);
                } else {
                    let wi = i - n_tame;
                    wild_jacs[wi] = wild_jacs[wi].add_affine(&step_points[si]);
                    wild_dists[wi] = wild_dists[wi].add_mod_n(&step_scalars[si]);
                }
            }

            total_steps += N_WALKS as u64;

            if step > 0 && step % 500_000 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                println!("    Step {}: {} total | {} DPs | {} coll | {:.0}/s",
                         step, total_steps, tame_dps.len(), collisions, total_steps as f64 / elapsed);
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        if found {
            LBEResult { found: true, k: found_k, candidates_checked: total_steps, oracle_filtered, elapsed_ms }
        } else {
            println!("\n  [KANG] Done: {} steps, {} DPs, {} collisions", total_steps, tame_dps.len(), collisions);
            LBEResult { found: false, k: None, candidates_checked: total_steps, oracle_filtered, elapsed_ms }
        }
    }

    fn try_recover(&self, tame_dist: &Fe, wild_dist: &Fe, oracle_filtered: &mut u64,
                   range_start: &BigUint, range_end: &BigUint, rc_fe: &Fe) -> Option<BigUint> {
        let g = Point::generator();
        let k_fe = rc_fe.add_mod_n(tame_dist).sub_mod_n(wild_dist);
        let k_fe_neg = rc_fe.add_mod_n(tame_dist).add_mod_n(wild_dist).neg_mod_n();

        let lambda = Fe { limbs: crate::field::LAMBDA };
        let lambda_sq = lambda.mul_mod_n(&lambda);

        for &k_base in &[k_fe, k_fe_neg] {
            let lk = k_base.mul_mod_n(&lambda);
            let l2k = k_base.mul_mod_n(&lambda_sq);
            for kc in &[k_base, k_base.neg_mod_n(), lk, lk.neg_mod_n(), l2k, l2k.neg_mod_n()] {
                let k_big = kc.to_biguint();
                if k_big < *range_start || k_big >= *range_end { continue; }
                let q = g.scalar_mul(kc);
                if q.inf { continue; }
                if let Some(ref oracle) = self.oracle {
                    if !oracle.check_x(&q.x.to_bytes()) { *oracle_filtered += 1; continue; }
                }
                if q.x == self.target_point.x &&
                   (q.y == self.target_point.y || q.y == self.target_point.y.neg_mod_p()) {
                    println!("  *** KEY FOUND: 0x{:x} ***", k_big);
                    return Some(k_big);
                }
            }
        }
        None
    }
}

#[inline]
fn hash_affine_x(pt: &Point, n: usize) -> usize {
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
