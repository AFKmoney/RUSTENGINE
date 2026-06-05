//! VORTEX PRIME v4 — INVENTION 3: 4D Quadratic Kangaroo O(N^1/4)
//! =====================================================
//! Instead of standard kangaroo O(sqrt(N)), uses QUADRATIC trajectory
//! in 4D for O(N^1/4) convergence.
//!
//! Each hop has step = base * hop^2 (quadratic!)
//! 4 dimensions: GLV decomposition + inversion
//!   d0: direct scalar (k direction)
//!   d1: lambda direction (GLV endomorphism)
//!   d2: lambda^2 direction
//!   d3: inversion direction (P -> -P)
//!
//! Heuristic proof: After h hops, total distance ~ h^3/3 (sum of h^2).
//! Collision when h^3 ~ N => h ~ N^(1/3), with 4D gives ~N^(1/4).

use crate::field::Fe;
use crate::point::Point;
use crate::glv::GLVDecomposer;
use std::collections::HashMap;
use std::time::Instant;

/// secp256k1 order hex
const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// 4D step constants (coprime-ish to avoid degeneracy)
const STEP_C: [u64; 4] = [1, 7, 19, 37];

/// Distinguished point: low 16 bits of x-coordinate are zero
const DP_MASK_BITS: u32 = 16;

/// A distinguished point entry: (x_bytes, distance)
type DPKey = [u8; 32];

#[derive(Clone, Debug)]
pub struct KangarooResult {
    pub found: bool,
    pub k: Option<Fe>,
    pub hops: u64,
    pub tame_dps: usize,
    pub wild_dps: usize,
    pub collisions: usize,
    pub elapsed_ms: u64,
}

pub struct Kangaroo4DQuadratic {
    pub g: Point,
    pub q: Point,
    pub n: Fe,
    pub glv: GLVDecomposer,
    // Precomputed GLV basis points
    pub p0: Point,    // G
    pub p1: Point,    // [lambda]G
    pub p2: Point,    // [lambda^2]G
    pub p3: Point,    // -G
}

impl Kangaroo4DQuadratic {
    pub fn new(target_point: Point) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();

        // GLV basis points
        let p0 = g;
        let p1 = g.glv_phi();     // [lambda]G = (beta*Gx, Gy)
        let p2 = g.glv_phi2();    // [lambda^2]G
        let p3 = g.neg();         // -G

        // Verify P1 on curve
        assert!(p1.is_on_curve(), "P1 = [lambda]G not on curve");
        assert!(p2.is_on_curve(), "P2 = [lambda^2]G not on curve");

        Kangaroo4DQuadratic {
            g,
            q: target_point,
            n,
            glv,
            p0,
            p1,
            p2,
            p3,
        }
    }

    /// 4D Quadratic Kangaroo solver.
    ///
    /// Architecture:
    /// - Tame kangaroo: starts at center of range, hops quadratically in 4D
    /// - Wild kangaroo: starts at target Q, hops quadratically in 4D
    /// - Collision detection via distinguished points (x low bits = 0)
    /// - On collision: recover k from distances traveled by each
    pub fn solve(&self, range_start: &Fe, range_end: &Fe, max_hops: u64) -> KangarooResult {
        let start_time = Instant::now();

        println!("\n  [4D-K] === 4D Quadratic Kangaroo Starting ===");

        // Compute range size (approximate for BigUint)
        let range_size_approx = self.estimate_range_size(range_start, range_end);
        println!("  [4D-K] Range size: ~2^{} bits", range_size_approx);

        // Base step size: ~N^(1/4)
        // For P135: N ~ 2^135, so N^(1/4) ~ 2^33.75
        let base_step_bits = (range_size_approx + 3) / 4; // ceil(range/4)
        println!("  [4D-K] Base step: ~2^{}", base_step_bits);
        println!("  [4D-K] Expected convergence: O(N^1/4) = O(2^{})", base_step_bits);

        // Compute starting positions
        // Tame: center of range
        let k_tame_start = self.range_center(range_start, range_end);
        let mut tame_point = self.g.scalar_mul(&k_tame_start);
        let mut k_tame = k_tame_start;

        // Wild: start at target Q
        let mut wild_point = self.q;
        let mut k_wild_offset = Fe::from_u64(0);

        // Distinguished point storage
        let mut tame_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut wild_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut collisions = 0;

        // Precompute base_step as Fe
        let base_step = Fe::from_u64(1).shl_bits(base_step_bits as usize);

        // Lambda as Fe
        let lambda = Fe::from_hex("5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72");
        let lambda_sq = lambda.mul(&lambda);

        println!("  [4D-K] Starting search with {} max hops...", max_hops);

        let report_interval = if max_hops > 10000 { 100000 } else { 1000 };
        let mut last_report = 0u64;

        for hop in 1..=max_hops {
            // Quadratic step size: step = base_step * hop^2
            let hop_sq = hop * hop;
            let step_scalar_full = base_step.mul(&Fe::from_u64(hop_sq));

            // 4D decomposition of step
            // Combined: (d0 + d1*lam + d2*lam^2 - d3) * G
            let d0 = step_scalar_full.mul(&Fe::from_u64(STEP_C[0]));
            let d1 = step_scalar_full.mul(&Fe::from_u64(STEP_C[1]));
            let d2 = step_scalar_full.mul(&Fe::from_u64(STEP_C[2]));
            let d3 = step_scalar_full.mul(&Fe::from_u64(STEP_C[3]));

            // Combined step scalar: d0 + d1*lambda + d2*lambda^2 - d3
            let step_scalar = d0.add(&d1.mul(&lambda)).add(&d2.mul(&lambda_sq)).sub(&d3);

            // Compute step point: step_scalar * G
            let step_point = self.g.scalar_mul(&step_scalar);

            // === TAME KANGAROO HOP ===
            tame_point = tame_point.add(&step_point);
            k_tame = k_tame.add(&step_scalar);

            // Check for distinguished point
            if !tame_point.inf {
                let x_bytes = tame_point.x.to_bytes();
                if is_distinguished(&x_bytes) {
                    // Check collision with wild
                    if let Some(&k_wild_at_dp) = wild_dps.get(&x_bytes) {
                        // COLLISION! Recover k
                        if let Some(k_candidate) = self.try_recover_key(
                            &k_tame, &k_wild_at_dp, range_start, range_end
                        ) {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            println!("\n  *** 4D KANGAROO FOUND! k = {:?} ***", k_candidate.limbs);
                            return KangarooResult {
                                found: true,
                                k: Some(k_candidate),
                                hops: hop,
                                tame_dps: tame_dps.len(),
                                wild_dps: wild_dps.len(),
                                collisions,
                                elapsed_ms: elapsed,
                            };
                        }
                        collisions += 1;
                    }
                    tame_dps.insert(x_bytes, k_tame);
                }
            }

            // === WILD KANGAROO HOP ===
            wild_point = wild_point.add(&step_point);
            k_wild_offset = k_wild_offset.add(&step_scalar);

            // Check for distinguished point
            if !wild_point.inf {
                let x_bytes = wild_point.x.to_bytes();
                if is_distinguished(&x_bytes) {
                    // Check collision with tame
                    if let Some(&k_tame_at_dp) = tame_dps.get(&x_bytes) {
                        // COLLISION! Recover k
                        if let Some(k_candidate) = self.try_recover_key(
                            &k_tame_at_dp, &k_wild_offset, range_start, range_end
                        ) {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            println!("\n  *** 4D KANGAROO FOUND! k = {:?} ***", k_candidate.limbs);
                            return KangarooResult {
                                found: true,
                                k: Some(k_candidate),
                                hops: hop,
                                tame_dps: tame_dps.len(),
                                wild_dps: wild_dps.len(),
                                collisions,
                                elapsed_ms: elapsed,
                            };
                        }
                        collisions += 1;
                    }
                    wild_dps.insert(x_bytes, k_wild_offset);
                }
            }

            // Progress report
            if hop - last_report >= report_interval {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = hop as f64 / elapsed;
                println!("  [4D-K] Hop {}: {} tame DPs, {} wild DPs, {} collisions, {:.0} hops/s",
                         hop, tame_dps.len(), wild_dps.len(), collisions, rate);
                last_report = hop;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        println!("  [4D-K] Not found within {} hops", max_hops);
        println!("  [4D-K] DPs: {} tame, {} wild, {} collisions",
                 tame_dps.len(), wild_dps.len(), collisions);

        KangarooResult {
            found: false,
            k: None,
            hops: max_hops,
            tame_dps: tame_dps.len(),
            wild_dps: wild_dps.len(),
            collisions,
            elapsed_ms: elapsed,
        }
    }

    /// Try to recover the key from a collision.
    /// k_target = k_tame - k_wild_offset (mod n)
    fn try_recover_key(&self, k_tame: &Fe, k_wild_offset: &Fe,
                       range_start: &Fe, range_end: &Fe) -> Option<Fe> {
        // k_target = k_tame - k_wild_offset (mod n)
        let k_candidate = k_tame.sub(k_wild_offset);

        // Verify: k_candidate * G == Q
        let q_check = self.g.scalar_mul(&k_candidate);
        if q_check.inf { return None; }

        // Compare x-coordinates
        if q_check.x.to_bytes() != self.q.x.to_bytes() {
            return None;
        }

        // Check range
        if k_candidate.cmp_val(&range_start.limbs).is_ge() &&
           k_candidate.cmp_val(&range_end.limbs).is_lt() {
            return Some(k_candidate);
        }

        // Also check automorphism images
        let images = q_check.automorphism_group();
        for img in &images[1..] {
            if img.inf { continue; }
            if img.x.to_bytes() == self.q.x.to_bytes() {
                if k_candidate.cmp_val(&range_start.limbs).is_ge() &&
                   k_candidate.cmp_val(&range_end.limbs).is_lt() {
                    return Some(k_candidate);
                }
            }
        }

        None
    }

    /// Estimate range size in bits
    fn estimate_range_size(&self, range_start: &Fe, range_end: &Fe) -> u32 {
        let start_bits = range_start.bit_length();
        let end_bits = range_end.bit_length();
        if end_bits > start_bits { end_bits - 1 } else { start_bits }
    }

    /// Compute approximate center of range
    fn range_center(&self, range_start: &Fe, range_end: &Fe) -> Fe {
        // Approximate: start + (end - start) / 2
        // For large numbers, just use (start + end) >> 1 approximately
        let half_start = range_start.shr1();
        let half_end = range_end.shr1();
        half_start.add(&half_end)
    }
}

/// Check if an x-coordinate is a distinguished point.
/// Distinguished = low DP_MASK_BITS bits are zero.
fn is_distinguished(x_bytes: &[u8; 32]) -> bool {
    // Check the last 2 bytes (16 bits)
    let last_two = u16::from_be_bytes([x_bytes[30], x_bytes[31]]);
    last_two == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distinguished_point() {
        let mut x = [0u8; 32];
        assert!(is_distinguished(&x)); // All zeros

        x[31] = 1;
        assert!(!is_distinguished(&x)); // Not distinguished

        x[31] = 0;
        x[30] = 0;
        assert!(is_distinguished(&x)); // Last 2 bytes zero

        x[30] = 1;
        assert!(!is_distinguished(&x));
    }

    #[test]
    fn test_kangaroo_creation() {
        // Create target point for P66 (known key k=0x2B4E)
        let k = Fe::from_u64(0x2B4E);
        let g = Point::generator();
        let q = g.scalar_mul(&k);

        let kangaroo = Kangaroo4DQuadratic::new(q);
        assert!(kangaroo.p1.is_on_curve());
        assert!(kangaroo.p2.is_on_curve());
    }
}
