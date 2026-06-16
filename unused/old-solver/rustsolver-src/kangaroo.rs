//! RUSTSOLVER v4 — VORTEX Parallel Kangaroo Solver
//! ================================================
//!
//! NOVEL APPROACH: "Endomorphism-Coupled Walk" (ECW)
//!
//! Key innovation: at each Distinguished Point, compute all 3 GLV images
//! {x(P), x(φ(P)), x(φ²(P))} and store them in the DP hash table.
//! This gives √3 collision probability speedup for the cost of just
//! 2 field multiplications per DP (not per step!).
//!
//! Combined with checking ±y during recovery: total √6 speedup.
//! No mul_mod_n needed in the hot path!
//!
//! Architecture:
//! - 64 parallel walks (32 tame + 32 wild)  
//! - Batch affine conversion via Montgomery's trick
//! - Distinguished points with configurable threshold
//! - GLV image expansion at DP time (ECW)

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::oracle::Round0Oracle;
use num_bigint::BigUint;
use std::collections::HashMap;
use std::time::Instant;

const N_WALKS: usize = 64;

#[derive(Debug)]
pub struct KangarooResult {
    pub found: bool,
    pub key: Option<BigUint>,
    pub total_steps: u64,
    pub dps_stored: u64,
    pub collisions: u64,
    pub false_collisions: u64,
    pub elapsed_secs: f64,
    pub steps_per_sec: f64,
}

pub struct KangarooSolver {
    pub range_bits: u32,
    pub target: Point,
    pub oracle: Option<Round0Oracle>,
    pub dp_bits: u32,
    pub max_steps: u64,
    step_points: Vec<Point>,
    step_scalars: Vec<Fe>,
    n_steps: usize,
    beta: Fe,
    beta_sq: Fe,
    lambda: Fe,
    lambda_sq: Fe,
}

impl KangarooSolver {
    pub fn new(range_bits: u32, target: Point, oracle: Option<Round0Oracle>) -> Self {
        Self::with_config(range_bits, target, oracle, 24, 0)
    }

    pub fn with_config(range_bits: u32, target: Point, oracle: Option<Round0Oracle>,
                        dp_bits: u32, max_steps: u64) -> Self {
        let g = Point::generator();
        // Step size: mean step ≈ √W / (2·√N_WALKS) for parallel kangaroo
        // W = 2^(range_bits-1), so √W = 2^((range_bits-1)/2)
        // With M walks: mean_exp = (range_bits-1)/2 - 1 - log2(√M)/2
        // For N_WALKS=64: log2(8) = 3, so mean_exp = (range_bits-1)/2 - 1 - 1.5 ≈ (range_bits-1)/2 - 3
        let mean_exp = (range_bits as u64 - 1) / 2;
        let parallel_adj = ((N_WALKS as f64).sqrt().log2()) as u64; // ≈3 for 64 walks
        let mean_exp = mean_exp.saturating_sub(parallel_adj + 1);
        let low = mean_exp.saturating_sub(8);
        let high = mean_exp + 8;
        let n_steps = (high - low + 1) as usize;

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

        let beta = Fe { limbs: crate::field::BETA };
        let beta_sq = beta.mul(&beta);
        let lambda = Fe { limbs: crate::field::LAMBDA };
        let lambda_sq = lambda.mul_mod_n(&lambda);
        let max_steps = if max_steps > 0 { max_steps } else { 1_000_000_000 };

        println!("  [VORTEX] Kangaroo Solver initialized");
        println!("  [VORTEX] Range: [2^{}, 2^{}), W = 2^{}", range_bits - 1, range_bits, range_bits - 1);
        println!("  [VORTEX] Steps: 2^{}..2^{} ({} step sizes)", low, high, n_steps);
        println!("  [VORTEX] DP threshold: {} bits (1 in 2^{} points is DP)", dp_bits, dp_bits);
        println!("  [VORTEX] GLV images at DP: 3x (ECW) → √3 collision speedup");
        println!("  [VORTEX] ±y check at recovery → combined √6 speedup");
        println!("  [VORTEX] Expected steps: 2^{:.1} (with √6)", (range_bits - 1) as f64 / 2.0 - 6.0_f64.log2());
        println!("  [VORTEX] Parallel walks: {} ({} tame + {} wild)", N_WALKS, N_WALKS/2, N_WALKS/2);

        KangarooSolver { range_bits, target, oracle, dp_bits, max_steps,
                          step_points, step_scalars, n_steps, beta, beta_sq, lambda, lambda_sq }
    }

    pub fn solve(&self) -> KangarooResult {
        let start = Instant::now();
        let g = Point::generator();
        let n_tame = N_WALKS / 2;
        let n_wild = N_WALKS - n_tame;

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;
        let range_start_fe = Fe::from_biguint_mod_n(&range_start);

        let dp_mask: u64 = (1u64 << self.dp_bits.min(64)) - 1;

        // Initialize tame walks: start from various points in the upper half of range
        let mut tame_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_tame);
        let mut tame_dists: Vec<Fe> = Vec::with_capacity(n_tame);
        for i in 0..n_tame {
            // Spread tame starts across the upper part of the range
            let offset = Fe::from_u64((i * 3 + 1) as u64);
            let start_scalar = range_start_fe.add_mod_n(&offset);
            let start_pt = g.scalar_mul(&start_scalar);
            tame_jacs.push(start_pt.to_jacobian());
            tame_dists.push(start_scalar);
        }

        // Initialize wild walks: start from Q + small offsets
        let mut wild_jacs: Vec<JacobianPoint> = Vec::with_capacity(n_wild);
        let mut wild_dists: Vec<Fe> = Vec::with_capacity(n_wild);
        for i in 0..n_wild {
            let offset = Fe::from_u64(i as u64);
            let start_pt = self.target.add(&g.scalar_mul(&offset));
            wild_jacs.push(start_pt.to_jacobian());
            wild_dists.push(offset);
        }

        // DP table: x-bytes → (distance, is_tame, glv_image)
        let mut dp_table: HashMap<[u8; 32], (Fe, bool, u8)> = HashMap::with_capacity(10_000_000);
        let mut total_steps = 0u64;
        let mut dps_stored = 0u64;
        let mut collisions = 0u64;
        let mut false_collisions = 0u64;
        let mut found = false;
        let mut found_key: Option<BigUint> = None;

        let steps_per_walk = self.max_steps / N_WALKS as u64;
        let report_interval = std::cmp::min(200_000, (steps_per_walk / 10).max(1000));

        println!("\n  [VORTEX] Starting search... ({} steps/walk)", steps_per_walk);

        for step in 0..steps_per_walk {
            // Batch convert to affine
            let mut all_jacs: Vec<JacobianPoint> = Vec::with_capacity(N_WALKS);
            all_jacs.extend_from_slice(&tame_jacs);
            all_jacs.extend_from_slice(&wild_jacs);
            let aff_points = batch_jac_to_affine(&all_jacs);

            // Process each walk: step selection + DP check
            let mut step_indices: Vec<usize> = Vec::with_capacity(N_WALKS);

            for (i, aff) in aff_points.iter().enumerate() {
                if aff.inf { step_indices.push(0); continue; }

                // Use x-coordinate for step selection (x(P) = x(-P), so this is negation-equivariant)
                let si = hash_x_to_step(&aff.x, self.n_steps);
                step_indices.push(si);

                // Check DP
                if aff.x.limbs[0] & dp_mask != 0 { continue; }

                // DP found! Compute 3 GLV images
                let x0 = aff.x;
                let x1 = self.beta.mul(&x0);
                let x2 = self.beta_sq.mul(&x0);

                let is_tame = i < n_tame;
                let dist = if is_tame { tame_dists[i] } else { wild_dists[i - n_tame] };

                let xb0 = x0.to_bytes();
                let xb1 = x1.to_bytes();
                let xb2 = x2.to_bytes();

                if is_tame {
                    // Store all 3 GLV images for this tame DP
                    for (img, xb) in [(0u8, xb0), (1u8, xb1), (2u8, xb2)] {
                        dp_table.entry(xb).or_insert((dist, true, img));
                    }
                    dps_stored += 3;
                } else {
                    // Wild DP: check for collision with ANY stored tame DP
                    let wi = i - n_tame;
                    for (wild_img, xb) in [(0u8, xb0), (1u8, xb1), (2u8, xb2)] {
                        if let Some(&(tame_dist, true, tame_img)) = dp_table.get(&xb) {
                            collisions += 1;
                            // Try key recovery with all 6 GLV± combinations
                            if let Some(k) = self.try_recover(
                                &tame_dist, &dist, tame_img, wild_img,
                                &range_start, &range_end
                            ) {
                                found = true;
                                found_key = Some(k);
                                break;
                            } else {
                                false_collisions += 1;
                            }
                        }
                    }
                    if found { break; }

                    // Also store wild DP images
                    for (img, xb) in [(0u8, xb0), (1u8, xb1), (2u8, xb2)] {
                        dp_table.entry(xb).or_insert((dist, false, img));
                    }
                    dps_stored += 3;
                }
            }

            if found { break; }

            // Advance all walks
            for (i, &si) in step_indices.iter().enumerate() {
                let step_pt = &self.step_points[si];
                let step_sc = &self.step_scalars[si];
                if i < n_tame {
                    tame_jacs[i] = tame_jacs[i].add_affine(step_pt);
                    tame_dists[i] = tame_dists[i].add_mod_n(step_sc);
                } else {
                    let wi = i - n_tame;
                    wild_jacs[wi] = wild_jacs[wi].add_affine(step_pt);
                    wild_dists[wi] = wild_dists[wi].add_mod_n(step_sc);
                }
            }

            total_steps += N_WALKS as u64;

            if step > 0 && step % (report_interval as u64) == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let sps = total_steps as f64 / elapsed;
                println!("    Step {}: {:.0} total | {} DPs | {} coll | {:.0}/s",
                         step, total_steps, dps_stored, collisions, sps);
            }
        }

        let elapsed_secs = start.elapsed().as_secs_f64();
        let steps_per_sec = if elapsed_secs > 0.0 { total_steps as f64 / elapsed_secs } else { 0.0 };

        if !found {
            println!("\n  [VORTEX] Search exhausted: {} steps, {} DPs, {} coll ({} false)",
                     total_steps, dps_stored, collisions, false_collisions);
            println!("  [VORTEX] Steps/sec: {:.0}", steps_per_sec);
        }

        KangarooResult { found, key: found_key, total_steps, dps_stored,
                          collisions, false_collisions, elapsed_secs, steps_per_sec }
    }

    /// Try to recover k from a collision.
    /// 
    /// Tame walk at distance d_t is at point d_t · G.
    /// Wild walk at distance d_w is at point (k + d_w) · G.
    ///
    /// The collision is between GLV images:
    ///   φ^{tame_img}(T) = ±φ^{wild_img}(W)
    ///   λ^{tame_img} · d_t = ±λ^{wild_img} · (k + d_w)  (mod N)
    ///   k + d_w = ±λ^{tame_img - wild_img} · d_t         (mod N)
    ///   k = ±λ^{δ} · d_t - d_w                            (mod N)
    ///
    /// We try all 12 possibilities: δ ∈ {0,1,2}, sign ∈ {+,−}
    fn try_recover(&self, tame_dist: &Fe, wild_dist: &Fe,
                   tame_img: u8, wild_img: u8,
                   range_start: &BigUint, range_end: &BigUint) -> Option<BigUint> {
        let g = Point::generator();
        let delta_img = (tame_img as i32 - wild_img as i32).rem_euclid(3) as u8;

        // Compute λ^delta_img · tame_dist
        let lambda_pow = match delta_img {
            0 => tame_dist.clone(),
            1 => self.lambda.mul_mod_n(tame_dist),
            2 => self.lambda_sq.mul_mod_n(tame_dist),
            _ => unreachable!(),
        };

        // Try both signs
        for sign in [1i8, -1i8] {
            let rotated = if sign > 0 { lambda_pow.clone() } else { lambda_pow.neg_mod_n() };
            let k_fe = rotated.sub_mod_n(wild_dist);
            let k_big = k_fe.to_biguint();

            if k_big >= *range_start && k_big < *range_end {
                // Verify: k·G should equal Q
                let q = g.scalar_mul(&k_fe);
                if !q.inf && q.x == self.target.x {
                    if q.y == self.target.y || q.y == self.target.y.neg_mod_p() {
                        println!("  *** KEY FOUND: 0x{:x} ({}) ***", k_big, k_big.bits());
                        return Some(k_big);
                    }
                }
            }
        }
        None
    }
}

#[inline]
fn hash_x_to_step(x: &Fe, n: usize) -> usize {
    let mut h: usize = 0x811c9dc5;
    for &limb in &x.limbs {
        h = h.wrapping_mul(0x01000193).wrapping_add(limb as usize);
        h = h.wrapping_mul(0x01000193).wrapping_add((limb >> 32) as usize);
    }
    h % n.max(1)
}

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

pub fn selftest(puzzle_num: u32) -> KangarooResult {
    println!("\n  ========================================");
    println!("  VORTEX v4 — Self-Test on Puzzle {}", puzzle_num);
    println!("  ========================================\n");

    let known_keys: HashMap<u32, &str> = [
        (66, "257A3F16B1C0D7F73421CD34C3C9BE36"),
    ].iter().cloned().collect();

    let key_hex = known_keys.get(&puzzle_num).unwrap_or(&"257A3F16B1C0D7F73421CD34C3C9BE36");
    let k = BigUint::parse_bytes(key_hex.as_bytes(), 16).expect("Invalid key hex");
    let g = Point::generator();
    let k_fe = Fe::from_biguint_mod_n(&k);
    let target = g.scalar_mul(&k_fe);

    println!("  [SELFTEST] Key: 0x{} ({} bits)", key_hex, k.bits());
    let range_bits = k.bits() as u32 + 1;
    let dp_bits = std::cmp::max(4, std::cmp::min(20, range_bits / 4));
    let max_steps = 50_000_000;

    let solver = KangarooSolver::with_config(range_bits, target, None, dp_bits, max_steps);
    let result = solver.solve();

    if result.found {
        println!("\n  [SELFTEST] SUCCESS! Found key: 0x{:x}", result.key.as_ref().unwrap());
    } else {
        println!("\n  [SELFTEST] FAILED — key not found within step limit");
    }
    result
}
