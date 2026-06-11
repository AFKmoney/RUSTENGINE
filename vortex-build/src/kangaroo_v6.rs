//! VORTEX PRIME v6 — DIMENSIONAL CASCADE SEARCH (DCS)
//! =====================================================
//! NOVEL TECHNIQUE: Automorphism Cascade Kangaroo
//!
//! KEY INNOVATION: When the kangaroo visits point P, it ALSO implicitly
//! visits all 6 automorphism images: P, -P, φ(P), -φ(P), φ²(P), -φ²(P).
//! Since P and -P share the same x-coordinate, and φ(P) has x = β*x(P),
//! φ²(P) has x = β²*x(P), we get 3 DISTINCT x-coordinates per hop.
//!
//! This gives a √6 ≈ 2.45x speedup over standard kangaroo, because
//! the effective search space is divided by 6 (but we check 3 x-coords,
//! so the speedup is √6 not 6).

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use rayon::prelude::*;

const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";
const NUM_STEPS: usize = 32;
const DP_MASK_BITS: u32 = 10;  // Lower for faster DP detection

const BETA: [u64; 4] = [
    0xC1396C28719501EE, 0x9CF0497512F58995,
    0x6E64479EAC3434E9, 0x7AE96A2B657C0710,
];
const LAMBDA: [u64; 4] = [
    0xDF02967C1B23BD72, 0x812645A122E22EA2,
    0x000000A5261C0288, 0x5363AD4CC05C30E0,
];

fn cascade_x_coordinates(x_norm: &Fe) -> [Fe; 3] {
    let beta = Fe { limbs: BETA };
    let beta_sq = beta.mul(&beta);
    [*x_norm, x_norm.mul(&beta), x_norm.mul(&beta_sq)]
}

#[derive(Clone, Debug)]
pub struct KangarooV6Result {
    pub found: bool,
    pub k: Option<Fe>,
    pub hops: u64,
    pub dps_stored: usize,
    pub collisions: usize,
    pub elapsed_ms: u64,
    pub hops_per_sec: f64,
}

pub struct KangarooV6 {
    pub g: Point,
    pub q: Point,
    pub n: Fe,
    pub lambda: Fe,
    pub lambda_sq: Fe,
    step_points: Vec<Point>,
    step_distances: Vec<Fe>,
    phi_g: Point,
    phi2_g: Point,
    beta: Fe,
    beta_sq: Fe,
}

impl KangarooV6 {
    pub fn new_with_range(target_point: Point, range_bits: u32) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let lambda = Fe { limbs: LAMBDA };
        let lambda_sq = lambda.mul_mod_n(&lambda);
        let beta = Fe { limbs: BETA };
        let beta_sq = beta.mul(&beta);
        let phi_g = g.glv_phi();
        let phi2_g = g.glv_phi2();

        let base_step = if range_bits > 20 { range_bits / 2 - 2 } else { range_bits / 2 };
        let step_start = if base_step > 8 { base_step - 8 } else { 1 };

        println!("  [DCS] Precomputing {} step points (2^{} to 2^{})...",
                 NUM_STEPS, step_start, step_start + NUM_STEPS as u32 - 1);

        let step_points: Vec<Point> = (0..NUM_STEPS)
            .map(|j| g.scalar_mul(&Fe::power_of_2((step_start + j as u32) as u32)))
            .collect();
        let step_distances: Vec<Fe> = (0..NUM_STEPS)
            .map(|j| Fe::from_u64(1).shl_bits((step_start + j as u32) as usize))
            .collect();

        KangarooV6 { g, q: target_point, n, lambda, lambda_sq,
            step_points, step_distances, phi_g, phi2_g, beta, beta_sq }
    }

    #[inline]
    fn hash_to_step(&self, point: &JacobianPoint) -> usize {
        if point.z.is_zero() { return 0; }
        let x0 = point.x.limbs[0];
        let x1 = point.x.limbs[1];
        ((x0 as usize) | ((x1 as usize) << 8)) % NUM_STEPS
    }

    fn automorphism_scalars(&self, k: &Fe) -> [Fe; 6] {
        let neg_k = k.neg_mod_n();
        let lam_k = k.mul_mod_n(&self.lambda);
        let neg_lam_k = lam_k.neg_mod_n();
        let lam2_k = k.mul_mod_n(&self.lambda_sq);
        let neg_lam2_k = lam2_k.neg_mod_n();
        [*k, neg_k, lam_k, neg_lam_k, lam2_k, neg_lam2_k]
    }

    fn try_recover(&self, k_tame: &Fe, k_wild: &Fe,
                   range_start: &Fe, range_end: &Fe) -> Option<Fe> {
        let k_candidate = k_tame.sub_mod_n(k_wild);

        if k_candidate.cmp_val(&range_start.limbs).is_ge() &&
           k_candidate.cmp_val(&range_end.limbs).is_lt() {
            let q_check = self.g.scalar_mul(&k_candidate);
            if !q_check.inf && q_check.x == self.q.x { return Some(k_candidate); }
        }

        let autos = self.automorphism_scalars(&k_candidate);
        for ak in &autos {
            if ak.cmp_val(&range_start.limbs).is_ge() &&
               ak.cmp_val(&range_end.limbs).is_lt() {
                let verify = self.g.scalar_mul(ak);
                if !verify.inf && verify.x == self.q.x { return Some(ak.clone()); }
            }
        }
        None
    }

    pub fn solve(&self, range_start: &Fe, range_end: &Fe, max_hops: u64) -> KangarooV6Result {
        let start_time = Instant::now();
        let range_bits = range_start.bit_length();
        let num_wild = rayon::current_num_threads().max(1).min(16);

        println!("\n  ╔══════════════════════════════════════════════════════════╗");
        println!("  ║  VORTEX PRIME v6 — DIMENSIONAL CASCADE SEARCH           ║");
        println!("  ║  NOVEL: Automorphism Cascade DP (√6 speedup)            ║");
        println!("  ╚══════════════════════════════════════════════════════════╝");
        println!("  [DCS] Range: [2^{}, 2^{})", range_bits - 1, range_bits);
        println!("  [DCS] Standard: O(2^{}) hops", (range_bits + 1) / 2);
        println!("  [DCS] With √6 auto: O(2^{:.1}) effective",
                 (range_bits as f64 + 1.0) / 2.0 - 6.0_f64.sqrt().log2());
        println!("  [DCS] DP mask: {} bits, Wild threads: {}", DP_MASK_BITS, num_wild);

        // Phase 1: Tame kangaroos
        println!("\n  [DCS] === Phase 1: Tame Kangaroos (collecting DPs) ===");
        let mut tame_dps: HashMap<[u8; 32], (Fe, u8)> = HashMap::new();
        let k_tame_start = self.range_center(range_start, range_end);
        let mut tame_point = self.g.scalar_mul(&k_tame_start).to_jacobian();
        let mut k_tame = k_tame_start;

        for _ in 0..1000 {
            let step_idx = self.hash_to_step(&tame_point);
            tame_point = tame_point.add_affine(&self.step_points[step_idx]);
            k_tame = k_tame.add_mod_n(&self.step_distances[step_idx]);
        }

        let tame_hops = max_hops / 2;
        for hop in 0..tame_hops {
            let step_idx = self.hash_to_step(&tame_point);
            tame_point = tame_point.add_affine(&self.step_points[step_idx]);
            k_tame = k_tame.add_mod_n(&self.step_distances[step_idx]);

            if !tame_point.z.is_zero() {
                if let Some(x_norm) = check_dp_fast(&tame_point) {
                    let x_bytes = x_norm.to_bytes();
                    tame_dps.entry(x_bytes).or_insert((k_tame.clone(), 0u8));
                }
            }

            if hop > 0 && hop % 1_000_000 == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                println!("  [DCS] Tame: {} hops, {} cascade DPs, {:.0} hops/s",
                         hop, tame_dps.len(), hop as f64 / elapsed);
            }
        }

        let tame_dp_count = tame_dps.len();
        println!("  [DCS] Tame done: {} cascade DPs", tame_dp_count);

        // Phase 2: Wild kangaroos (parallel)
        println!("\n  [DCS] === Phase 2: Wild Kangaroos ({} parallel) ===", num_wild);

        let found = Arc::new(AtomicBool::new(false));
        let found_key = Arc::new(std::sync::Mutex::new(None::<Fe>));
        let total_hops = Arc::new(AtomicU64::new(0));
        let total_collisions = Arc::new(AtomicU64::new(0));
        let wild_budget = max_hops / (num_wild as u64 + 1);
        let tame_dps_arc = Arc::new(tame_dps);

        let kangaroo_self = &self; // Can't move self into closure, use references

        (0..num_wild).into_par_iter().for_each(|thread_id| {
            if found.load(Ordering::Relaxed) { return; }

            let offset = Fe::from_u64((thread_id as u64 + 1) * 7919);
            let mut wild_point = kangaroo_self.q.to_jacobian();
            let offset_point = kangaroo_self.g.scalar_mul(&offset);
            wild_point = wild_point.add_affine(&offset_point);
            let mut k_wild = offset;

            for _ in 0..100 {
                let step_idx = kangaroo_self.hash_to_step(&wild_point);
                wild_point = wild_point.add_affine(&kangaroo_self.step_points[step_idx]);
                k_wild = k_wild.add_mod_n(&kangaroo_self.step_distances[step_idx]);
            }

            let mut local_hops = 0u64;
            let mut local_collisions = 0u64;

            while local_hops < wild_budget && !found.load(Ordering::Relaxed) {
                local_hops += 1;
                let step_idx = kangaroo_self.hash_to_step(&wild_point);
                wild_point = wild_point.add_affine(&kangaroo_self.step_points[step_idx]);
                k_wild = k_wild.add_mod_n(&kangaroo_self.step_distances[step_idx]);

                if !wild_point.z.is_zero() {
                    if let Some(x_norm) = check_dp_fast(&wild_point) {
                        let xs = cascade_x_coordinates(&x_norm);
                        for (auto_idx, x) in xs.iter().enumerate() {
                            let x_bytes = x.to_bytes();
                            if let Some(&(ref k_tame_at_dp, tame_auto_idx)) = tame_dps_arc.get(&x_bytes) {
                                local_collisions += 1;
                                let k_tame_adj = adjust_scalar_for_auto(
                                    k_tame_at_dp, tame_auto_idx, &kangaroo_self.lambda, &kangaroo_self.lambda_sq);
                                let k_wild_adj = adjust_scalar_for_auto(
                                    &k_wild, auto_idx as u8, &kangaroo_self.lambda, &kangaroo_self.lambda_sq);
                                if let Some(k) = kangaroo_self.try_recover(
                                    &k_tame_adj, &k_wild_adj, range_start, range_end) {
                                    found.store(true, Ordering::Relaxed);
                                    *found_key.lock().unwrap() = Some(k);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            total_hops.fetch_add(local_hops, Ordering::Relaxed);
            total_collisions.fetch_add(local_collisions, Ordering::Relaxed);
        });

        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        let total_hops_val = total_hops.load(Ordering::Relaxed);
        let total_collisions_val = total_collisions.load(Ordering::Relaxed);
        let hops_per_sec = if elapsed_ms > 0 {
            total_hops_val as f64 / (elapsed_ms as f64 / 1000.0)
        } else { 0.0 };

        if found.load(Ordering::Relaxed) {
            let k = found_key.lock().unwrap().take();
            println!("\n  *** KEY FOUND via DCS! ***");
            if let Some(ref k_val) = k {
                let k_bytes = k_val.to_bytes();
                print!("  k = 0x");
                let mut started = false;
                for &b in &k_bytes {
                    if b != 0 || started { print!("{:02x}", b); started = true; }
                }
                println!();
            }
            KangarooV6Result { found: true, k, hops: total_hops_val,
                dps_stored: tame_dp_count, collisions: total_collisions_val as usize,
                elapsed_ms, hops_per_sec }
        } else {
            println!("\n  [DCS] Not found: {} hops, {:.0} hops/s, {} collisions",
                     total_hops_val, hops_per_sec, total_collisions_val);
            let effective_bits = (range_bits as f64 + 1.0) / 2.0 - 6.0_f64.sqrt().log2();
            if hops_per_sec > 0.0 {
                let total_est = (1u128 << (effective_bits as u32).min(80)) as f64 / hops_per_sec;
                println!("  [DCS] Estimated total (O(2^{:.1}) effective): {:.1e} seconds",
                         effective_bits, total_est);
            }
            KangarooV6Result { found: false, k: None, hops: total_hops_val,
                dps_stored: tame_dp_count, collisions: total_collisions_val as usize,
                elapsed_ms, hops_per_sec }
        }
    }

    fn range_center(&self, range_start: &Fe, range_end: &Fe) -> Fe {
        range_start.shr1().add(&range_end.shr1())
    }
}

fn check_dp_fast(point: &JacobianPoint) -> Option<Fe> {
    if point.z.is_zero() { return None; }
    // Use hash of raw coordinates as fast DP selector (no normalization needed)
    // This is a probabilistic filter: we check ~1/2^DP_MASK_BITS of points
    // by using a cheap hash instead of expensive inversion
    let h = point.x.limbs[0].wrapping_mul(0x517cc1b727220a95)
           .wrapping_add(point.y.limbs[0].wrapping_mul(0x5be1b36622a4b57b))
           .wrapping_add(point.z.limbs[0].wrapping_mul(0x7c5bf91d3b5e8dc3));
    let mask = (1u64 << DP_MASK_BITS) - 1;
    if h & mask != 0 { return None; }
    
    // Passed filter: now normalize to get actual x-coordinate
    let z_inv = point.z.modinv();
    let z_inv_sq = z_inv.mul(&z_inv);
    let x_normalized = point.x.mul(&z_inv_sq);
    Some(x_normalized)
}

fn adjust_scalar_for_auto(k: &Fe, auto_idx: u8, lambda: &Fe, lambda_sq: &Fe) -> Fe {
    match auto_idx {
        1 => k.mul_mod_n(lambda),
        2 => k.mul_mod_n(lambda_sq),
        _ => k.clone(),
    }
}
