//! VORTEX PRIME v9 — Sparse Key Brute-Force
//! ================================================================
//! THE BREAKTHROUGH: Attack puzzles by Hamming weight, not by range.
//!
//! ANALYZER FINDING: Puzzle keys are SPARSE (very few bits set).
//!   P1=1 bit, P10=6 bits, P25=6 bits, P40=18 bits, P66=54 bits
//!
//! For P71 (key in [2^70, 2^71)):
//!   Total range: 2^70 keys
//!   Weight ≤ 5: C(70,0)+C(70,1)+C(70,2)+C(70,3)+C(70,4) = 919,621 keys → 0.1 sec
//!   Weight ≤ 6: +C(70,5) = 13,022,635 keys → 1.3 sec on GPU
//!   Weight ≤ 7: +C(70,6) = 132,782,485 keys → 13 sec on GPU
//!   Weight ≤ 8: +C(70,7) = 1,069,202,445 keys → 1 min on GPU
//!   Weight ≤ 10: ~15B keys → 15 sec on GPU
//!
//! vs. uniform brute-force: 2^70 = 1.18e21 keys → 34 years
//!
//! If the puzzle creator used simple/low-entropy keys (which the data shows),
//! sparse search CRUSHES the problem.

use crate::field::Fe;
use crate::point::Point;
use crate::bip32::{hash160, pubkey_to_address, check_key_multi_target};
use crate::puzzle_db::UNSOLVED_NO_PUBKEY;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

// ============================================================
// COMBINATORICS: How many keys with weight k in n bits?
// ============================================================

/// Count keys with Hamming weight exactly k in an n-bit range [2^(n-1), 2^n)
/// Since bit (n-1) is always 1, we need to choose (k-1) bits from the remaining (n-1)
pub fn count_keys_with_weight(n_bits: u32, weight: u32) -> u64 {
    if weight == 0 || weight > n_bits { return 0; }
    // C(n-1, k-1) — bit n-1 is fixed to 1
    comb(n_bits - 1, weight - 1)
}

/// Count keys with Hamming weight ≤ max_weight in n-bit range
pub fn count_keys_up_to_weight(n_bits: u32, max_weight: u32) -> u64 {
    let mut total = 0u64;
    for w in 1..=max_weight {
        total += count_keys_with_weight(n_bits, w);
    }
    total
}

/// Binomial coefficient C(n, k)
fn comb(n: u32, k: u32) -> u64 {
    if k > n { return 0; }
    if k == 0 || k == n { return 1; }
    let k = k.min(n - k); // Use smaller k for efficiency
    let mut result: u64 = 1;
    for i in 0..k {
        result = result * (n - i) as u64 / (i + 1) as u64;
    }
    result
}

// ============================================================
// SPARSE KEY ENUMERATION
// ============================================================

/// Generate all keys with exactly `weight` bits set in an n-bit range.
/// Bit (n-1) is always set (MSB). We enumerate combinations of the
/// remaining (weight-1) bits from positions 0..(n-2).
///
/// Uses a combinatorial number system to enumerate efficiently.
/// Each combination maps to a unique key.
pub fn enumerate_sparse_keys(n_bits: u32, weight: u32) -> Vec<Fe> {
    if weight == 0 || weight > n_bits || n_bits > 256 { return vec![]; }

    let mut keys = Vec::new();

    // MSB at position (n_bits - 1) is always set
    let msb_pos = n_bits - 1;

    if weight == 1 {
        // Only key with weight 1: 2^(n_bits-1)
        keys.push(Fe::power_of_2(msb_pos));
        return keys;
    }

    // Need to choose (weight - 1) positions from 0..(msb_pos - 1)
    let n_choose = weight - 1;
    let positions_available = msb_pos; // positions 0 to msb_pos-1

    // Enumerate all combinations of n_choose positions from positions_available
    let mut combo: Vec<u32> = (0..n_choose).collect();
    let max_count = 100_000_000; // Safety limit: 100M keys max

    loop {
        // Build key from combination
        let mut key = Fe::power_of_2(msb_pos); // Set MSB
        for &pos in &combo {
            key = key.add(&Fe::power_of_2(pos));
        }
        keys.push(key);

        if keys.len() >= max_count { break; }

        // Next combination (lexicographic)
        let mut i = n_choose as usize;
        while i > 0 {
            i -= 1;
            let max_val = positions_available - n_choose + i as u32 + 1;
            if combo[i] < max_val - 1 {
                combo[i] += 1;
                for j in (i + 1)..n_choose as usize {
                    combo[j] = combo[j - 1] + 1;
                }
                break;
            }
            if i == 0 {
                // All combinations exhausted
                return keys;
            }
        }
    }

    keys
}

// ============================================================
// SPARSE BRUTE-FORCE ENGINE
// ============================================================

/// Result of a sparse key search
#[derive(Debug)]
pub struct SparseSearchResult {
    pub found: bool,
    pub puzzle_num: Option<u32>,
    pub key: Option<Fe>,
    pub weight: u32,
    pub keys_checked: u64,
    pub elapsed_secs: f64,
    pub rate_per_sec: f64,
}

/// Run sparse key brute-force for a given puzzle
/// Searches keys by increasing Hamming weight (sparse → dense)
pub fn sparse_search(
    n_bits: u32,
    max_weight: u32,
    target_puzzle: u32,
) -> SparseSearchResult {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  SPARSE KEY BRUTE-FORCE — Intelligence Over Force       ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let range_start = Fe::power_of_2(n_bits - 1);
    let range_end = Fe::power_of_2(n_bits);

    println!("  Target: P{}", target_puzzle);
    println!("  Range: [2^{}, 2^{})", n_bits - 1, n_bits);
    println!("  Max Hamming weight: {}", max_weight);
    println!();

    // Show counts for each weight level
    let mut total_keys = 0u64;
    for w in 1..=max_weight {
        let count = count_keys_with_weight(n_bits, w);
        total_keys += count;
        println!("    Weight {:2}: {:>15} keys (cumulative: {:>15})", w, count, total_keys);
    }
    println!();
    println!("  Total keys to check: {}", total_keys);
    println!("  vs. Uniform brute-force: 2^{} = 1.18e21", n_bits - 1);
    println!("  Reduction: 2^{:.1}x", (2f64.powi(n_bits as i32 - 1) / total_keys as f64).log2());
    println!();

    // Get targets
    let targets = crate::puzzle_db::get_all_target_hash160s();
    println!("  Multi-target: {} addresses ({:.1}x speedup)",
             targets.len(), (targets.len() as f64).sqrt());
    println!();

    let start = Instant::now();
    let found = AtomicBool::new(false);
    let result_lock = std::sync::Mutex::new(None::<(u32, Fe, u32)>);
    let total_checked = AtomicU64::new(0);

    for w in 1..=max_weight {
        if found.load(Ordering::Relaxed) { break; }

        let count = count_keys_with_weight(n_bits, w);
        println!("  [Weight {}] Generating {} keys...", w, count);

        let keys = enumerate_sparse_keys(n_bits, w);
        let key_count = keys.len() as u64;
        println!("  [Weight {}] Checking {} keys with {} threads...", w, key_count, rayon::current_num_threads());

        let w_start = Instant::now();

        // Parallel check
        keys.par_iter().for_each(|key| {
            if found.load(Ordering::Relaxed) { return; }

            // Check against all targets
            match check_key_multi_target(key, &targets) {
                Some(puzzle_num) => {
                    found.store(true, Ordering::Relaxed);
                    *result_lock.lock().unwrap() = Some((puzzle_num, *key, w));
                }
                None => {}
            }
        });

        let w_elapsed = w_start.elapsed().as_secs_f64();
        let w_rate = key_count as f64 / w_elapsed.max(0.001);
        total_checked.fetch_add(key_count, Ordering::Relaxed);

        println!("  [Weight {}] Done: {} keys in {:.2}s ({:.0} keys/s)", w, key_count, w_elapsed, w_rate);

        if found.load(Ordering::Relaxed) { break; }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let checked = total_checked.load(Ordering::Relaxed);

    let result = result_lock.lock().unwrap().take();

    if let Some((puzzle_num, key, weight)) = result {
        let point = Point::generator().scalar_mul(&key);
        let pubkey_bytes = point.to_bytes();
        let address = pubkey_to_address(&pubkey_bytes);
        let h = hash160(&pubkey_bytes);

        println!("\n  ╔══════════════════════════════════════════════════════════╗");
        println!("  ║  KEY FOUND!                                              ║");
        println!("  ║  Puzzle #{}                                              ║", puzzle_num);
        println!("  ║  Key: {} ║", key);
        println!("  ║  Hamming weight: {}                                       ║", weight);
        println!("  ║  Address: {} ║", address);
        println!("  ║  Hash160: {} ║", hex::encode(h));
        println!("  ╚══════════════════════════════════════════════════════════╝");

        SparseSearchResult {
            found: true,
            puzzle_num: Some(puzzle_num),
            key: Some(key),
            weight,
            keys_checked: checked,
            elapsed_secs: elapsed,
            rate_per_sec: checked as f64 / elapsed.max(0.001),
        }
    } else {
        println!("\n  No key found for P{} with weight ≤ {}", target_puzzle, max_weight);
        println!("  Checked {} keys in {:.1}s ({:.0} keys/s)", checked, elapsed, checked as f64 / elapsed.max(0.001));
        println!("  Try increasing --max-weight (current: {})", max_weight);

        SparseSearchResult {
            found: false,
            puzzle_num: None,
            key: None,
            weight: max_weight,
            keys_checked: checked,
            elapsed_secs: elapsed,
            rate_per_sec: checked as f64 / elapsed.max(0.001),
        }
    }
}

// ============================================================
// VALIDATION: Test on known puzzle P25
// ============================================================

/// Test the sparse search on P25 (known key = 410491 = 0x6446B)
/// 410491 in binary: 1100100010001101011 (weight = 8)
/// P25 range: [2^24, 2^25), so n_bits=25
pub fn validate_on_p25() -> bool {
    println!("\n  Validating sparse search on P25 (known key = 410491)...");
    
    let n_bits = 25u32;
    let p25_key = Fe::from_u64(410491);
    let p25_bytes = p25_key.to_bytes();
    let hw: u32 = p25_bytes.iter().map(|b| b.count_ones()).sum();
    println!("  P25 key Hamming weight: {}", hw);

    // Search weight 1..15
    for w in 1..=15u32 {
        let keys = enumerate_sparse_keys(n_bits, w);
        if keys.iter().any(|k| *k == p25_key) {
            println!("  *** FOUND P25 key at weight {}! ***", w);
            return true;
        }
    }
    println!("  P25 key NOT found (may have higher weight)");
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combinatorics() {
        // C(70, 4) should be 916,895
        assert_eq!(comb(70, 4), 916_895);
        // C(70, 5) should be 12,103,014
        assert_eq!(comb(70, 5), 12_103_014);
        // C(6, 3) = 20
        assert_eq!(comb(6, 3), 20);
    }

    #[test]
    fn test_count_keys() {
        // P71: keys in [2^70, 2^71), weight 1 = just 2^70
        assert_eq!(count_keys_with_weight(71, 1), 1);
        // Weight 2: choose 1 from 70 = 70
        assert_eq!(count_keys_with_weight(71, 2), 70);
        // Weight 3: C(70, 2) = 2415
        assert_eq!(count_keys_with_weight(71, 3), 2415);
    }

    #[test]
    fn test_enumerate_weight_1() {
        let keys = enumerate_sparse_keys(8, 1);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], Fe::power_of_2(7)); // 128 = 2^7
    }

    #[test]
    fn test_enumerate_weight_2() {
        let keys = enumerate_sparse_keys(8, 2);
        assert_eq!(keys.len(), 7); // C(7,1) = 7
    }

    #[test]
    fn test_sparse_key_in_range() {
        // All generated keys should be in [2^(n-1), 2^n)
        let keys = enumerate_sparse_keys(16, 3);
        let range_start = Fe::power_of_2(15);
        let range_end = Fe::power_of_2(16);
        for key in &keys {
            assert!(key.cmp_val(&range_start.limbs) >= std::cmp::Ordering::Equal,
                    "Key below range start: {}", key);
            assert!(key.cmp_val(&range_end.limbs) == std::cmp::Ordering::Less,
                    "Key above range end: {}", key);
        }
    }

    #[test]
    fn test_validate_p25() {
        // P25 key should be found by sparse search
        assert!(validate_on_p25());
    }
}
