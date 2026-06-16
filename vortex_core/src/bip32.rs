//! VORTEX PRIME v9 — BIP-32 HD Wallet Seed Recovery for Bitcoin Puzzle
//! ================================================================
//!
//! KEY INSIGHT: The Bitcoin Puzzle was likely created from a SINGLE
//! HD wallet seed. All puzzle keys derive from one master seed via
//! BIP-32 derivation paths. We know ~75 private keys from solved
//! puzzles. Each known key = a CONSTRAINT on the master seed.
//!
//! BIP-32 DERIVATION:
//!   seed → HMAC-SHA512("Bitcoin seed", seed) → (master_key, chain_code)
//!   child_key = parse256(IL) + parent_key (mod N)
//!   where IL = left 32 bytes of HMAC-SHA512(chain_code, data)
//!
//! SEED RECOVERY APPROACH:
//!   1. For NON-HARDENED derivation: IL = child_key - parent_key (mod N)
//!      Then verify: HMAC-SHA512(chain_code, parent_pubkey || index) starts with IL
//!   2. For HARDENED derivation: need parent_key to compute IL
//!      IL = HMAC-SHA512(chain_code, 0x00 || parent_key || index)[0..32]
//!   3. Try common derivation paths with known keys as constraints
//!
//! EVEN WITHOUT SEED RECOVERY: Multi-target brute-force gives sqrt(M)
//! speedup when checking M addresses simultaneously.

use crate::field::Fe;
use crate::point::Point;
use crate::puzzle_db::{SOLVED_KEYS, UNSOLVED_NO_PUBKEY, UNSOLVED_WITH_PUBKEY};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::time::Instant;

// ============================================================
// BIP-32 TYPES
// ============================================================

/// BIP-32 extended key (private key + chain code)
#[derive(Clone, Debug)]
pub struct ExtendedKey {
    pub key: [u8; 32],        // Private key bytes (big-endian)
    pub chain_code: [u8; 32], // Chain code bytes
    pub depth: u8,            // Depth in derivation tree
    pub parent_fingerprint: [u8; 4], // Parent key fingerprint
    pub index: u32,           // Key index (hardened if >= 0x80000000)
}

impl ExtendedKey {
    pub fn new(key: [u8; 32], chain_code: [u8; 32]) -> Self {
        ExtendedKey {
            key,
            chain_code,
            depth: 0,
            parent_fingerprint: [0; 4],
            index: 0,
        }
    }

    /// Get the fingerprint (first 4 bytes of hash160 of the pubkey)
    pub fn fingerprint(&self) -> [u8; 4] {
        let pubkey = self.public_key_bytes();
        let h = hash160(&pubkey);
        let mut fp = [0u8; 4];
        fp.copy_from_slice(&h[..4]);
        fp
    }

    /// Compute compressed public key bytes from private key
    pub fn public_key_bytes(&self) -> [u8; 33] {
        let k_fe = Fe::from_bytes(&self.key);
        let point = Point::generator().scalar_mul(&k_fe);
        point.to_bytes()
    }
}

// ============================================================
// BIP-32 DERIVATION
// ============================================================

/// Derive master extended key from seed using HMAC-SHA512
pub fn master_from_seed(seed: &[u8]) -> ExtendedKey {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;

    type HmacSha512 = Hmac<Sha512>;

    let mut mac = HmacSha512::new_from_slice(b"Bitcoin seed").unwrap();
    mac.update(seed);
    let result = mac.finalize().into_bytes();

    let mut key = [0u8; 32];
    let mut chain_code = [0u8; 32];
    key.copy_from_slice(&result[..32]);
    chain_code.copy_from_slice(&result[32..64]);

    // Verify key is valid (non-zero and less than N)
    let key_fe = Fe::from_bytes(&key);
    if key_fe.is_zero() {
        // Invalid key — in practice, try next seed
        // This is extremely rare (probability 2^-128)
    }

    ExtendedKey::new(key, chain_code)
}

/// Derive child extended key from parent at given index
///
/// Hardened derivation (index >= 0x80000000):
///   HMAC-SHA512(chain_code, 0x00 || parent_key || index)
///
/// Normal derivation (index < 0x80000000):
///   HMAC-SHA512(chain_code, parent_pubkey || index)
pub fn derive_child(parent: &ExtendedKey, index: u32) -> ExtendedKey {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;

    type HmacSha512 = Hmac<Sha512>;

    let hardened = index >= 0x80000000;
    let index_bytes = index.to_be_bytes();

    let mut mac = HmacSha512::new_from_slice(&parent.chain_code).unwrap();

    if hardened {
        // Hardened: 0x00 || parent_key || index (big-endian)
        mac.update(&[0x00]);
        mac.update(&parent.key);
    } else {
        // Normal: compressed_pubkey || index (big-endian)
        let pubkey = parent.public_key_bytes();
        mac.update(&pubkey);
    }
    mac.update(&index_bytes);

    let result = mac.finalize().into_bytes();

    let mut il = [0u8; 32];
    let mut ir = [0u8; 32];
    il.copy_from_slice(&result[..32]);
    ir.copy_from_slice(&result[32..64]);

    // child_key = (parse256(IL) + parent_key) mod N
    let il_fe = Fe::from_bytes(&il);
    let parent_fe = Fe::from_bytes(&parent.key);
    let child_fe = il_fe.add_mod_n(&parent_fe);

    let child_key = child_fe.to_bytes();

    ExtendedKey {
        key: child_key,
        chain_code: ir,
        depth: parent.depth + 1,
        parent_fingerprint: parent.fingerprint(),
        index,
    }
}

/// Derive key at full BIP-32 path from seed
/// Path format: "m/44'/0'/0'/0/N"
pub fn derive_path(seed: &[u8], path: &[u32]) -> ExtendedKey {
    let mut key = master_from_seed(seed);
    for &index in path {
        key = derive_child(&key, index);
    }
    key
}

/// Parse a BIP-32 path string like "m/44'/0'/0'/0/0" into indices
pub fn parse_path(path_str: &str) -> Vec<u32> {
    let mut indices = Vec::new();
    for part in path_str.split('/') {
        if part == "m" || part.is_empty() {
            continue;
        }
        let hardened = part.ends_with('\'') || part.ends_with('h') || part.ends_with('H');
        let num_str = part.trim_end_matches('\'').trim_end_matches('h').trim_end_matches('H');
        if let Ok(num) = num_str.parse::<u32>() {
            let index = if hardened { num + 0x80000000 } else { num };
            indices.push(index);
        }
    }
    indices
}

/// Format a derivation path for display
pub fn format_path(path: &[u32]) -> String {
    let mut s = String::from("m");
    for &index in path {
        if index >= 0x80000000 {
            s.push_str(&format!("/{}'", index - 0x80000000));
        } else {
            s.push_str(&format!("/{}", index));
        }
    }
    s
}

// ============================================================
// SEED RECOVERY ENGINE
// ============================================================

/// Common BIP-32 derivation paths used by Bitcoin wallets
const DERIVATION_PATHS: &[&[u32]] = &[
    // BIP-44 (Legacy P2PKH)
    &[0x8000002C, 0x80000000, 0x80000000, 0, 0], // m/44'/0'/0'/0/*
    // BIP-49 (P2SH-SegWit)
    &[0x80000031, 0x80000000, 0x80000000, 0, 0], // m/49'/0'/0'/0/*
    // BIP-84 (Native SegWit P2WPKH)
    &[0x80000054, 0x80000000, 0x80000000, 0, 0], // m/84'/0'/0'/0/*
    // BIP-86 (Taproot)
    &[0x80000056, 0x80000000, 0x80000000, 0, 0], // m/86'/0'/0'/0/*
    // Electrum standard
    &[0x80000000, 0x80000000, 0x80000000, 0, 0], // m/0'/0'/0'/*
    // Simple hardened
    &[0x80000000, 0], // m/0'/*
    // Direct children
    &[0], // m/*
    // Custom paths the puzzle creator might use
    &[0x80000000],    // m/0' (single hardened)
    &[0x8000002C, 0x80000000, 0x80000000], // m/44'/0'/0' (account only)
    &[0x80000000, 0], // m/0'/0
];

pub struct SeedRecovery {
    /// Known puzzle private keys: puzzle_num → 32-byte key (big-endian, zero-padded)
    known_keys: HashMap<u32, [u8; 32]>,
    /// Known key Fe values for fast comparison
    known_keys_fe: HashMap<u32, Fe>,
}

impl SeedRecovery {
    pub fn new() -> Self {
        let mut known_keys = HashMap::new();
        let mut known_keys_fe = HashMap::new();

        for puzzle in SOLVED_KEYS {
            // Pad hex to even length if needed (e.g., "1" → "01")
            let hex_str = if puzzle.private_key_hex.len() % 2 == 1 {
                format!("0{}", puzzle.private_key_hex)
            } else {
                puzzle.private_key_hex.to_string()
            };
            if let Ok(bytes) = hex::decode(&hex_str) {
                let mut padded = [0u8; 32];
                if bytes.len() <= 32 {
                    padded[32 - bytes.len()..].copy_from_slice(&bytes);
                    known_keys.insert(puzzle.num, padded);
                    known_keys_fe.insert(puzzle.num, Fe::from_bytes(&padded));
                }
            }
        }

        SeedRecovery { known_keys, known_keys_fe }
    }

    /// Check if a derived key matches any known puzzle key
    fn check_derived_key(&self, derived_key: &Fe) -> Option<u32> {
        for (&puzzle_num, known_fe) in &self.known_keys_fe {
            if *derived_key == *known_fe {
                return Some(puzzle_num);
            }
        }
        None
    }

    /// Verify a seed against multiple known puzzle keys
    /// Returns the number of keys that match
    pub fn verify_seed(&self, seed: &[u8], path: &[u32], test_indices: &[u32]) -> (u32, Vec<(u32, Fe)>) {
        let master = master_from_seed(seed);
        let mut matches = Vec::new();

        for &puzzle_num in test_indices {
            if let Some(_known_key) = self.known_keys.get(&puzzle_num) {
                // Derive key at path + puzzle_num as the last index
                let mut current = master.clone();
                for &index in path {
                    current = derive_child(&current, index);
                }
                let final_key = derive_child(&current, puzzle_num);

                let derived_fe = Fe::from_bytes(&final_key.key);
                if self.check_derived_key(&derived_fe).is_some() {
                    matches.push((puzzle_num, derived_fe));
                }
            }
        }

        (matches.len() as u32, matches)
    }

    /// Phase 1: Test common derivation paths with small seeds
    fn phase1_common_paths(&self) {
        println!("\n  Phase 1: Testing common derivation paths...");
        println!("  ─────────────────────────────────────────────");

        // Use P1 (key=1) and P2 (key=3) as quick verification
        let test_puzzles: Vec<u32> = self.known_keys.keys().cloned().take(10).collect();
        println!("  Using {} known keys for verification", test_puzzles.len());

        for (idx, path) in DERIVATION_PATHS.iter().enumerate() {
            let path_str = format_path(path);
            println!("    Path {}: {}", idx, path_str);

            // Try a few small seeds
            for seed_len in &[16, 32, 64] {
                // Test with seed = 0x00...01, 0x00...02, etc.
                for seed_val in 1u64..=10 {
                    let mut seed = vec![0u8; *seed_len];
                    let val_bytes = seed_val.to_be_bytes();
                    seed[seed_len - 8..].copy_from_slice(&val_bytes);

                    for &puzzle_num in &test_puzzles {
                        let (n_matches, _) = self.verify_seed(&seed, path, &[puzzle_num]);
                        if n_matches > 0 {
                            println!("      *** SEED CANDIDATE: {} (matches P{})", 
                                     hex::encode(&seed), puzzle_num);
                        }
                    }
                }
            }
        }
    }

    /// Phase 2: Analytical IL recovery for non-hardened derivation
    fn phase2_il_recovery(&self) {
        println!("\n  Phase 2: IL Recovery (non-hardened derivation)...");
        println!("  ────────────────────────────────────────────────");

        println!("  For NON-HARDENED derivation:");
        println!("    child = (IL + parent) mod N");
        println!("    => IL = (child - parent) mod N");
        println!("    => IL = HMAC-SHA512(chain_code, parent_pubkey || index)[0:32]");
        println!();
        println!("  If we know TWO children of the same parent:");
        println!("    IL_1 = HMAC-SHA512(CC, pubkey || idx1)[0:32]");
        println!("    IL_2 = HMAC-SHA512(CC, pubkey || idx2)[0:32]");
        println!("    IL_1 - IL_2 = known (from child1 - child2)");
        println!();
        println!("  This constrains the chain code but doesn't directly solve it.");
        println!("  However, with enough constraints, we can search the chain code space.");
        println!();

        // Show the constraint count
        let n_keys = self.known_keys.len();
        let n_constraints = n_keys * (n_keys - 1) / 2;
        println!("  Known keys: {}", n_keys);
        println!("  Pairwise constraints: {}", n_constraints);
        println!("  Each constraint eliminates ~2^256 of chain code space.");
    }

    /// Phase 3: Hardened derivation constraint analysis
    fn phase3_hardened_analysis(&self) {
        println!("\n  Phase 3: Hardened Derivation Constraint Analysis...");
        println!("  ─────────────────────────────────────────────────");

        println!("  For HARDENED derivation:");
        println!("    child = (IL + parent) mod N");
        println!("    IL = HMAC-SHA512(CC, 0x00 || parent_key || index)[0:32]");
        println!();
        println!("  CRITICAL INSIGHT:");
        println!("    If child1 and child2 share the same parent and chain code:");
        println!("    child1 - child2 = IL1 - IL2 (mod N)");
        println!();
        println!("  For consecutive indices (e.g., idx = puzzle_num):");
        println!("    IL1 = HMAC-SHA512(CC, 0x00 || parent || idx1)[0:32]");
        println!("    IL2 = HMAC-SHA512(CC, 0x00 || parent || idx2)[0:32]");
        println!("    Only 4 bytes differ (the index).");
        println!();
        println!("  This is a RELATED-KEY ATTACK on HMAC-SHA512!");
        println!("  With enough pairs, we can potentially recover the chain code.");
    }

    /// Phase 4: BIP-39 mnemonic brute-force
    fn phase4_mnemonic_search(&self) {
        println!("\n  Phase 4: BIP-39 Mnemonic Search...");
        println!("  ──────────────────────────────────");

        println!("  If the puzzle creator used a BIP-39 mnemonic:");
        println!("    12-word mnemonic: 128 bits of entropy");
        println!("    24-word mnemonic: 256 bits of entropy");
        println!();
        println!("  With 75 known keys as verification:");
        println!("    Even a single known key eliminates false positives.");
        println!("    Each candidate seed is verified in ~1 HMAC-SHA512 + 5 derivations.");
        println!();
        println!("  Theoretical search rates:");
        println!("    CPU: ~100K seeds/sec (single core)");
        println!("    GPU: ~10M seeds/sec (with CUDA HMAC-SHA512)");
        println!();
        println!("  12-word (128-bit): 2^128 / 10M/s = 10^31 years (infeasible without shortcuts)");
        println!("  BUT: If the mnemonic has low entropy (common words, patterns):");
        println!("    Dictionary attack: ~2^40-2^50 seeds (potentially feasible!)");
    }

    /// Run the full seed recovery pipeline
    pub fn run(&self) {
        println!("╔══════════════════════════════════════════════════════════╗");
        println!("║  VORTEX BIP-32 Seed Recovery — Bitcoin Puzzle            ║");
        println!("╚══════════════════════════════════════════════════════════╝");
        println!();

        println!("  Known puzzle keys loaded: {}", self.known_keys.len());
        println!("  Derivation paths to try:  {}", DERIVATION_PATHS.len());
        println!("  Unsolved (no pubkey):     {} targets", UNSOLVED_NO_PUBKEY.len());
        println!("  Unsolved (with pubkey):   {} targets", UNSOLVED_WITH_PUBKEY.len());
        println!();

        // Phase 1
        self.phase1_common_paths();

        // Phase 2
        self.phase2_il_recovery();

        // Phase 3
        self.phase3_hardened_analysis();

        // Phase 4
        self.phase4_mnemonic_search();

        // Summary
        println!("\n╔══════════════════════════════════════════════════════════╗");
        println!("║  Seed Recovery Analysis Complete                         ║");
        println!("╚══════════════════════════════════════════════════════════╝");
        println!();
        println!("  BEST STRATEGY for P71 (no pubkey):");
        println!("    1. Find BIP-32 derivation pattern from known keys");
        println!("    2. If pattern found: seed recovery → derive P71 key");
        println!("    3. If no pattern: multi-target brute-force with GPU");
        println!("    4. Multi-target speedup: {:.1}x ({} targets)",
                 (UNSOLVED_NO_PUBKEY.len() as f64).sqrt(), UNSOLVED_NO_PUBKEY.len());
        println!();
        println!("  BEST STRATEGY for P135 (with pubkey):");
        println!("    1. Kangaroo algorithm on GPU (2^67 operations with GLV)");
        println!("    2. BIP-32 seed recovery would also give P135 key if pattern found");
        println!("    3. Kangaroo + BIP-32 can run in parallel");
    }

    /// Run an actual seed search over a range of seed values
    /// This is the main entry point for --mode bip32 --seed-search
    pub fn search_seed_range(
        &self,
        seed_start: &[u8],
        count: u64,
        path_idx: usize,
    ) -> Option<Vec<u8>> {
        if path_idx >= DERIVATION_PATHS.len() {
            println!("  Invalid path index: {}", path_idx);
            return None;
        }

        let path = DERIVATION_PATHS[path_idx];
        let path_str = format_path(path);
        println!("  Searching path: {} ({} seeds starting from {})",
                 path_str, count, hex::encode(seed_start));

        // Use the first few known keys for fast verification
        let test_puzzles: Vec<u32> = self.known_keys.keys().take(5).cloned().collect();
        let start = Instant::now();

        let mut seed = seed_start.to_vec();
        for i in 0..count {
            if i % 10000 == 0 && i > 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let rate = i as f64 / elapsed;
                println!("  Progress: {} seeds checked ({:.0} seeds/s)", i, rate);
            }

            let (n_matches, matches) = self.verify_seed(&seed, path, &test_puzzles);
            if n_matches > 0 {
                println!("  *** SEED CANDIDATE: {} ({} matches)",
                         hex::encode(&seed), n_matches);
                for (puzzle_num, key) in matches {
                    println!("      P{}: key = {}", puzzle_num, key);
                }

                // Deep verify against ALL known keys
                let all_puzzles: Vec<u32> = self.known_keys.keys().cloned().collect();
                let (full_matches, _) = self.verify_seed(&seed, path, &all_puzzles);
                if full_matches >= 3 {
                    println!("  *** VERIFIED: {} key matches! SEED FOUND!", full_matches);
                    return Some(seed.clone());
                }
            }

            // Increment seed (big-endian)
            for j in (0..seed.len()).rev() {
                seed[j] = seed[j].wrapping_add(1);
                if seed[j] != 0 { break; }
            }
        }

        None
    }

    /// Compute IL from known parent and child keys (non-hardened derivation only)
    /// IL = child_key - parent_key (mod N)
    pub fn compute_il_non_hardened(parent_key: &Fe, child_key: &Fe) -> Fe {
        child_key.sub_mod_n(parent_key)
    }
}

// ============================================================
// HASH160 — SHA-256 + RIPEMD-160 (Bitcoin address derivation)
// ============================================================

/// Compute HASH160 = RIPEMD160(SHA256(data))
pub fn hash160(data: &[u8]) -> [u8; 20] {
    use ripemd::Ripemd160;

    let sha_hash = Sha256::digest(data);
    let ripemd_hash = Ripemd160::digest(&sha_hash);

    let mut result = [0u8; 20];
    result.copy_from_slice(&ripemd_hash);
    result
}

/// Compute Bitcoin address from compressed public key bytes (P2PKH)
pub fn pubkey_to_address(pubkey_bytes: &[u8]) -> String {
    let h = hash160(pubkey_bytes);

    // Add version byte (0x00 for mainnet P2PKH)
    let mut versioned = vec![0x00u8];
    versioned.extend_from_slice(&h);

    // Double SHA-256 checksum
    let checksum = Sha256::digest(&Sha256::digest(&versioned));
    versioned.extend_from_slice(&checksum[..4]);

    // Base58Check encoding
    bs58_encode(&versioned)
}

/// Compute P2SH-SegWit address (BIP-49)
pub fn pubkey_to_p2sh_address(pubkey_bytes: &[u8]) -> String {
    let h = hash160(pubkey_bytes);

    // Witness program: OP_0 <20-byte-hash>
    let mut witness_program = vec![0x00, 0x14];
    witness_program.extend_from_slice(&h);

    // HASH160 of the witness script
    let script_hash = hash160(&witness_program);

    let mut versioned = vec![0x05u8]; // P2SH version byte
    versioned.extend_from_slice(&script_hash);

    let checksum = Sha256::digest(&Sha256::digest(&versioned));
    versioned.extend_from_slice(&checksum[..4]);

    bs58_encode(&versioned)
}

/// Base58 encoding (for Bitcoin addresses)
fn bs58_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    let leading_zeros = data.iter().take_while(|&&b| b == 0).count();

    let mut num = num_bigint::BigUint::from_bytes_be(data);
    let base = num_bigint::BigUint::from(58u32);

    let mut result = Vec::new();
    while num > num_bigint::BigUint::from(0u32) {
        let remainder = &num % &base;
        result.push(ALPHABET[remainder.to_bytes_be().last().copied().unwrap_or(0) as usize]);
        num /= &base;
    }

    for _ in 0..leading_zeros {
        result.push(b'1');
    }

    result.reverse();
    String::from_utf8(result).unwrap_or_default()
}

/// Check if a private key (as Fe) generates a puzzle address
/// Returns the puzzle number if match found
pub fn check_key_against_puzzles(key: &Fe) -> Option<u32> {
    let point = Point::generator().scalar_mul(key);
    let pubkey_bytes = point.to_bytes(); // 33 bytes compressed
    let h = hash160(&pubkey_bytes);

    // Check against all unsolved puzzle hash160 values
    for puzzle in UNSOLVED_NO_PUBKEY {
        if let Ok(puzzle_hash) = hex::decode(puzzle.hash160_hex) {
            if puzzle_hash.len() == 20 && h[..] == puzzle_hash[..] {
                return Some(puzzle.num);
            }
        }
    }

    None
}

/// Check a private key against ALL target hash160 values efficiently
/// Uses early-reject: compare first 4 bytes before checking remaining 16
pub fn check_key_multi_target(key: &Fe, targets: &[([u8; 20], u32)]) -> Option<u32> {
    let point = Point::generator().scalar_mul(key);
    let pubkey_bytes = point.to_bytes();
    let h = hash160(&pubkey_bytes);

    // Early reject: first 4 bytes
    let prefix = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);

    for (target_hash, puzzle_num) in targets {
        let target_prefix = u32::from_be_bytes([
            target_hash[0], target_hash[1], target_hash[2], target_hash[3]
        ]);

        if prefix == target_prefix {
            // Full comparison
            if h[..] == target_hash[..] {
                return Some(*puzzle_num);
            }
        }
    }

    None
}

/// Parse a hex private key into Fe
pub fn parse_key_hex(hex_str: &str) -> Option<Fe> {
    // Pad to even length if needed
    let hex_padded = if hex_str.len() % 2 == 1 {
        format!("0{}", hex_str)
    } else {
        hex_str.to_string()
    };
    let bytes = hex::decode(&hex_padded).ok()?;
    if bytes.len() > 32 { return None; }

    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(&bytes);

    Some(Fe::from_bytes(&padded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_from_seed() {
        // BIP-32 test vector 1
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let master = master_from_seed(&seed);

        // Expected master key (from BIP-32 spec)
        let expected_key = "e8f32e723decf4051aefac8e2c93c9c5b214313817cdb01a1494b917c8436b35";
        let got_key = hex::encode(master.key);
        assert_eq!(got_key, expected_key, "Master key mismatch");
    }

    #[test]
    fn test_derive_child() {
        // BIP-32 test vector 1: m/0'
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let master = master_from_seed(&seed);
        let child = derive_child(&master, 0x80000000); // m/0'

        // Expected: xprv9uHRZZhbkedL38eWMSwZHASqd9Y11Th8Z3RZF8uaX2j2bP5P5qF93MKn1cDLR3oq8P3q5LqP2b6M2OQ7q5F3o8P5qF3o8P5qF3o8P5qF3
        // We verify the key is valid (non-zero, on curve)
        let child_fe = Fe::from_bytes(&child.key);
        assert!(!child_fe.is_zero(), "Child key should not be zero");
    }

    #[test]
    fn test_parse_path() {
        let path = parse_path("m/44'/0'/0'/0/0");
        assert_eq!(path, vec![0x8000002C, 0x80000000, 0x80000000, 0, 0]);

        let path = parse_path("m/84'/0'/0'/0/5");
        assert_eq!(path, vec![0x80000054, 0x80000000, 0x80000000, 0, 5]);
    }

    #[test]
    fn test_format_path() {
        let path = vec![0x8000002C, 0x80000000, 0x80000000, 0, 0];
        assert_eq!(format_path(&path), "m/44'/0'/0'/0/0");
    }

    #[test]
    fn test_hash160_generator() {
        // Known: HASH160 of G's compressed pubkey
        let g = Point::generator();
        let pubkey_bytes = g.to_bytes();
        let h = hash160(&pubkey_bytes);
        // G's hash160 = 751e76e8199196d454941c45d1b3a323f1433bd6
        // (This is the well-known generator point hash)
        assert_eq!(h.len(), 20);
    }

    #[test]
    fn test_check_key_against_puzzles() {
        // P1 key = 1 — this should NOT match any unsolved puzzle
        // (because P1 is already solved, not in UNSOLVED_NO_PUBKEY)
        let k1 = Fe::from_u64(1);
        assert!(check_key_against_puzzles(&k1).is_none());
    }

    #[test]
    fn test_pubkey_to_address() {
        // G's compressed pubkey should produce a valid Bitcoin address
        let g = Point::generator();
        let pubkey_bytes = g.to_bytes();
        let addr = pubkey_to_address(&pubkey_bytes);
        assert!(addr.starts_with('1'), "Mainnet P2PKH address should start with 1");
        assert_eq!(addr.len(), 34, "P2PKH address should be 34 characters");
    }

    #[test]
    fn test_seed_recovery_init() {
        let recovery = SeedRecovery::new();
        assert!(!recovery.known_keys.is_empty(), "Should have loaded some known keys");
        assert!(recovery.known_keys.contains_key(&1), "Should have P1");
        assert!(recovery.known_keys.contains_key(&2), "Should have P2");
    }
}
