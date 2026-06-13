//! TITAN V16.2 — Layer 9: Bloom-Filter Collision Accelerator (FIXED)
//! ================================================================
//! V16.2 FIX: Replace cheap mod_inv_2k hash with FULL normalize_x.
//!   - The cheap hash was NOT representation-invariant → 0 collisions!
//!   - Now uses full field inversion per step — slower but CORRECT.
//!
//! Two-tier Bloom filter architecture:
//!   Tier 1: Bloom filter for O(1) probabilistic DP matching
//!   Tier 2: Compact HashMap for confirmed matches (key recovery)

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::glv::GLVDecomposer;
use std::collections::HashMap;
use std::time::Instant;

/// secp256k1 order
const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// Number of step sizes
const NUM_STEPS: usize = 32;

/// DP mask bits
const DP_MASK_BITS: u32 = 4;

// ============================================================
// BLOOM FILTER
// ============================================================

/// A simple Bloom filter for 32-byte keys
#[derive(Clone)]
pub struct BloomFilter {
    bits: Vec<u64>,
    num_bits: usize,
    num_hashes: usize,
    count: usize,
}

impl BloomFilter {
    pub fn new(expected_items: usize, fp_rate: f64) -> Self {
        let ln2_sq = (2.0f64.ln()).powi(2);
        let num_bits = (-(expected_items as f64) * fp_rate.ln() / ln2_sq) as usize;
        let num_bits = num_bits.next_power_of_two().max(1024);
        let num_hashes = ((num_bits as f64 / expected_items.max(1) as f64) * 2.0f64.ln()) as usize;
        let num_hashes = num_hashes.clamp(2, 7);
        let num_words = (num_bits + 63) / 64;

        println!("  [BLOOM] Filter: {} bits ({} MB), {} hashes, FP rate: {:.3}%",
                 num_bits, num_words * 8 / (1024 * 1024), num_hashes, fp_rate * 100.0);

        BloomFilter { bits: vec![0u64; num_words], num_bits, num_hashes, count: 0 }
    }

    pub fn with_bits(num_bits: usize) -> Self {
        let num_bits = num_bits.next_power_of_two();
        let num_words = (num_bits + 63) / 64;
        BloomFilter { bits: vec![0u64; num_words], num_bits, num_hashes: 3, count: 0 }
    }

    #[inline]
    fn hash_indices(&self, key: &[u8; 32], hash_idx: usize) -> usize {
        let h1 = u64::from_be_bytes(key[0..8].try_into().unwrap());
        let h2 = u64::from_be_bytes(key[8..16].try_into().unwrap());
        let combined = h1.wrapping_add((hash_idx as u64).wrapping_mul(h2));
        (combined as usize) % self.num_bits
    }

    pub fn insert(&mut self, key: &[u8; 32]) {
        for i in 0..self.num_hashes {
            let idx = self.hash_indices(key, i);
            self.bits[idx / 64] |= 1u64 << (idx % 64);
        }
        self.count += 1;
    }

    pub fn contains(&self, key: &[u8; 32]) -> bool {
        for i in 0..self.num_hashes {
            let idx = self.hash_indices(key, i);
            if self.bits[idx / 64] & (1u64 << (idx % 64)) == 0 {
                return false;
            }
        }
        true
    }

    pub fn clear(&mut self) {
        for word in &mut self.bits { *word = 0; }
        self.count = 0;
    }

    pub fn count(&self) -> usize { self.count }
    pub fn memory_bytes(&self) -> usize { self.bits.len() * 8 }
}

// ============================================================
// ROLLING BLOOM FILTER
// ============================================================

/// A rolling Bloom filter that automatically expires old entries.
pub struct RollingBloom {
    filters: Vec<BloomFilter>,
    current_gen: usize,
    max_gens: usize,
    items_per_gen: usize,
    gen_count: usize,
}

impl RollingBloom {
    pub fn new(max_gens: usize, items_per_gen: usize) -> Self {
        let filters = vec![BloomFilter::new(items_per_gen, 0.01); max_gens];
        RollingBloom { filters, current_gen: 0, max_gens, items_per_gen, gen_count: 0 }
    }

    pub fn insert(&mut self, key: &[u8; 32]) {
        self.filters[self.current_gen].insert(key);
        self.gen_count += 1;
        if self.gen_count >= self.items_per_gen {
            self.current_gen = (self.current_gen + 1) % self.max_gens;
            self.filters[self.current_gen].clear();
            self.gen_count = 0;
        }
    }

    pub fn contains(&self, key: &[u8; 32]) -> bool {
        for filter in &self.filters {
            if filter.contains(key) { return true; }
        }
        false
    }
}

// ============================================================
// BLOOM-ACCELERATED KANGAROO (FIXED)
// ============================================================

/// Result from the bloom-accelerated kangaroo
#[derive(Clone, Debug)]
pub struct BloomKangarooResult {
    pub found: bool,
    pub k: Option<Fe>,
    pub hops: u64,
    pub tame_dps: usize,
    pub wild_dps: usize,
    pub bloom_checks: u64,
    pub bloom_hits: u64,
    pub false_positives: u64,
    pub collisions: usize,
    pub elapsed_ms: u64,
}

/// Normalize the x-coordinate of a Jacobian point: x = X/Z²
#[inline]
fn normalize_x(point: &JacobianPoint) -> Fe {
    if point.z.is_zero() { return Fe::ZERO; }
    let z_inv = point.z.modinv();
    let z_inv_sq = z_inv.mul(&z_inv);
    point.x.mul(&z_inv_sq)
}

/// Bloom-Filter accelerated Kangaroo solver (FIXED)
pub struct BloomKangaroo {
    pub g: Point,
    pub q: Point,
    pub n: Fe,
    pub glv: GLVDecomposer,
    step_points: Vec<Point>,
    step_distances: Vec<Fe>,
}

impl BloomKangaroo {
    pub fn new(target_point: Point, range_bits: u32) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();

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
            .map(|s| g.scalar_mul(s))
            .collect();

        let step_distances: Vec<Fe> = step_scalars;

        BloomKangaroo { g, q: target_point, n, glv, step_points, step_distances }
    }

    /// Solve using Bloom-filter accelerated kangaroo
    pub fn solve(&self, range_start: &Fe, range_end: &Fe, max_hops: u64) -> BloomKangarooResult {
        let start_time = Instant::now();

        println!("\n  [BLOOM] === Bloom-Filter Collision Accelerator (V16.2 FIXED) ===");

        let range_bits = range_start.bit_length();
        println!("  [BLOOM] Range: [2^{}, 2^{})", range_bits - 1, range_bits);

        let dp_mask: u64 = (1u64 << DP_MASK_BITS.min(64)) - 1;

        // === SETUP BLOOM FILTERS ===
        let expected_dps = (max_hops >> DP_MASK_BITS) as usize;
        let mut tame_bloom = BloomFilter::new(expected_dps.max(1000), 0.001);
        let mut wild_bloom = BloomFilter::new(expected_dps.max(1000), 0.001);

        // Tier 2: Compact hash tables for confirmed matches
        let mut tame_exact: HashMap<[u8; 32], Fe> = HashMap::new();
        let mut wild_exact: HashMap<[u8; 32], Fe> = HashMap::new();

        println!("  [BLOOM] Tame bloom: {} MB", tame_bloom.memory_bytes() / (1024 * 1024));
        println!("  [BLOOM] Wild bloom: {} MB", wild_bloom.memory_bytes() / (1024 * 1024));

        let mut total_hops = 0u64;
        let mut bloom_checks = 0u64;
        let mut bloom_hits = 0u64;
        let mut false_positives = 0u64;
        let mut collisions = 0usize;

        // Tame kangaroo: starts at range end
        let mut tame_point = self.g.scalar_mul(range_end).to_jacobian();
        let mut k_tame = range_end.clone();
        let mut tame_x_norm = normalize_x(&tame_point);

        // Wild kangaroo: starts at Q
        let mut wild_point = self.q.to_jacobian();
        let mut k_wild_offset = Fe::from_u64(0);
        let mut wild_x_norm = normalize_x(&wild_point);

        let num_steps = self.step_points.len();

        // Warmup (using correct pattern: normalize NEW position after hop)
        for _ in 0..500 {
            let step_idx = (tame_x_norm.limbs[0] as usize) % num_steps;
            tame_point = tame_point.add_affine(&self.step_points[step_idx]);
            k_tame = k_tame.add_mod_n(&self.step_distances[step_idx]);
            tame_x_norm = normalize_x(&tame_point);

            let step_idx = (wild_x_norm.limbs[0] as usize) % num_steps;
            wild_point = wild_point.add_affine(&self.step_points[step_idx]);
            k_wild_offset = k_wild_offset.add_mod_n(&self.step_distances[step_idx]);
            wild_x_norm = normalize_x(&wild_point);
        }

        println!("  [BLOOM] Starting search ({} max hops)...", max_hops);

        let report_interval = max_hops / 20;
        let mut last_report = 0u64;

        while total_hops < max_hops {
            total_hops += 1;

            // === TAME HOP ===
            {
                // Step selection from CURRENT position's cached x_norm
                let step_idx = (tame_x_norm.limbs[0] as usize) % num_steps;
                tame_point = tame_point.add_affine(&self.step_points[step_idx]);
                k_tame = k_tame.add_mod_n(&self.step_distances[step_idx]);

                // Normalize NEW position for DP check AND next step
                if !tame_point.z.is_zero() {
                    tame_x_norm = normalize_x(&tame_point);

                    if tame_x_norm.limbs[0] & dp_mask == 0 {
                        let dp_key = tame_x_norm.to_bytes();

                        bloom_checks += 1;
                        if wild_bloom.contains(&dp_key) {
                            bloom_hits += 1;
                            if let Some(&wild_dist) = wild_exact.get(&dp_key) {
                                collisions += 1;
                                if let Some(k) = self.try_recover(&k_tame, &wild_dist, range_start, range_end) {
                                    let elapsed = start_time.elapsed().as_millis() as u64;
                                    println!("\n  [BLOOM] KEY FOUND! Bloom hits: {}, FPs: {}", bloom_hits, false_positives);
                                    return BloomKangarooResult {
                                        found: true, k: Some(k), hops: total_hops,
                                        tame_dps: tame_exact.len(), wild_dps: wild_exact.len(),
                                        bloom_checks, bloom_hits, false_positives, collisions, elapsed_ms: elapsed,
                                    };
                                }
                            } else {
                                false_positives += 1;
                            }
                        }
                        tame_bloom.insert(&dp_key);
                        tame_exact.insert(dp_key, k_tame.clone());
                    }
                }
            }

            // === WILD HOP ===
            {
                let step_idx = (wild_x_norm.limbs[0] as usize) % num_steps;
                wild_point = wild_point.add_affine(&self.step_points[step_idx]);
                k_wild_offset = k_wild_offset.add_mod_n(&self.step_distances[step_idx]);

                if !wild_point.z.is_zero() {
                    wild_x_norm = normalize_x(&wild_point);

                    if wild_x_norm.limbs[0] & dp_mask == 0 {
                        let dp_key = wild_x_norm.to_bytes();

                        bloom_checks += 1;
                        if tame_bloom.contains(&dp_key) {
                            bloom_hits += 1;
                            if let Some(&tame_dist) = tame_exact.get(&dp_key) {
                                collisions += 1;
                                if let Some(k) = self.try_recover(&tame_dist, &k_wild_offset, range_start, range_end) {
                                    let elapsed = start_time.elapsed().as_millis() as u64;
                                    println!("\n  [BLOOM] KEY FOUND! Bloom hits: {}, FPs: {}", bloom_hits, false_positives);
                                    return BloomKangarooResult {
                                        found: true, k: Some(k), hops: total_hops,
                                        tame_dps: tame_exact.len(), wild_dps: wild_exact.len(),
                                        bloom_checks, bloom_hits, false_positives, collisions, elapsed_ms: elapsed,
                                    };
                                }
                            } else {
                                false_positives += 1;
                            }
                        }
                        wild_bloom.insert(&dp_key);
                        wild_exact.insert(dp_key, k_wild_offset.clone());
                    }
                }
            }

            // Progress
            if total_hops - last_report >= report_interval {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = total_hops as f64 / elapsed;
                let fp_rate = if bloom_hits > 0 { false_positives as f64 / bloom_hits as f64 } else { 0.0 };
                println!("  [BLOOM] Hops: {} | Rate: {:.0}/s | DPs: {}+{} | Coll: {} | Bloom FP rate: {:.1}%",
                         total_hops, rate, tame_exact.len(), wild_exact.len(), collisions, fp_rate * 100.0);
                last_report = total_hops;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        println!("  [BLOOM] Not found. Bloom hits: {}, FPs: {} ({:.1}% FP rate)",
                 bloom_hits, false_positives,
                 if bloom_hits > 0 { false_positives as f64 / bloom_hits as f64 * 100.0 } else { 0.0 });

        BloomKangarooResult {
            found: false, k: None, hops: max_hops,
            tame_dps: tame_exact.len(), wild_dps: wild_exact.len(),
            bloom_checks, bloom_hits, false_positives, collisions, elapsed_ms: elapsed,
        }
    }

    fn try_recover(&self, k_tame: &Fe, k_wild: &Fe, range_start: &Fe, range_end: &Fe) -> Option<Fe> {
        let k_pos = k_tame.sub_mod_n(k_wild);
        if let Some(k) = self.check_candidate(&k_pos, range_start, range_end) { return Some(k); }
        let k_neg = k_tame.add_mod_n(k_wild).neg_mod_n();
        if let Some(k) = self.check_candidate(&k_neg, range_start, range_end) { return Some(k); }
        None
    }

    fn check_candidate(&self, k: &Fe, range_start: &Fe, range_end: &Fe) -> Option<Fe> {
        if k.cmp_val(&range_start.limbs).is_ge() && k.cmp_val(&range_end.limbs).is_lt() {
            let q_check = self.g.scalar_mul(k);
            if !q_check.inf && q_check.x == self.q.x { return Some(k.clone()); }
        }
        let autos = self.glv.automorphism_scalars(k);
        for ak in &autos {
            if ak.cmp_val(&range_start.limbs).is_ge() && ak.cmp_val(&range_end.limbs).is_lt() {
                let verify = self.g.scalar_mul(ak);
                if !verify.inf && verify.x == self.q.x { return Some(ak.clone()); }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter_basic() {
        let mut bf = BloomFilter::new(10000, 0.01);
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let key3 = [3u8; 32];
        bf.insert(&key1);
        bf.insert(&key2);
        assert!(bf.contains(&key1));
        assert!(bf.contains(&key2));
        assert!(!bf.contains(&key3));
    }

    #[test]
    fn test_rolling_bloom() {
        let mut rb = RollingBloom::new(4, 1000);
        let key1 = [1u8; 32];
        rb.insert(&key1);
        assert!(rb.contains(&key1));
    }
}
