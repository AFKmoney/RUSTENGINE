//! TITAN V16.2 — Layer 7: Tag-Team Parallel Kangaroos (FIXED v2)
//! ================================================================
//! V16.2 FIX: Two critical bugs fixed:
//!   1. Replace cheap mod_inv_2k hash with FULL normalize_x (was not representation-invariant → 0 collisions)
//!   2. Fix DP check: normalize the NEW position (after hop), not the old one
//!      Previously: stored (old_x → new_distance) — WRONG!
//!      Now: stores (new_x → new_distance) — CORRECT!
//!
//! Pattern: same as the working basic Kangaroo solver:
//!   - normalize current position → use for step selection
//!   - hop → advance point and distance
//!   - normalize new position → use for DP check AND next step selection

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::glv::GLVDecomposer;
use std::collections::HashMap;
use std::time::Instant;

/// secp256k1 order
const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// Number of step sizes per strategy
const NUM_STEPS: usize = 32;

/// Distinguished point mask bits
const DP_MASK_BITS: u32 = 4;

/// Kangaroo strategy type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KangarooStrategy {
    Classic,
    GlvExpanded,
    Aggressive,
    Conservative,
    WideSweep,
}

impl KangarooStrategy {
    fn name(&self) -> &'static str {
        match self {
            KangarooStrategy::Classic => "Classic",
            KangarooStrategy::GlvExpanded => "GLV-Exp",
            KangarooStrategy::Aggressive => "Aggro",
            KangarooStrategy::Conservative => "Consv",
            KangarooStrategy::WideSweep => "Wide",
        }
    }
}

/// A single kangaroo instance
struct Kangaroo {
    strategy: KangarooStrategy,
    is_tame: bool,
    point: JacobianPoint,
    distance: Fe,
    x_norm: Fe,  // Cached normalized x of CURRENT position
    steps_taken: u64,
    step_points: Vec<Point>,
    step_distances: Vec<Fe>,
}

/// Normalize the x-coordinate of a Jacobian point: x = X/Z²
#[inline]
fn normalize_x(point: &JacobianPoint) -> Fe {
    if point.z.is_zero() { return Fe::ZERO; }
    let z_inv = point.z.modinv();
    let z_inv_sq = z_inv.mul(&z_inv);
    point.x.mul(&z_inv_sq)
}

impl Kangaroo {
    /// Do one hop. Uses cached x_norm for step selection,
    /// then normalizes the NEW position for DP check and next step.
    fn hop(&mut self) {
        // Step selection from CURRENT position's cached x_norm
        let num_steps = self.step_points.len().max(1);
        let step_idx = (self.x_norm.limbs[0] as usize) % num_steps;

        // Advance
        self.point = self.point.add_affine(&self.step_points[step_idx]);
        self.distance = self.distance.add_mod_n(&self.step_distances[step_idx]);
        self.steps_taken += 1;

        // Normalize NEW position — cached for next step selection AND DP check
        self.x_norm = normalize_x(&self.point);
    }
}

type DPKey = [u8; 32];

/// Result from the tag-team solver
#[derive(Clone, Debug)]
pub struct TagTeamResult {
    pub found: bool,
    pub k: Option<Fe>,
    pub total_hops: u64,
    pub total_dps: usize,
    pub collisions: usize,
    pub elapsed_ms: u64,
    pub strategy_counts: Vec<(String, u64)>,
}

/// The Tag-Team Parallel Kangaroo solver
pub struct TagTeamKangaroo {
    pub g: Point,
    pub q: Point,
    pub n: Fe,
    pub glv: GLVDecomposer,
    pub strategies: Vec<KangarooStrategy>,
}

impl TagTeamKangaroo {
    pub fn new(target_point: Point) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();
        let strategies = vec![
            KangarooStrategy::Classic,
            KangarooStrategy::GlvExpanded,
            KangarooStrategy::Aggressive,
            KangarooStrategy::Conservative,
            KangarooStrategy::WideSweep,
        ];
        TagTeamKangaroo { g, q: target_point, n, glv, strategies }
    }

    pub fn with_strategies(target_point: Point, strategies: Vec<KangarooStrategy>) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();
        TagTeamKangaroo { g, q: target_point, n, glv, strategies }
    }

    /// Solve using tag-team kangaroos
    pub fn solve(&self, range_start: &Fe, range_end: &Fe, max_hops: u64) -> TagTeamResult {
        let start_time = Instant::now();

        println!("\n  [TAG] === Tag-Team Parallel Kangaroos (V16.2 FIXED v2) ===");
        println!("  [TAG] Strategies: {}",
                 self.strategies.iter().map(|s| s.name()).collect::<Vec<_>>().join(", "));

        let range_bits = range_start.bit_length();
        println!("  [TAG] Range: [2^{}, 2^{})", range_bits - 1, range_bits);

        let dp_mask: u64 = (1u64 << DP_MASK_BITS.min(64)) - 1;

        // Step sizes: powers of 2 (GCD=1)
        let m = if range_bits >= 10 {
            ((range_bits / 2) as usize).saturating_sub(2).max(6).min(NUM_STEPS)
        } else {
            range_bits as usize
        };
        let num_steps = m.min(NUM_STEPS);

        let step_scalars: Vec<Fe> = (0..num_steps)
            .map(|j| Fe::power_of_2(j as u32))
            .collect();
        let step_points: Vec<Point> = step_scalars.iter()
            .map(|s| self.g.scalar_mul(s))
            .collect();
        let step_distances: Vec<Fe> = step_scalars;

        println!("  [TAG] Steps: {{2^0, ..., 2^{}}} (GCD=1)", num_steps - 1);
        println!("  [TAG] DP bits: {} (1 in 2^{})", DP_MASK_BITS, DP_MASK_BITS);

        // Create kangaroos with DIFFERENT starting points per strategy
        let mut kangaroos: Vec<Kangaroo> = Vec::new();

        for (si, &strategy) in self.strategies.iter().enumerate() {
            // Tame kangaroo: starts at range_end + strategy_offset
            let tame_offset = Fe::from_u64((si * 7919 + 1) as u64);  // Different offset per strategy
            let tame_start_scalar = range_end.add_mod_n(&tame_offset);
            let tame_start = self.g.scalar_mul(&tame_start_scalar);
            let tame = Kangaroo {
                strategy,
                is_tame: true,
                point: tame_start.to_jacobian(),
                distance: tame_start_scalar,
                x_norm: Fe::ZERO,  // Will be set in warmup
                steps_taken: 0,
                step_points: step_points.clone(),
                step_distances: step_distances.clone(),
            };

            // Wild kangaroo: starts at Q + strategy_offset
            let wild_offset = Fe::from_u64((si * 6271 + 7) as u64);
            let wild_start = self.q.add(&self.g.scalar_mul(&wild_offset));
            let wild = Kangaroo {
                strategy,
                is_tame: false,
                point: wild_start.to_jacobian(),
                distance: wild_offset,
                x_norm: Fe::ZERO,
                steps_taken: 0,
                step_points: step_points.clone(),
                step_distances: step_distances.clone(),
            };

            kangaroos.push(tame);
            kangaroos.push(wild);
        }

        // Shared DP pool
        let mut tame_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut wild_dps: HashMap<DPKey, Fe> = HashMap::new();
        let mut collisions = 0usize;

        // Warmup: initialize x_norm and get away from starting points
        for k in &mut kangaroos {
            k.x_norm = normalize_x(&k.point);
            for _ in 0..500 {
                k.hop();
            }
        }

        println!("  [TAG] Warmup complete. Starting search...");

        let report_interval = max_hops / 20;
        let mut total_hops = 0u64;
        let mut last_report = 0u64;

        // Round-robin main loop
        while total_hops < max_hops {
            for kang in &mut kangaroos {
                if total_hops >= max_hops { break; }
                total_hops += 1;

                // Hop — advances point, distance, and x_norm
                kang.hop();

                // DP check on the NEW position (x_norm is already the new position's x)
                if !kang.point.z.is_zero() && kang.x_norm.limbs[0] & dp_mask == 0 {
                    let dp_key = kang.x_norm.to_bytes();

                    if kang.is_tame {
                        if let Some(&wild_dist) = wild_dps.get(&dp_key) {
                            collisions += 1;
                            if let Some(k) = self.try_recover(&kang.distance, &wild_dist, range_start, range_end) {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                println!("\n  [TAG] KEY FOUND! Strategy: {}", kang.strategy.name());
                                return self.make_result(true, Some(k), total_hops, &tame_dps, &wild_dps, collisions, elapsed, &kangaroos);
                            }
                        }
                        tame_dps.insert(dp_key, kang.distance.clone());
                    } else {
                        if let Some(&tame_dist) = tame_dps.get(&dp_key) {
                            collisions += 1;
                            if let Some(k) = self.try_recover(&tame_dist, &kang.distance, range_start, range_end) {
                                let elapsed = start_time.elapsed().as_millis() as u64;
                                println!("\n  [TAG] KEY FOUND! Strategy: {}", kang.strategy.name());
                                return self.make_result(true, Some(k), total_hops, &tame_dps, &wild_dps, collisions, elapsed, &kangaroos);
                            }
                        }
                        wild_dps.insert(dp_key, kang.distance.clone());
                    }
                }
            }

            // Progress report
            if total_hops - last_report >= report_interval {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = total_hops as f64 / elapsed;
                println!("  [TAG] Hops: {} | Rate: {:.0} hops/s | DPs: {}+{} | Coll: {}",
                         total_hops, rate, tame_dps.len(), wild_dps.len(), collisions);
                last_report = total_hops;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        self.make_result(false, None, total_hops, &tame_dps, &wild_dps, collisions, elapsed, &kangaroos)
    }

    fn try_recover(&self, tame_dist: &Fe, wild_dist: &Fe, range_start: &Fe, range_end: &Fe) -> Option<Fe> {
        let k_pos = tame_dist.sub_mod_n(wild_dist);
        let k_neg = tame_dist.add_mod_n(wild_dist).neg_mod_n();

        for k_candidate in &[k_pos, k_neg] {
            if k_candidate.cmp_val(&range_start.limbs).is_ge() &&
               k_candidate.cmp_val(&range_end.limbs).is_lt() {
                let q_check = self.g.scalar_mul(k_candidate);
                if !q_check.inf && q_check.x == self.q.x {
                    return Some(k_candidate.clone());
                }
            }

            let autos = self.glv.automorphism_scalars(k_candidate);
            for ak in &autos {
                if ak.cmp_val(&range_start.limbs).is_ge() &&
                   ak.cmp_val(&range_end.limbs).is_lt() {
                    let verify = self.g.scalar_mul(ak);
                    if !verify.inf && verify.x == self.q.x {
                        return Some(ak.clone());
                    }
                }
            }
        }

        None
    }

    fn make_result(&self, found: bool, k: Option<Fe>, total_hops: u64,
                   tame_dps: &HashMap<DPKey, Fe>, wild_dps: &HashMap<DPKey, Fe>,
                   collisions: usize, elapsed_ms: u64,
                   kangaroos: &[Kangaroo]) -> TagTeamResult {
        let strategy_counts: Vec<(String, u64)> = kangaroos.iter()
            .map(|k| (k.strategy.name().to_string(), k.steps_taken))
            .collect();

        if !found {
            println!("  [TAG] Not found: {} hops, {} DPs, {} collisions",
                     total_hops, tame_dps.len() + wild_dps.len(), collisions);
        }

        TagTeamResult {
            found, k, total_hops,
            total_dps: tame_dps.len() + wild_dps.len(),
            collisions, elapsed_ms, strategy_counts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tagteam_creation() {
        let k = Fe::from_u64(0x6c3a4f);
        let g = Point::generator();
        let q = g.scalar_mul(&k);
        let tt = TagTeamKangaroo::new(q);
        assert_eq!(tt.strategies.len(), 5);
    }
}
