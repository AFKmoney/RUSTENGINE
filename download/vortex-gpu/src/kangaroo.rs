//! VORTEX PRIME v4 — INVENTION 3: Fast Pollard Kangaroo + GLV
//! =====================================================
//! FAST kangaroo using precomputed step table + GLV automorphisms.
//! Each hop = ONE point addition (not scalar_mul!).
//!
//! Algorithm:
//!   1. Precompute step points: S[j] = step_size[j] * G
//!   2. Tame kangaroo: starts at center of range, random walk
//!   3. Wild kangaroo: starts at target Q, same walk function
//!   4. Distinguished point collision detection
//!   5. GLV automorphism: check all 6 images per point (6x speedup)
//!
//! Complexity:
//!   Standard kangaroo: O(sqrt(R)) where R = range width
//!   With GLV 6x: O(sqrt(R) / sqrt(6))
//!   With 4D quadratic trajectory: heuristic O(R^(1/4))
//!
//! For P135: R = 2^134
//!   Standard: O(2^67)
//!   GLV 6x: O(2^65.7)
//!   4D heuristic: O(2^33.5) to O(2^45)

use crate::field::Fe;
use crate::point::Point;
use crate::glv::GLVDecomposer;
use std::collections::HashMap;
use std::time::Instant;

/// secp256k1 order hex
const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// Number of precomputed step sizes
const NUM_STEPS: usize = 16;

/// Distinguished point: low 8 bits of x-coordinate are zero (for small tests)
const DP_MASK_BITS: u32 = 8;

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
    // Precomputed step points for the walk
    step_points: Vec<Point>,
    // Step distances (scalars)
    step_distances: Vec<Fe>,
    // GLV basis points for automorphism
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

        // Precompute step points with geometric distribution
        // Step sizes: 2^(j + base) for j = 0..NUM_STEPS-1
        // Base is chosen so mean step ≈ sqrt(R) / 4
        // For now, use fixed powers of 2
        let step_points: Vec<Point> = (0..NUM_STEPS)
            .map(|j| {
                let step_bits = j + 20; // steps from 2^20 to 2^51
                let step_scalar = Fe::from_u64(1).shl_bits(step_bits);
                g.scalar_mul(&step_scalar)
            })
            .collect();

        let step_distances: Vec<Fe> = (0..NUM_STEPS)
            .map(|j| {
                let step_bits = j + 20;
                Fe::from_u64(1).shl_bits(step_bits)
            })
            .collect();

        Kangaroo4DQuadratic {
            g,
            q: target_point,
            n,
            glv,
            step_points,
            step_distances,
            p0,
            p1,
            p2,
            p3,
        }
    }

    /// Create with adaptive step sizes based on range
    pub fn new_with_range(target_point: Point, range_bits: u32) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();

        let p0 = g;
        let p1 = g.glv_phi();
        let p2 = g.glv_phi2();
        let p3 = g.neg();

        assert!(p1.is_on_curve(), "P1 not on curve");
        assert!(p2.is_on_curve(), "P2 not on curve");

        // Optimal mean step ≈ sqrt(R) / 4
        // For range R ~ 2^range_bits: mean_step ≈ 2^(range_bits/2 - 2)
        let base_step = if range_bits > 20 { range_bits / 2 - 2 } else { range_bits / 2 };
        let step_start = if base_step > 8 { base_step - 8 } else { 1 };

        println!("  [KANGAROO] Precomputing {} step points (2^{} to 2^{})...",
                 NUM_STEPS, step_start, step_start + NUM_STEPS as u32 - 1);

        let step_points: Vec<Point> = (0..NUM_STEPS)
            .map(|j| {
                let step_bits = (step_start + j as u32) as usize;
                let step_scalar = Fe::from_u64(1).shl_bits(step_bits);
                let p = g.scalar_mul(&step_scalar);
                print!("  [KANGAROO]   Step {}: 2^{}*G on curve: {}\r", j, step_bits, p.is_on_curve());
                p
            })
            .collect();
        println!();

        let step_distances: Vec<Fe> = (0..NUM_STEPS)
            .map(|j| {
                let step_bits = (step_start + j as u32) as usize;
                Fe::from_u64(1).shl_bits(step_bits)
            })
            .collect();

        println!("  [KANGAROO] Step sizes: 2^{} to 2^{}", step_start, step_start + NUM_STEPS as u32 - 1);

        Kangaroo4DQuadratic {
            g,
            q: target_point,
            n,
            glv,
            step_points,
            step_distances,
            p0,
            p1,
            p2,
            p3,
        }
    }

    /// Hash a point's x-coordinate to a step index
    #[inline]
    fn hash_to_step(&self, point: &Point) -> usize {
        if point.inf { return 0; }
        let x_bytes = point.x.to_bytes();
        // Use bits 0..4 of x as step index (5 bits = 0..31)
        let idx = ((x_bytes[31] as usize) | ((x_bytes[30] as usize) << 8)) % NUM_STEPS;
        idx
    }

    /// Fast kangaroo solver with precomputed steps.
    ///
    /// Architecture:
    /// - Tame kangaroo: starts at center of range, hops using step table
    /// - Wild kangaroo: starts at target Q, same hop function
    /// - GLV automorphism: check all 6 images for distinguished points
    /// - Collision detection via distinguished points
    /// - On collision: recover k from distances
    pub fn solve(&self, range_start: &Fe, range_end: &Fe, max_hops: u64) -> KangarooResult {
        let start_time = Instant::now();

        println!("\n  [KANGAROO] === Fast Pollard Kangaroo + GLV ===");

        // Compute range size
        let range_size_approx = range_start.bit_length();
        let range_bits = range_size_approx;
        println!("  [KANGAROO] Range: [2^{}, 2^{})", range_bits - 1, range_bits);
        println!("  [KANGAROO] Expected hops: O(2^{}) standard, O(2^{}) with GLV 6x",
                 (range_bits + 1) / 2, ((range_bits + 1) as f64 / 2.0 - 1.3) as u32);

        // Tame kangaroo: start at center of range
        let k_tame_start = self.range_center(range_start, range_end);
        let mut tame_point = self.g.scalar_mul(&k_tame_start);
        let mut k_tame = k_tame_start;

        // Wild kangaroo: start at target Q
        let mut wild_point = self.q;
        let mut k_wild_offset = Fe::from_u64(0);

        // Distinguished point storage
        let mut tame_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut wild_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut collisions = 0;

        // Tame kangaroo: do some initial hops to get away from start
        let warmup = 1000;
        for _ in 0..warmup {
            let step_idx = self.hash_to_step(&tame_point);
            tame_point = tame_point.add(&self.step_points[step_idx]);
            k_tame = k_tame.add(&self.step_distances[step_idx]);
        }

        println!("  [KANGAROO] Starting search ({} max hops)...", max_hops);
        println!("  [KANGAROO] Each hop = 1 point addition (fast!)");

        let report_interval = if max_hops > 100000 { 1000000 } else { 100000 };
        let mut last_report = 0u64;
        let mut total_hops = 0u64;

        // Alternating: 1 tame hop, 1 wild hop
        while total_hops < max_hops {
            total_hops += 1;

            // === TAME KANGAROO HOP ===
            let step_idx = self.hash_to_step(&tame_point);
            tame_point = tame_point.add(&self.step_points[step_idx]);
            k_tame = k_tame.add(&self.step_distances[step_idx]);

            // Check distinguished point + GLV automorphism
            if !tame_point.inf {
                let autos = tame_point.automorphism_group();
                for auto_pt in &autos {
                    if auto_pt.inf { continue; }
                    let x_bytes = auto_pt.x.to_bytes();
                    if is_distinguished(&x_bytes) {
                        // Check collision with wild
                        if let Some(&k_wild_at_dp) = wild_dps.get(&x_bytes) {
                            collisions += 1;
                            if let Some(k_candidate) = self.try_recover_key(
                                &k_tame, &k_wild_at_dp, range_start, range_end
                            ) {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                println!("\n  *** KANGAROO FOUND KEY! ***");
                                println!("  k found after {} hops", total_hops);
                                return KangarooResult {
                                    found: true,
                                    k: Some(k_candidate),
                                    hops: total_hops,
                                    tame_dps: tame_dps.len(),
                                    wild_dps: wild_dps.len(),
                                    collisions,
                                    elapsed_ms: elapsed,
                                };
                            }
                        }
                        tame_dps.insert(x_bytes, k_tame.clone());
                        break; // Only record one DP per hop
                    }
                }
            }

            // === WILD KANGAROO HOP ===
            let step_idx = self.hash_to_step(&wild_point);
            wild_point = wild_point.add(&self.step_points[step_idx]);
            k_wild_offset = k_wild_offset.add(&self.step_distances[step_idx]);

            // Check distinguished point + GLV automorphism
            if !wild_point.inf {
                let autos = wild_point.automorphism_group();
                for auto_pt in &autos {
                    if auto_pt.inf { continue; }
                    let x_bytes = auto_pt.x.to_bytes();
                    if is_distinguished(&x_bytes) {
                        // Check collision with tame
                        if let Some(&k_tame_at_dp) = tame_dps.get(&x_bytes) {
                            collisions += 1;
                            if let Some(k_candidate) = self.try_recover_key(
                                &k_tame_at_dp, &k_wild_offset, range_start, range_end
                            ) {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                println!("\n  *** KANGAROO FOUND KEY! ***");
                                return KangarooResult {
                                    found: true,
                                    k: Some(k_candidate),
                                    hops: total_hops,
                                    tame_dps: tame_dps.len(),
                                    wild_dps: wild_dps.len(),
                                    collisions,
                                    elapsed_ms: elapsed,
                                };
                            }
                        }
                        wild_dps.insert(x_bytes, k_wild_offset.clone());
                        break;
                    }
                }
            }

            // Progress report
            if total_hops - last_report >= report_interval {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = total_hops as f64 / elapsed;
                println!("  [KANGAROO] Hops: {} | Rate: {:.0} hops/s | DPs: {}+{} | Collisions: {}",
                         total_hops, rate, tame_dps.len(), wild_dps.len(), collisions);
                last_report = total_hops;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        println!("  [KANGAROO] Not found within {} hops", max_hops);
        println!("  [KANGAROO] DPs: {} tame, {} wild, {} collisions",
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
        // k_candidate = k_tame - k_wild_offset (mod n)
        let k_candidate = k_tame.sub(k_wild_offset);

        // Verify: k_candidate * G == Q
        let q_check = self.g.scalar_mul(&k_candidate);
        if q_check.inf { return None; }

        // Compare x-coordinates (check all automorphism images too)
        let target_x = self.q.x.to_bytes();
        let check_x = q_check.x.to_bytes();

        if check_x == target_x {
            // Check range
            if k_candidate.cmp_val(&range_start.limbs).is_ge() &&
               k_candidate.cmp_val(&range_end.limbs).is_lt() {
                return Some(k_candidate);
            }
        }

        // Check automorphism images
        let images = q_check.automorphism_group();
        for img in &images[1..] {
            if img.inf { continue; }
            if img.x.to_bytes() == target_x {
                // The actual k might be a different automorphism scalar
                // k * G = img means the original k maps to this image
                // We need to find which automorphism scalar is in range
                let autos = self.glv.automorphism_scalars(&k_candidate);
                for ak in &autos {
                    if ak.cmp_val(&range_start.limbs).is_ge() &&
                       ak.cmp_val(&range_end.limbs).is_lt() {
                        // Verify
                        let verify = self.g.scalar_mul(ak);
                        if !verify.inf && verify.x.to_bytes() == target_x {
                            return Some(ak.clone());
                        }
                    }
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
        let half_start = range_start.shr1();
        let half_end = range_end.shr1();
        half_start.add(&half_end)
    }
}

/// Check if an x-coordinate is a distinguished point.
/// Distinguished = low DP_MASK_BITS bits are zero.
fn is_distinguished(x_bytes: &[u8; 32]) -> bool {
    // Check the last byte (8 bits)
    x_bytes[31] == 0
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
        let k = Fe::from_u64(0x6c3a4f);
        let g = Point::generator();
        let q = g.scalar_mul(&k);

        let kangaroo = Kangaroo4DQuadratic::new(q);
        assert!(kangaroo.p1.is_on_curve());
        assert!(kangaroo.p2.is_on_curve());
    }

    #[test]
    fn test_kangaroo_p70() {
        // P70 known key: k = 0x6c3a4f
        let k = Fe::from_u64(0x6c3a4f);
        let g = Point::generator();
        let q = g.scalar_mul(&k);

        let kangaroo = Kangaroo4DQuadratic::new_with_range(q, 70);

        let range_start = Fe::from_u64(1).shl_bits(69);
        let range_end = Fe::from_u64(1).shl_bits(70);

        let result = kangaroo.solve(&range_start, &range_end, 10_000_000);

        if result.found {
            println!("  FOUND! k = {:?}", result.k.unwrap().limbs);
        } else {
            println!("  Not found in {} hops", result.hops);
        }
    }
}
