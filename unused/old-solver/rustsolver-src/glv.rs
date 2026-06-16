//! PRISM VORTEX v12 — Layer 1: Exact GLV Decomposition for secp256k1
//! ================================================================
//!
//! Implements the Gallant-Lambert-Vanstone (GLV) decomposition:
//!   k = k1 + k2 * lambda (mod N)
//! where |k1|, |k2| ~ sqrt(N) ≈ 2^128
//!
//! Uses BigUint for all arithmetic (no BigInt dependency issues).

use num_bigint::BigUint;
use num_traits::{Zero, One};
use crate::field::Fe;
use crate::point::Point;

/// secp256k1 group order N
pub fn secp256k1_order() -> BigUint {
    BigUint::parse_bytes(
        b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141", 16
    ).unwrap()
}

/// Lambda: cube root of unity mod N (lambda^3 = 1 mod N)
pub fn secp256k1_lambda() -> BigUint {
    BigUint::parse_bytes(
        b"5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72", 16
    ).unwrap()
}

// Z[omega] factorization constants
const PI_A_HEX: &str = "114CA50F7A8E2F3F657C1108D9D44CFD8";
const PI_B_HEX: &str = "3086D221A7D46BCDE86C90E49284EB15";

/// GLV decomposition result (using BigUint with sign flag)
#[derive(Debug, Clone)]
pub struct GLVDecomposition {
    /// k1 absolute value
    pub k1: BigUint,
    /// k1 is negative
    pub k1_neg: bool,
    /// k2 absolute value
    pub k2: BigUint,
    /// k2 is negative
    pub k2_neg: bool,
    /// Whether the decomposition was verified
    pub verified: bool,
}

/// Exact GLV decomposition for secp256k1 using the lattice basis method.
///
/// Given k in [0, N), computes (k1, k2) such that:
///   k ≡ k1 + k2 * lambda (mod N)
///   |k1| < ~2^128, |k2| < ~2^128
///
/// Algorithm: Uses the short lattice basis from the Z[omega] factorization
/// to perform Babai's nearest plane rounding.
pub fn glv_decompose(k: &BigUint) -> GLVDecomposition {
    let n = secp256k1_order();
    let lam = secp256k1_lambda();
    let pi_a = BigUint::parse_bytes(PI_A_HEX.as_bytes(), 16).unwrap();
    let pi_b = BigUint::parse_bytes(PI_B_HEX.as_bytes(), 16).unwrap();

    // Method: Compute k2 = round(k * lambda_inv / N), then k1 = k - k2 * lambda mod N
    // This gives a balanced decomposition where |k1|, |k2| < ~sqrt(N)
    //
    // lambda_inv = lambda^(N-2) mod N (Fermat)
    // But computing this exactly is slow with BigUint.
    //
    // Instead, use the direct lattice-based method:
    // v1 = (PI_A, PI_B) is a short vector of L = {(a,b) : a + b*lambda ≡ 0 mod N}
    // (verified: PI_A + PI_B * lambda ≡ 0 mod N in Z[omega])
    //
    // Babai rounding:
    // c1 = round(k * PI_B / N) -- approximately
    // k1 = k - c1 * PI_A (mod N, centered)
    // k2 = -c1 * PI_B (mod N, centered)

    // Compute c1 using simple division:
    // c1 ≈ k * PI_B / N
    // We use: c1 = (k * PI_B + N/2) / N for rounding
    let k_pi_b = k * &pi_b;
    let half_n = &n >> 1;
    let c1 = (&k_pi_b + &half_n) / &n;

    // k1 = k - c1 * PI_A (mod N)
    let c1_pi_a = &c1 * &pi_a;
    let k1_raw = if c1_pi_a <= *k {
        k - &c1_pi_a
    } else {
        &n - (&c1_pi_a - k)
    };

    // k2 = -c1 * PI_B (mod N) = N - (c1 * PI_B mod N) if positive
    let c1_pi_b_mod_n: BigUint = (&c1 * &pi_b) % &n;
    let k2_raw = if c1_pi_b_mod_n.is_zero() {
        BigUint::zero()
    } else {
        &n - &c1_pi_b_mod_n
    };

    // Center the results: if > N/2, it's negative
    let (k1, k1_neg) = if k1_raw > half_n {
        (&n - &k1_raw, true)
    } else {
        (k1_raw, false)
    };

    let (k2, k2_neg) = if k2_raw > half_n {
        (&n - &k2_raw, true)
    } else {
        (k2_raw, false)
    };

    // Verify: k1 + k2 * lambda ≡ k (mod N)
    let k1_mod = if k1_neg { &n - &k1 } else { k1.clone() };
    let k2_mod = if k2_neg { &n - &k2 } else { k2.clone() };
    let k2_lambda = (&k2_mod * &lam) % &n;
    let reconstructed = (&k1_mod + &k2_lambda) % &n;
    let verified = reconstructed == k % &n;

    GLVDecomposition { k1, k1_neg, k2, k2_neg, verified }
}

/// Compute the 6 GLV-related scalars from k:
/// k, -k, lambda*k, -lambda*k, lambda^2*k, -lambda^2*k (all mod N)
pub fn glv_six_scalars(k: &BigUint) -> [BigUint; 6] {
    let n = secp256k1_order();
    let lam = secp256k1_lambda();
    let lam_sq = (&lam * &lam) % &n;

    let k_n = k % &n;
    let neg_k = &n - &k_n;
    let lam_k = (&lam * &k_n) % &n;
    let neg_lam_k = &n - &lam_k;
    let lam2_k = (&lam_sq * &k_n) % &n;
    let neg_lam2_k = &n - &lam2_k;

    [k_n, neg_k, lam_k, neg_lam_k, lam2_k, neg_lam2_k]
}

/// GLV multi-scalar multiplication: compute k1*G + k2*phi(G)
/// Uses interleaved double-and-add (Shamir's trick variant)
pub fn glv_double_mul(k1: &BigUint, k1_neg: bool, k2: &BigUint, k2_neg: bool, g: &Point, phi_g: &Point) -> Point {
    let neg_g = g.neg();
    let neg_phi_g = phi_g.neg();

    let max_bits = std::cmp::max(k1.bits(), k2.bits()) as usize;
    if max_bits == 0 { return Point::infinity(); }

    let mut result = crate::point::JacobianPoint::infinity();

    for i in (0..max_bits).rev() {
        result = result.double();

        let k1_bit_set = i < k1.bits() as usize && k1.bit(i as u64);
        let k2_bit_set = i < k2.bits() as usize && k2.bit(i as u64);

        if k1_bit_set && k2_bit_set {
            let p1 = if k1_neg { &neg_g } else { g };
            let p2 = if k2_neg { &neg_phi_g } else { phi_g };
            result = result.add_affine(p1).add_affine(p2);
        } else if k1_bit_set {
            let p1 = if k1_neg { &neg_g } else { g };
            result = result.add_affine(p1);
        } else if k2_bit_set {
            let p2 = if k2_neg { &neg_phi_g } else { phi_g };
            result = result.add_affine(p2);
        }
    }

    result.to_affine()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glv_decomposition_small() {
        let k = BigUint::from(42u64);
        let decomp = glv_decompose(&k);
        assert!(decomp.verified, "GLV decomposition must be verified");

        let n = secp256k1_order();
        let lam = secp256k1_lambda();
        let k1_mod = if decomp.k1_neg { &n - &decomp.k1 } else { decomp.k1.clone() };
        let k2_mod = if decomp.k2_neg { &n - &decomp.k2 } else { decomp.k2.clone() };
        let k2_lambda = (&k2_mod * &lam) % &n;
        let reconstructed = (&k1_mod + &k2_lambda) % &n;
        assert_eq!(reconstructed, k % n);
    }

    #[test]
    fn test_glv_six_scalars() {
        let k = BigUint::from(12345u64);
        let scalars = glv_six_scalars(&k);
        assert_eq!(scalars.len(), 6);

        let n = secp256k1_order();
        let lam = secp256k1_lambda();
        assert_eq!(scalars[2], (&lam * &k) % &n);
    }
}
