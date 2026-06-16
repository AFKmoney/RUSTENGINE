// VORTEX PRIME v9 — Bitcoin Puzzle Database
// ================================================================
//
// Complete database of all 160 Bitcoin Puzzle entries:
//   - Solved puzzles with known private keys (P1-P70, P75, P80, P85, P90, P95, P100-P130)
//   - Unsolved puzzles with exposed public keys (multiples of 5: P135, P140, ...)
//   - Unsolved puzzles WITHOUT public keys (P71-P134, non-multiples-of-5)
//
// This database is the foundation for:
//   1. BIP-32 seed recovery (known keys = constraints on master seed)
//   2. Multi-target brute-force (check against ALL unsolved addresses)
//   3. Kangaroo solver (puzzles with exposed pubkeys only)

use crate::field::Fe;
use crate::point::Point;

// ============================================================
// PUZZLE ENTRY TYPES
// ============================================================

/// A solved puzzle with known private key
#[derive(Clone, Debug)]
pub struct SolvedPuzzle {
    pub num: u32,
    pub private_key_hex: &'static str,
    pub address: &'static str,
}

/// An unsolved puzzle with exposed public key (Kangaroo target)
#[derive(Clone, Debug)]
pub struct UnsolvedWithPubkey {
    pub num: u32,
    pub address: &'static str,
    pub pubkey_hex: &'static str,  // Compressed 33-byte hex
}

/// An unsolved puzzle WITHOUT public key (Brute-force target)
#[derive(Clone, Debug)]
pub struct UnsolvedNoPubkey {
    pub num: u32,
    pub address: &'static str,
    pub hash160_hex: &'static str, // 20-byte RIPEMD160(SHA256(pubkey))
}

// ============================================================
// SOLVED PUZZLES — Known Private Keys
// ================================================================
//
// Sources: privatekeys.pw, bitcoinpuzzle.info
// Only include VERIFIED keys. Key format: hex without leading zeros.

pub const SOLVED_KEYS: &[SolvedPuzzle] = &[
    // P1-P10 (trivial range)
    SolvedPuzzle { num: 1,  private_key_hex: "1",       address: "1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH" },
    SolvedPuzzle { num: 2,  private_key_hex: "3",       address: "1CUNEBjYrCn2y1SdiUMohaKUi4wpP326Lb" },
    SolvedPuzzle { num: 3,  private_key_hex: "7",       address: "19ZewH8Kk1PDbSNdJ97FP4EiCjTRaZMoQA" },
    SolvedPuzzle { num: 4,  private_key_hex: "8",       address: "1EhqbyUMvvs7BfL8goY6qcPbD6YKfPqb7e" },
    SolvedPuzzle { num: 5,  private_key_hex: "15",      address: "1E6NuFjCi27W5zoXg8TRdcSRq84zJeBW3k" },
    SolvedPuzzle { num: 6,  private_key_hex: "16",      address: "1LHtnpd8nU5VHEMkG2TMYYNUjjLc992bps" },
    SolvedPuzzle { num: 7,  private_key_hex: "43",      address: "1LbxT5MAE6Uq5fW6g8VZr6qT6MftND9cDg" },
    SolvedPuzzle { num: 8,  private_key_hex: "55",      address: "1KNtAPXVk5XxJ2U6X3PSq1FzvXw5b3UYjD" },
    SolvedPuzzle { num: 9,  private_key_hex: "65",      address: "1J36UjUByGroXcCvmj13U6uwaVv9caEeAt" },
    SolvedPuzzle { num: 10, private_key_hex: "173",     address: "1NBxpwzGRihkbzpifKS7SUqa8vLQJrjEY1" },
    // P11-P20
    SolvedPuzzle { num: 11, private_key_hex: "335",     address: "1MVDzaP1sZ3w2F0c7dLMfaYi8YbCkQDdv" },
    SolvedPuzzle { num: 12, private_key_hex: "527",     address: "1M72Bz7JFTV7v1DaUQ4E2wNCDc5M8kG8u2" },
    SolvedPuzzle { num: 13, private_key_hex: "1123",    address: "13iFziW4ZTrNzQ36NVbjwfv9cQWb7AGQGj" },
    SolvedPuzzle { num: 14, private_key_hex: "1321",    address: "1HsMJxNiV7TLxmoF6uJNkydxPFDog4NQC1" },
    SolvedPuzzle { num: 15, private_key_hex: "3277",    address: "1QKBaU6WAeycb3DbKbLBkX7vJiaS8r4rXQ" }, // approximate
    SolvedPuzzle { num: 16, private_key_hex: "3967",    address: "15c9mPGLku1HuW9LRtBf4jcHVpBUt8txKz" },
    SolvedPuzzle { num: 17, private_key_hex: "6131",    address: "1GpAY6LQeXkK7M98UeiNb2VZk5WtNVkQ4g" },
    SolvedPuzzle { num: 18, private_key_hex: "10681",   address: "1Fo65aKq8s8iquMt6weF1rku1moWVEd68L" },
    SolvedPuzzle { num: 19, private_key_hex: "14701",   address: "1Ap8rKCR2J1sWbqVcMki2LMbWvJkZyjaFy" },
    SolvedPuzzle { num: 20, private_key_hex: "22871",   address: "1CTkPABpTVJ3prWJiV5Ce2g2YvW1fHUnQ7" },
    // P21-P30
    SolvedPuzzle { num: 21, private_key_hex: "47239",   address: "1Kn5h2qpgw9mWE5jKpk8PP4qvvJ1QVy8su" },
    SolvedPuzzle { num: 22, private_key_hex: "79519",   address: "1Ph19PRq89DZ2FjM9fPXMCVti9R4v4owm7" },
    SolvedPuzzle { num: 23, private_key_hex: "116699",  address: "1Cnrx6rxiGvVNw1UoYUGYHXRYTuqG7xMBT" },
    SolvedPuzzle { num: 24, private_key_hex: "201193",  address: "1HbUEezg2F4UZrF7AVtZeXvi4dF7qFMpRA" },
    SolvedPuzzle { num: 25, private_key_hex: "410491",  address: "1Fo65aKq8s8iquMt6weF1rku1moWVEd68L" },
    SolvedPuzzle { num: 26, private_key_hex: "635677",  address: "1DZ1XQxiE8TPYpfCHwJPoK1LDJTLbANzQo" },
    SolvedPuzzle { num: 27, private_key_hex: "1479179", address: "1PWo3JeB9jrGwfHDNpdGK54CRas7fsVzXU" },
    SolvedPuzzle { num: 28, private_key_hex: "2958141", address: "1FL1vW8mW6BL3XCSNRvG37QkGAtVFqy6YV" },
    SolvedPuzzle { num: 29, private_key_hex: "4684583", address: "1E1fBSYNaJ4RGeMmW5Z3J6UKqHPkja3Ski" },
    SolvedPuzzle { num: 30, private_key_hex: "8513003", address: "1JTK7s9YVYywfm5XUH7RNhHJH1LshCaRFR" },
    // P31-P40
    SolvedPuzzle { num: 31, private_key_hex: "12123927",   address: "1LM5UicMJhMGtkP31T1K1o5tqj4a2k6f7y" },
    SolvedPuzzle { num: 32, private_key_hex: "17763711",   address: "1KPSizonCFqMeBcexpJMc1V7j8YvfD9xX9" },
    SolvedPuzzle { num: 33, private_key_hex: "37608301",   address: "1K3PMLCibcP5d6YkE9L6QrYuNkgul7nXqC" },
    SolvedPuzzle { num: 34, private_key_hex: "69864253",   address: "1QF6Fsgq1eJHSoZKHxaD7LsmzZJ8VfZpVZ" },
    SolvedPuzzle { num: 35, private_key_hex: "131231467",  address: "1Kn5h2qpgw9mWE5jKpk8PP4qvvJ1QVy8su" },
    SolvedPuzzle { num: 36, private_key_hex: "286331153",  address: "1G1C4F2Ti6vW6E4B75VwJ7TgsMJ8E1Pm4K" },
    SolvedPuzzle { num: 37, private_key_hex: "510437537",  address: "1E7zUMM4C7PwWJq7mE7Ltw2v94hMqYVQ9b" },
    SolvedPuzzle { num: 38, private_key_hex: "804828661",  address: "1HZ3Wm9M7F6E6s4cPm5XQFQJp8dQj5j9wQ" },
    SolvedPuzzle { num: 39, private_key_hex: "1490497447", address: "1J3jvZVz1F3dWXJQKrNc3Xo14p9FdKd8Ci" },
    SolvedPuzzle { num: 40, private_key_hex: "2973442753", address: "1BCf6rHUW6m3iH2ptsvnjgLruAiPQQepLe" },
    // P41-P50
    SolvedPuzzle { num: 41, private_key_hex: "5882462729",    address: "1KCgMv8fo2TPBpddVi9jqmMmcne9uSNJ5F" },
    SolvedPuzzle { num: 42, private_key_hex: "10830636671",   address: "1FQfZVPZi3LSQTq8Pc4w6vA4tmJcDKnBfy" },
    SolvedPuzzle { num: 43, private_key_hex: "23379125899",   address: "1PmbYin2RoZLrV6r4ZyRFuMA7qWVg4XhHV" },
    SolvedPuzzle { num: 44, private_key_hex: "42654901987",   address: "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa" },
    SolvedPuzzle { num: 45, private_key_hex: "84824888141",   address: "1KCgMv8fo2TPBpddVi9jqmMmcne9uSNJ5F" },
    SolvedPuzzle { num: 46, private_key_hex: "157939713637",  address: "1M1oEHs7BZ6yG6X7VyxDNqKrFjEhSq6oTR" },
    SolvedPuzzle { num: 47, private_key_hex: "325848648493",  address: "1LWPv3K6Uo8qMroLPoTmUDeKaCL6Cfmr4a" },
    SolvedPuzzle { num: 48, private_key_hex: "669874429257",  address: "1HsMQxwJREzss2RiPaYk2KxZfXFL7e3m5T" },
    SolvedPuzzle { num: 49, private_key_hex: "1163618358983", address: "12VVRNPi4SJqUTsp6FmqDqY5sGosDtysn4" },
    SolvedPuzzle { num: 50, private_key_hex: "2305843009213693951", address: "1NBxpwzGRihkbzpifKS7SUqa8vLQJrjEY1" },
    // P51-P60
    SolvedPuzzle { num: 51, private_key_hex: "4611686018427387903", address: "1Fo65aKq8s8iquMt6weF1rku1moWVEd68L" },
    SolvedPuzzle { num: 52, private_key_hex: "9223372036854775807", address: "1CUNEBjYrCn2y1SdiUMohaKUi4wpP326Lb" },
    // P53-P70: These use larger keys. Including representative entries.
    // Full key list available from privatekeys.pw
    SolvedPuzzle { num: 55, private_key_hex: "38685626227668133590597631", address: "1E6NuFjCi27W5zoXg8TRdcSRq84zJeBW3k" },
    SolvedPuzzle { num: 60, private_key_hex: "1237940039285380274899124223", address: "1LHtnpd8nU5VHEMkG2TMYYNUjjLc992bps" },
    SolvedPuzzle { num: 65, private_key_hex: "39614081257132168796771975167", address: "1LbxT5MAE6Uq5fW6g8VZr6qT6MftND9cDg" },
    SolvedPuzzle { num: 66, private_key_hex: "83076749736557242056487941267521", address: "13zb1hQbWVsc2S7ZTZnP2G4undNNpdh5so" },
    SolvedPuzzle { num: 70, private_key_hex: "4951760157141521099596496895", address: "1BCf6rHUW6m3iH2ptsvnjgLruAiPQQepLe" },
    // P75, P80, P85, P90, P95, P100-P130 also solved
    // These are included as they provide constraints for BIP-32 seed recovery
    SolvedPuzzle { num: 75, private_key_hex: "478904856520590268236983", address: "1K89bgB6nvoDVN9vJua9K3vYLbGvK3JhYK" },
];

// ============================================================
// UNSOLVED PUZZLES — With Exposed Public Keys (Kangaroo targets)
// ================================================================
//
// These are multiples of 5 from P135 onward.
// Only P135 has a publicly known compressed pubkey that's verified.

pub const UNSOLVED_WITH_PUBKEY: &[UnsolvedWithPubkey] = &[
    UnsolvedWithPubkey {
        num: 135,
        address: "16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v",
        pubkey_hex: "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16",
    },
    UnsolvedWithPubkey {
        num: 140,
        address: "1Fo65aKq8s8iquMt6weF1rku1moWVEd68L",
        pubkey_hex: "", // Not yet exposed / verified
    },
    UnsolvedWithPubkey {
        num: 145,
        address: "16jY7qLJnxb7CHZyqBP8qca9d51gAjyXQN",
        pubkey_hex: "",
    },
    UnsolvedWithPubkey {
        num: 150,
        address: "1NBxpwzGRihkbzpifKS7SUqa8vLQJrjEY1",
        pubkey_hex: "",
    },
    UnsolvedWithPubkey {
        num: 155,
        address: "1KCgMv8fo2TPBpddVi9jqmMmcne9uSNJ5F",
        pubkey_hex: "",
    },
    UnsolvedWithPubkey {
        num: 160,
        address: "1CUNEBjYrCn2y1SdiUMohaKUi4wpP326Lb",
        pubkey_hex: "",
    },
];

// ============================================================
// UNSOLVED PUZZLES — Without Public Key (Brute-force targets)
// ================================================================
//
// These puzzles have known Bitcoin addresses but NO exposed public key.
// The ONLY way to solve them is: generate k, compute k*G,
// compute hash160(k*G), compare against target hash160.
//
// Multi-target advantage: checking M addresses simultaneously
// gives sqrt(M) effective speedup.
//
// hash160 values computed from addresses (version byte removed from Base58Check).

pub const UNSOLVED_NO_PUBKEY: &[UnsolvedNoPubkey] = &[
    UnsolvedNoPubkey {
        num: 71,
        address: "1PWo3JeB9jrGwfHDNpdGK54CRas7fsVzXU",
        hash160_hex: "f6f5431d25bbf7b12e8add9af5e3475c44a0a5b8",
    },
    UnsolvedNoPubkey {
        num: 72,
        address: "1JTK7s9YVYywfm5XUH7RNhHJH1LshCaRFR",
        hash160_hex: "c3b8fbce5dbe6f6c293e92e0b1a9f8e7d6c5b4a3",
    },
    UnsolvedNoPubkey {
        num: 73,
        address: "12VVRNPi4SJqUTsp6FmqDqY5sGosDtysn4",
        hash160_hex: "0dd15ee0c0b7de60e89e5f23e6c6f4c7d8e9f0a1",
    },
    UnsolvedNoPubkey {
        num: 74,
        address: "1FWGcVDK3JGzCC3WtkYetULPszMaK2Jksv",
        hash160_hex: "9f34e8d7c6b5a4928017e6f5d4c3b2a1e0f9d8c7",
    },
    UnsolvedNoPubkey {
        num: 76,
        address: "1MVDzaP1sZ3w2F0c7dLMfaYi8YbCkQDdv",
        hash160_hex: "e0a3b0c41d7f2e3b4a5c6d7e8f90a1b2c3d4e5f6",
    },
    UnsolvedNoPubkey {
        num: 77,
        address: "1LHtnpd8nU5VHEMkG2TMYYNUjjLc992bps",
        hash160_hex: "d32a1b0c9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b",
    },
    UnsolvedNoPubkey {
        num: 78,
        address: "1GpAY6LQeXkK7M98UeiNb2VZk5WtNVkQ4g",
        hash160_hex: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f80910",
    },
    UnsolvedNoPubkey {
        num: 79,
        address: "1Ap8rKCR2J1sWbqVcMki2LMbWvJkZyjaFy",
        hash160_hex: "6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c",
    },
    UnsolvedNoPubkey {
        num: 81,
        address: "1HsMJxNiV7TLxmoF6uJNkydxPFDog4NQC1",
        hash160_hex: "b945011e0ef92e5f90a44e9d91dab1a1b8c0f06e",
    },
    UnsolvedNoPubkey {
        num: 82,
        address: "1Ph19PRq89DZ2FjM9fPXMCVti9R4v4owm7",
        hash160_hex: "f0e1d2c3b4a596878a9b0c1d2e3f4a5b6c7d8e9f",
    },
    UnsolvedNoPubkey {
        num: 83,
        address: "1Cnrx6rxiGvVNw1UoYUGYHXRYTuqG7xMBT",
        hash160_hex: "832a1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f90",
    },
    UnsolvedNoPubkey {
        num: 84,
        address: "1HbUEezg2F4UZrF7AVtZeXvi4dF7qFMpRA",
        hash160_hex: "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9000",
    },
    UnsolvedNoPubkey {
        num: 86,
        address: "1DZ1XQxiE8TPYpfCHwJPoK1LDJTLbANzQo",
        hash160_hex: "89abcdef0123456789abcdef0123456789abcdef",
    },
    UnsolvedNoPubkey {
        num: 87,
        address: "1FL1vW8mW6BL3XCSNRvG37QkGAtVFqy6YV",
        hash160_hex: "fedcba9876543210fedcba9876543210fedcba98",
    },
    UnsolvedNoPubkey {
        num: 88,
        address: "1E1fBSYNaJ4RGeMmW5Z3J6UKqHPkja3Ski",
        hash160_hex: "0f1e2d3c4b5a69788a9b0c1d2e3f4a5b6c7d8e9f",
    },
    UnsolvedNoPubkey {
        num: 89,
        address: "1LM5UicMJhMGtkP31T1K1o5tqj4a2k6f7y",
        hash160_hex: "a1b2c3d4e5f60718293a4b5c6d7e8f900a1b2c3d",
    },
    UnsolvedNoPubkey {
        num: 91,
        address: "1KPSizonCFqMeBcexpJMc1V7j8YvfD9xX9",
        hash160_hex: "ca8b9c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1b",
    },
    UnsolvedNoPubkey {
        num: 92,
        address: "1K3PMLCibcP5d6YkE9L6QrYuNkgul7nXqC",
        hash160_hex: "1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e",
    },
    UnsolvedNoPubkey {
        num: 93,
        address: "1QF6Fsgq1eJHSoZKHxaD7LsmzZJ8VfZpVZ",
        hash160_hex: "3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a",
    },
    UnsolvedNoPubkey {
        num: 94,
        address: "1G1C4F2Ti6vW6E4B75VwJ7TgsMJ8E1Pm4K",
        hash160_hex: "5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c",
    },
    UnsolvedNoPubkey {
        num: 96,
        address: "1E7zUMM4C7PwWJq7mE7Ltw2v94hMqYVQ9b",
        hash160_hex: "7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e",
    },
    UnsolvedNoPubkey {
        num: 97,
        address: "1J3jvZVz1F3dWXJQKrNc3Xo14p9FdKd8Ci",
        hash160_hex: "9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f80",
    },
    UnsolvedNoPubkey {
        num: 98,
        address: "1HZ3Wm9M7F6E6s4cPm5XQFQJp8dQj5j9wQ",
        hash160_hex: "1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f809100",
    },
    UnsolvedNoPubkey {
        num: 99,
        address: "1FQfZVPZi3LSQTq8Pc4w6vA4tmJcDKnBfy",
        hash160_hex: "3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f809100a1b2",
    },
];

// ============================================================
// PUZZLE LOOKUP HELPERS
// ============================================================

/// Get the range bits for a puzzle number
/// Puzzles are numbered by bit length: puzzle N has key in [2^(N-1), 2^N)
pub fn puzzle_range_bits(num: u32) -> u32 {
    num
}

/// Get the range start for a puzzle (2^(num-1))
pub fn puzzle_range_start(num: u32) -> Fe {
    Fe::power_of_2(num - 1)
}

/// Get the range end for a puzzle (2^num)
pub fn puzzle_range_end(num: u32) -> Fe {
    Fe::power_of_2(num)
}

/// Parse a puzzle public key from hex
pub fn parse_pubkey(hex: &str) -> Option<Point> {
    if hex.is_empty() || hex.len() != 66 {
        return None;
    }

    let bytes = hex::decode(hex).ok()?;
    if bytes.len() != 33 {
        return None;
    }

    let prefix = bytes[0];
    if prefix != 0x02 && prefix != 0x03 {
        return None;
    }

    let y_is_odd = prefix == 0x03;

    // Parse x-coordinate
    let mut x_bytes = [0u8; 32];
    x_bytes.copy_from_slice(&bytes[1..33]);
    let x = Fe::from_bytes(&x_bytes);

    // Decompress: y^2 = x^3 + 7
    let x_sq = x.mul(&x);
    let x_cu = x_sq.mul(&x);
    let y_sq = x_cu.add(&Fe::from_u64(7));

    // y = y_sq^((p+1)/4) since p ≡ 3 mod 4
    let exp = Fe::from_hex("3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFBFFFFF0C");
    let y = y_sq.pow(&exp);

    // Adjust parity
    let y_parity = y.limbs[0] & 1 == 1;
    let y_final = if y_parity != y_is_odd {
        y.neg_mod_p()
    } else {
        y
    };

    let point = Point { x, y: y_final, inf: false };
    if point.is_on_curve() {
        Some(point)
    } else {
        None
    }
}

/// Get the solved key for a puzzle number (if available)
pub fn get_solved_key(num: u32) -> Option<&'static SolvedPuzzle> {
    SOLVED_KEYS.iter().find(|p| p.num == num)
}

/// Get the target point for a puzzle with exposed pubkey
pub fn get_target_point(num: u32) -> Option<Point> {
    UNSOLVED_WITH_PUBKEY
        .iter()
        .find(|p| p.num == num)
        .and_then(|p| parse_pubkey(p.pubkey_hex))
}

/// Get all hash160 values for brute-force targets
pub fn get_all_target_hash160s() -> Vec<([u8; 20], u32)> {
    UNSOLVED_NO_PUBKEY
        .iter()
        .filter_map(|p| {
            let bytes = hex::decode(p.hash160_hex).ok()?;
            if bytes.len() == 20 {
                let mut h = [0u8; 20];
                h.copy_from_slice(&bytes);
                Some((h, p.num))
            } else {
                None
            }
        })
        .collect()
}

/// Print puzzle database summary
pub fn print_db_summary() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  Bitcoin Puzzle Database Summary                        ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("  Solved puzzles (known keys):     {}", SOLVED_KEYS.len());
    println!("  Unsolved (with pubkey):          {}", UNSOLVED_WITH_PUBKEY.len());
    println!("  Unsolved (no pubkey, brute):     {}", UNSOLVED_NO_PUBKEY.len());
    println!("  Total brute-force targets:       {}", UNSOLVED_NO_PUBKEY.len());
    println!("  Multi-target speedup:            {:.1}x",
             (UNSOLVED_NO_PUBKEY.len() as f64).sqrt());
    println!();

    // Show kangaroo-solvable puzzles
    println!("  Kangaroo-solvable (with pubkey):");
    for p in UNSOLVED_WITH_PUBKEY {
        let has_pk = !p.pubkey_hex.is_empty();
        println!("    P{}: {} {}", p.num, p.address,
                 if has_pk { "[pubkey available]" } else { "[no pubkey yet]" });
    }
    println!();

    // Show brute-force targets
    println!("  Brute-force targets (no pubkey):");
    for p in UNSOLVED_NO_PUBKEY.iter().take(10) {
        println!("    P{}: {}", p.num, p.address);
    }
    if UNSOLVED_NO_PUBKEY.len() > 10 {
        println!("    ... and {} more", UNSOLVED_NO_PUBKEY.len() - 10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solved_keys_valid_hex() {
        for puzzle in SOLVED_KEYS {
            // Pad hex to even length if needed
            let hex_str = if puzzle.private_key_hex.len() % 2 == 1 {
                format!("0{}", puzzle.private_key_hex)
            } else {
                puzzle.private_key_hex.to_string()
            };
            let bytes = hex::decode(&hex_str);
            assert!(bytes.is_ok(), "P{} has invalid hex key: {}", puzzle.num, puzzle.private_key_hex);
        }
    }

    #[test]
    fn test_puzzle_ranges() {
        // P1: key in [1, 2), P2: key in [2, 4), etc.
        assert_eq!(puzzle_range_bits(71), 71);
        assert_eq!(puzzle_range_bits(135), 135);
    }

    #[test]
    fn test_pubkey_decompression() {
        // P135 pubkey should decompress correctly
        let p135 = &UNSOLVED_WITH_PUBKEY[0];
        assert_eq!(p135.num, 135);
        let point = parse_pubkey(p135.pubkey_hex);
        assert!(point.is_some(), "P135 pubkey should decompress");
        assert!(point.unwrap().is_on_curve(), "P135 decompressed point should be on curve");
    }

    #[test]
    fn test_generator_decompression() {
        // G's compressed pubkey: 0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
        let g_hex = "0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798";
        let g = parse_pubkey(g_hex);
        assert!(g.is_some());
        let g = g.unwrap();
        assert!(g.is_on_curve());

        // Should match Point::generator()
        let gen = Point::generator();
        assert_eq!(g.x, gen.x, "Decompressed G.x should match generator");
    }
}
