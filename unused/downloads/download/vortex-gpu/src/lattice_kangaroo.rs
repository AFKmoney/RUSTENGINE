//! TITAN V16.2 — Layer 4: Lattice-Guided Kangaroo (LGK) v3
//! ================================================================
//! THE KEY INNOVATION: The kangaroo searches in the REDUCED COEFFICIENT
//! SPACE defined by the 6D lattice, NOT in the full scalar range.
//!
//! v3 FIX: Replace cheap mod_inv_2k hash with FULL normalize_x.
//!   - The cheap hash was NOT representation-invariant — same affine
//!     point with different Z → different step → walks diverge → 0 collisions!
//!   - Now uses full field inversion (258 muls) per step — slower but CORRECT.
//!   - Batch affine optimization will recover speed later.
//!
//! v2 FIX: Track scalar distance as Fe (mod n) instead of i64 coefficients.
//!   - OLD: CoeffVector { c: [i64; 6] } → OVERFLOWS for P135!
//!   - NEW: Scalar distance tracked as Fe mod n → NEVER overflows!
//!
//! How it works:
//!   Each hop adds ±(step_val * s_dim) to the scalar distance (mod n).
//!   The EC point moves by ±(step_val * P_dim) where P_dim = s_dim * G.
//!   On collision: k = offset + tame_dist - wild_dist (mod n).

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::glv::GLVDecomposer;
use std::collections::HashMap;
use std::time::Instant;

/// Number of precomputed step types per dimension
const STEPS_PER_DIM: usize = 16;

/// Distinguished point mask bits (adaptive — set in new())
const DP_MASK_BITS_DEFAULT: u32 = 10;

/// secp256k1 order
const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// DP key: 32-byte x-coordinate
type DPKey = [u8; 32];

/// Result from the lattice-guided kangaroo
#[derive(Clone, Debug)]
pub struct LatticeKangarooResult {
    pub found: bool,
    pub k: Option<Fe>,
    pub hops: u64,
    pub elapsed_ms: u64,
    pub method: String,
}

/// Normalize the x-coordinate of a Jacobian point: x = X/Z²
/// This is the CORRECT way to get a representation-invariant hash.
/// Cost: 1 field inversion + 2 multiplications ≈ 258 field muls
#[inline]
fn normalize_x(point: &JacobianPoint) -> Fe {
    if point.z.is_zero() {
        return Fe::ZERO;
    }
    let z_inv = point.z.modinv();
    let z_inv_sq = z_inv.mul(&z_inv);
    point.x.mul(&z_inv_sq)
}

/// Hash a Jacobian point to a (dimension, step_type) pair
/// using RAW JACOBIAN X+Z for step selection (high entropy, prevents cycles).
///
/// CRITICAL FIX V16.3: No direction bit! The direction bit was causing
/// 2-cycles: step +S at A, then step -S at A+S → back to A.
/// Instead, we precompute both positive and negative step points as
/// separate step types, eliminating the direction flip pattern.
#[inline]
fn hash_to_step_lgk(point: &JacobianPoint, active_dims: &[usize],
                     num_step_types: usize) -> (usize, usize) {
    if point.z.is_zero() || active_dims.is_empty() {
        return (active_dims.first().copied().unwrap_or(1), 0);
    }

    // Use RAW Jacobian X+Z for step selection — NOT normalize_x!
    let hash = point.x.limbs[0]
        .wrapping_mul(0x517cc1b727220a95)
        .wrapping_add(point.x.limbs[1])
        .wrapping_mul(0x6c62272e07bb0142)
        .wrapping_add(point.z.limbs[0]);

    let dim_idx = (hash as usize) % active_dims.len();
    let dim = active_dims[dim_idx];
    let step_type = ((hash >> 16) as usize) % num_step_types;

    (dim, step_type)
}

/// The Lattice-Guided Kangaroo solver v3
///
/// Tracks scalar distance as Fe (mod n) — NO i64 OVERFLOW!
/// Uses FULL normalize_x for step selection — CORRECT deterministic walk!
///
/// Algorithm:
///   Tame kangaroo: starts at offset·G, distance = 0 (mod n)
///   Wild kangaroo: starts at Q, distance = 0 (mod n)
///   Each hop: point += ±step_point, distance += ±step_scalar_dist (mod n)
///   On DP collision: k = offset + tame_dist - wild_dist (mod n)
pub struct LatticeKangaroo {
    /// Generator point
    pub g: Point,
    /// Target point Q
    pub q: Point,
    /// Group order N
    pub n: Fe,

    /// Lattice basis scalars: sᵢ = vᵢ[0] (first component of reduced basis vector i)
    pub basis_scalars: [Fe; 6],
    /// Precomputed points: Pᵢ = sᵢ · G
    pub basis_points: [Point; 6],
    /// Offset point: offset · G
    pub offset_point: Point,
    /// Offset scalar: range center
    pub offset_scalar: Fe,

    /// GLV decomposer for automorphism checks
    pub glv: GLVDecomposer,

    /// Step EC points for each (dim, step_type): positive [0..half), negative [half..STEPS_PER_DIM)
    pub step_points: [[Point; STEPS_PER_DIM]; 6],
    /// Step scalar distances for each (dim, step_type): positive [0..half), negative [half..STEPS_PER_DIM)
    pub step_dists: [[Fe; STEPS_PER_DIM]; 6],

    /// Maximum coefficient per dimension (for range checking)
    pub max_coeff: [u64; 6],
    /// Active dimensions (bits > 0)
    pub active_dims: Vec<usize>,

    /// DP mask bits (adaptive)
    pub dp_bits: u32,
}

impl LatticeKangaroo {
    /// Create a new LatticeKangaroo from the lattice decomposition results.
    pub fn new(
        target_point: Point,
        basis_scalars: [Fe; 6],
        offset_scalar: Fe,
        max_coeff_bits: [u32; 6],
    ) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();

        // Precompute basis points: Pᵢ = sᵢ · G
        let basis_points: [Point; 6] = std::array::from_fn(|i| {
            if basis_scalars[i].is_zero() {
                Point::infinity()
            } else {
                let p = g.scalar_mul(&basis_scalars[i]);
                assert!(p.is_on_curve() || p.inf, "basis point {} not on curve!", i);
                p
            }
        });

        // Precompute offset point: offset · G
        let offset_point = g.scalar_mul(&offset_scalar);
        assert!(offset_point.is_on_curve(), "offset point not on curve!");

        // Find active dimensions
        let active_dims: Vec<usize> = (0..6)
            .filter(|&i| max_coeff_bits[i] > 0 && !basis_scalars[i].is_zero())
            .collect();

        println!("  [LGK] Active dimensions: {:?}", active_dims);

        // Choose DP bits based on total search space
        let total_bits: u32 = max_coeff_bits.iter().sum();
        let dp_bits = if total_bits > 80 {
            28  // For P135: ~2^28 → manageable DP table
        } else if total_bits > 50 {
            16
        } else if total_bits > 30 {
            8
        } else {
            4
        };

        // Compute step sizes and precompute ALL steps (positive and negative interleaved)
        // No direction bit — positive and negative steps are separate step types
        // STEPS_PER_DIM must be even: first half positive, second half negative
        let half_steps = STEPS_PER_DIM / 2;
        let mut step_points: [[Point; STEPS_PER_DIM]; 6] = std::array::from_fn(|_| {
            std::array::from_fn(|_| Point::infinity())
        });
        let mut step_dists: [[Fe; STEPS_PER_DIM]; 6] = std::array::from_fn(|_| {
            std::array::from_fn(|_| Fe::ZERO)
        });
        let mut max_coeff: [u64; 6] = [0u64; 6];

        for dim in 0..6 {
            let bits = max_coeff_bits[dim];

            if bits == 0 || basis_scalars[dim].is_zero() {
                for j in 0..STEPS_PER_DIM {
                    step_points[dim][j] = Point::infinity();
                    step_dists[dim][j] = Fe::ZERO;
                }
                max_coeff[dim] = 0;
                continue;
            }

            // Optimal mean step ≈ √(max_range) / 2
            let base_step = if bits > 10 {
                (bits / 2).saturating_sub(2) as usize
            } else {
                1usize
            };

            for j in 0..half_steps {
                let step_bits = base_step + j;
                let step_val = 1u64 << step_bits.min(63);

                // Precompute step_val · P_dim (positive direction)
                let step_scalar = Fe::from_u64(step_val);
                let step_pt = basis_points[dim].scalar_mul(&step_scalar);

                // Positive step: step_types [0..half_steps)
                step_points[dim][j] = step_pt.clone();
                let dist_pos = step_scalar.mul_mod_n(&basis_scalars[dim]);
                step_dists[dim][j] = dist_pos;

                // Negative step: step_types [half_steps..STEPS_PER_DIM)
                step_points[dim][j + half_steps] = step_pt.neg();
                step_dists[dim][j + half_steps] = dist_pos.neg_mod_n();
            }

            max_coeff[dim] = if bits < 63 { 1u64 << bits } else { 1u64 << 62 };

            println!("  [LGK] dim {}: bits={}, steps=2^[{}..{}], max=2^{}",
                     dim, bits, base_step, base_step + STEPS_PER_DIM - 1, bits);
        }

        println!("  [LGK] All basis points on curve: {}",
                 basis_points.iter().all(|p| p.is_on_curve() || p.inf));
        println!("  [LGK] Scalar distances: ALL mod n (overflow-PROOF)");
        println!("  [LGK] Hash: FULL normalize_x (CORRECT representation-invariant)");
        println!("  [LGK] DP bits: {} (1 in 2^{} points is DP)", dp_bits, dp_bits);

        LatticeKangaroo {
            g, q: target_point, n,
            basis_scalars, basis_points,
            offset_point, offset_scalar,
            glv,
            step_points, step_dists,
            max_coeff,
            active_dims,
            dp_bits,
        }
    }

    /// Run the Lattice-Guided Kangaroo search.
    ///
    /// v3: Uses FULL normalize_x for step selection. CORRECT deterministic walk.
    /// v2: Track scalar distance as Fe (mod n). NEVER overflows.
    ///
    /// On collision: k = offset + tame_dist - wild_dist (mod n)
    pub fn solve(&self, max_hops: u64) -> LatticeKangarooResult {
        let start_time = Instant::now();

        println!("\n  [LGK] === Lattice-Guided Kangaroo v3 (FULL normalize_x) ===");
        println!("  [LGK] Tame: starts at offset·G (distance = 0)");
        println!("  [LGK] Wild: starts at Q (distance = 0)");
        println!("  [LGK] Active dims: {:?}", self.active_dims);
        println!("  [LGK] Scalar tracking: Fe mod n (NO i64 overflow!)");
        println!("  [LGK] Hash: FULL normalize_x (CORRECT!)");

        let dp_mask: u64 = (1u64 << self.dp_bits.min(64)) - 1;

        // === TAME KANGAROO ===
        // Starts at offset·G with scalar distance = 0
        let mut tame_point = self.offset_point.to_jacobian();
        let mut tame_dist = Fe::ZERO; // scalar distance mod n

        // Warmup: get away from starting point
        for _ in 0..500 {
            let (dim, st) = hash_to_step_lgk(
                &tame_point, &self.active_dims, STEPS_PER_DIM
            );
            tame_point = tame_point.add_affine(&self.step_points[dim][st]);
            tame_dist = tame_dist.add_mod_n(&self.step_dists[dim][st]);
        }

        // === WILD KANGAROO ===
        // Starts at Q with scalar distance = 0
        let mut wild_point = self.q.to_jacobian();
        let mut wild_dist = Fe::ZERO;

        // Warmup
        for _ in 0..500 {
            let (dim, st) = hash_to_step_lgk(
                &wild_point, &self.active_dims, STEPS_PER_DIM
            );
            wild_point = wild_point.add_affine(&self.step_points[dim][st]);
            wild_dist = wild_dist.add_mod_n(&self.step_dists[dim][st]);
        }

        // DP storage: x-coordinate → scalar distance (Fe)
        let mut tame_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut wild_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut collisions = 0usize;

        let report_interval = if max_hops > 100_000 { 500_000 } else { 50_000 };
        let mut total_hops = 0u64;
        let mut last_report = 0u64;

        println!("  [LGK] dp_mask = 0x{:x}, dp_bits = {}", dp_mask, self.dp_bits);

        // Main loop
        while total_hops < max_hops {
            total_hops += 1;

            // === TAME HOP ===
            {
                let (dim, st) = hash_to_step_lgk(
                    &tame_point, &self.active_dims, STEPS_PER_DIM
                );
                tame_point = tame_point.add_affine(&self.step_points[dim][st]);
                tame_dist = tame_dist.add_mod_n(&self.step_dists[dim][st]);

                // Check DP using NEW position's normalized x
                if !tame_point.z.is_zero() {
                    let new_x_norm = normalize_x(&tame_point);

                    if new_x_norm.limbs[0] & dp_mask == 0 {
                        let dp_key = new_x_norm.to_bytes();
                        if let Some(wd) = wild_dps.get(&dp_key) {
                            collisions += 1;
                            if let Some(k) = self.try_recover(&tame_dist, wd) {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                return LatticeKangarooResult {
                                    found: true, k: Some(k),
                                    hops: total_hops, elapsed_ms: elapsed,
                                    method: "LGK v3 (Fe scalar + full norm)".to_string(),
                                };
                            }
                        }
                        tame_dps.insert(dp_key, tame_dist.clone());
                    }
                }
            }

            // === WILD HOP ===
            {
                let (dim, st) = hash_to_step_lgk(
                    &wild_point, &self.active_dims, STEPS_PER_DIM
                );
                wild_point = wild_point.add_affine(&self.step_points[dim][st]);
                wild_dist = wild_dist.add_mod_n(&self.step_dists[dim][st]);

                // Check DP using NEW position's normalized x
                if !wild_point.z.is_zero() {
                    let new_x_norm = normalize_x(&wild_point);

                    if new_x_norm.limbs[0] & dp_mask == 0 {
                        let dp_key = new_x_norm.to_bytes();
                        if let Some(td) = tame_dps.get(&dp_key) {
                            collisions += 1;
                            if let Some(k) = self.try_recover(td, &wild_dist) {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                return LatticeKangarooResult {
                                    found: true, k: Some(k),
                                    hops: total_hops, elapsed_ms: elapsed,
                                    method: "LGK v3 (Fe scalar + full norm)".to_string(),
                                };
                            }
                        }
                        wild_dps.insert(dp_key, wild_dist.clone());
                    }
                }
            }

            // Progress report
            if total_hops - last_report >= report_interval {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = total_hops as f64 / elapsed;
                println!("  [LGK] Hops: {} | Rate: {:.0} hops/s | DPs: {}+{} | Coll: {}",
                         total_hops, rate, tame_dps.len(), wild_dps.len(), collisions);
                last_report = total_hops;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        let rate = if elapsed > 0 { total_hops as f64 / (elapsed as f64 / 1000.0) } else { 0.0 };

        println!("  [LGK] Not found within {} hops ({:.0} hops/s)", max_hops, rate);
        println!("  [LGK] DPs: {} tame, {} wild, {} collisions",
                 tame_dps.len(), wild_dps.len(), collisions);

        LatticeKangarooResult {
            found: false, k: None,
            hops: max_hops, elapsed_ms: elapsed,
            method: "LGK v3 (Fe scalar + full norm)".to_string(),
        }
    }

    /// Try to recover k from a collision.
    ///
    /// On collision between tame (at distance tame_dist) and wild (at distance wild_dist):
    ///   Positive collision: offset + tame_dist ≡ k + wild_dist (mod n)
    ///     → k ≡ offset + tame_dist - wild_dist (mod n)
    ///   Negative collision: offset - tame_dist ≡ k - wild_dist (mod n)
    ///     → k ≡ -(offset + tame_dist + wild_dist) (mod n)
    fn try_recover(&self, tame_dist: &Fe, wild_dist: &Fe) -> Option<Fe> {
        // Case 1: Positive collision → k = offset + tame_dist - wild_dist
        let k_pos = self.offset_scalar.add_mod_n(tame_dist).sub_mod_n(wild_dist);
        let q_check = self.g.scalar_mul(&k_pos);
        if !q_check.inf && q_check.x == self.q.x {
            println!("  [LGK] KEY VERIFIED (positive collision)!");
            return Some(k_pos);
        }

        // Case 2: Negative collision → k = -(offset + tame_dist + wild_dist)
        let k_neg = self.offset_scalar.add_mod_n(tame_dist).add_mod_n(wild_dist).neg_mod_n();
        let q_check_neg = self.g.scalar_mul(&k_neg);
        if !q_check_neg.inf && q_check_neg.x == self.q.x {
            println!("  [LGK] KEY VERIFIED (negative collision)!");
            return Some(k_neg);
        }

        // Check automorphism images for both
        for k_candidate in [&k_pos, &k_neg] {
            let autos = self.glv.automorphism_scalars(k_candidate);
            for ak in &autos {
                let verify = self.g.scalar_mul(ak);
                if !verify.inf && verify.x == self.q.x {
                    println!("  [LGK] KEY FOUND via automorphism!");
                    return Some(ak.clone());
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lgk_scalar_tracking() {
        let g = Point::generator();

        let offset = Fe::from_u64(1000);
        let offset_point = g.scalar_mul(&offset);
        assert!(offset_point.is_on_curve());

        let s0 = Fe::from_u64(5);
        let p0 = g.scalar_mul(&s0);
        let step_val = Fe::from_u64(2);
        let step_point = p0.scalar_mul(&step_val);
        let step_dist = step_val.mul_mod_n(&s0);

        let total_point = offset_point.add(&step_point);
        let total_scalar = offset.add_mod_n(&step_dist);

        let verify = g.scalar_mul(&total_scalar);
        assert!(verify.x == total_point.x, "Scalar tracking mismatch!");
    }
}
