//! RUSTSOLVER v12 — EXACT GLV Decomposition for secp256k1
//! ============================================================
//!
//! Implements the Gallant-Lambert-Vanstone (GLV) decomposition:
//!   k = k1 + k2 * lambda (mod N)
//! where |k1|, |k2| < 2^128.
//!
//! ALGORITHM:
//!   1. Compute the reduced basis of lattice L = { (x,y) : x + y*λ ≡ 0 (mod N) }
//!      using Lagrange's 2D lattice reduction (at initialization)
//!   2. Use Babai's nearest plane method with the reduced basis to decompose k
//!   3. Verify k1 + k2*λ ≡ k (mod N)
//!
//! The lattice reduction is a ONE-TIME computation done at init.
//! The per-collision decomposition uses the precomputed reduced basis.

use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{Signed, Zero};

// ============================================================
// secp256k1 CONSTANTS
// ============================================================

const N_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";
const LAMBDA_HEX: &str = "5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72";

// ============================================================
// GLV DECOMPOSITION RESULT
// ============================================================

/// Result of GLV decomposition: k = k1 + k2 * lambda (mod N)
#[derive(Clone, Debug)]
pub struct GLVDecomposition {
    pub k1: BigInt,
    pub k2: BigInt,
    pub verified: bool,
    pub k1_bits: u64,
    pub k2_bits: u64,
}

impl GLVDecomposition {
    /// Approximate log2 of Eisenstein norm: N(k1,k2) = k1² - k1*k2 + k2²
    pub fn eisenstein_norm_bits(&self) -> u64 {
        let k1_bits = self.k1.clone().abs().bits() as u64;
        let k2_bits = self.k2.clone().abs().bits() as u64;
        if k1_bits == 0 && k2_bits == 0 { return 0; }
        std::cmp::max(2 * std::cmp::max(k1_bits, k2_bits), k1_bits + k2_bits)
    }

    pub fn is_small(&self, max_bits: u32) -> bool {
        self.k1_bits <= max_bits as u64 && self.k2_bits <= max_bits as u64
    }
}

// ============================================================
// 2D LATTICE REDUCTION (Lagrange's algorithm)
// ============================================================

/// Reduce a 2D lattice basis using Lagrange's algorithm.
/// Input: two basis vectors (x1, y1), (x2, y2)
/// Output: reduced basis with short, nearly orthogonal vectors.
fn lagrange_reduce(v1: (BigInt, BigInt), v2: (BigInt, BigInt)) -> ((BigInt, BigInt), (BigInt, BigInt)) {
    let (mut v1x, mut v1y) = v1;
    let (mut v2x, mut v2y) = v2;

    for _iter in 0..1000 {
        // Compute norms
        let n1 = &v1x * &v1x + &v1y * &v1y;
        let n2 = &v2x * &v2x + &v2y * &v2y;

        // Ensure ||v1|| <= ||v2|| (swap if needed)
        if n2 < n1 {
            std::mem::swap(&mut v1x, &mut v2x);
            std::mem::swap(&mut v1y, &mut v2y);
        }

        // m = round(v1·v2 / v1·v1)
        let dot = &v1x * &v2x + &v1y * &v2y;
        let n1_clone = &v1x * &v1x + &v1y * &v1y;

        if n1_clone.is_zero() { break; }

        // Round division: m = round(dot / n1)
        let m = round_div(&dot, &n1_clone);

        if m.is_zero() { break; }

        // v2 = v2 - m * v1
        v2x -= &m * &v1x;
        v2y -= &m * &v1y;
    }

    ((v1x, v1y), (v2x, v2y))
}

/// Round division for BigInt: round(a / b)
fn round_div(a: &BigInt, b: &BigInt) -> BigInt {
    if b.is_zero() { return BigInt::from(0); }
    // Use truncated division
    let q = a / b;
    let r = a - &q * b;
    // If |2r| > |b|, round away from zero
    let abs_b = b.abs();
    let two_r: BigInt = &r << 1;  // r * 2
    if two_r.abs() > abs_b {
        if r.sign() == b.sign() {
            q + BigInt::from(1)
        } else {
            q - BigInt::from(1)
        }
    } else {
        q
    }
}

// ============================================================
// GLV DECOMPOSER
// ============================================================

pub struct GLVDecomposer {
    n: BigUint,
    lambda: BigUint,
    /// Reduced basis vector 1: (b1x, b1y) — satisfies b1x + b1y*λ ≡ 0 (mod N)
    b1: (BigInt, BigInt),
    /// Reduced basis vector 2: (b2x, b2y) — satisfies b2x + b2y*λ ≡ 0 (mod N)
    b2: (BigInt, BigInt),
    /// For Babai: the Gram-Schmidt orthogonal component of v2
    /// mu = <v1, v2> / <v1, v1>
    mu: BigInt,
    /// <v1, v1> for Babai
    v1_sq: BigInt,
}

impl GLVDecomposer {
    pub fn new() -> Self {
        let n_big = BigUint::parse_bytes(N_HEX.as_bytes(), 16).expect("Invalid N");
        let lambda_big = BigUint::parse_bytes(LAMBDA_HEX.as_bytes(), 16).expect("Invalid lambda");

        let n_int = BigInt::from(n_big.clone());
        let lambda_int = BigInt::from(lambda_big.clone());

        // Initial basis of L = { (x,y) : x + y*λ ≡ 0 (mod N) }
        // v1 = (N, 0):  N + 0*λ = N ≡ 0 (mod N) ✓
        // v2 = (N-λ, 1):  (N-λ) + 1*λ = N ≡ 0 (mod N) ✓
        let v1 = (n_int.clone(), BigInt::from(0));
        let v2 = (&n_int - &lambda_int, BigInt::from(1));

        println!("  [GLV] Computing reduced lattice basis...");
        println!("    Initial: v1=({}b, 0), v2=({}b, 1)",
                 v1.0.bits(), v2.0.bits());

        // Reduce the basis
        let ((b1x, b1y), (b2x, b2y)) = lagrange_reduce(v1, v2);

        println!("    Reduced: b1=({}b, {}b), b2=({}b, {}b)",
                 b1x.bits(), b1y.bits(), b2x.bits(), b2y.bits());

        // Compute Babai's algorithm constants
        let v1_sq = &b1x * &b1x + &b1y * &b1y;
        let dot = &b1x * &b2x + &b1y * &b2y;
        let mu = round_div(&dot, &v1_sq);

        println!("    v1·v1 = {}b, mu = {}b", v1_sq.bits(), mu.bits());

        GLVDecomposer {
            n: n_big,
            lambda: lambda_big,
            b1: (b1x, b1y),
            b2: (b2x, b2y),
            mu,
            v1_sq,
        }
    }

    /// Exact GLV decomposition: k = k1 + k2*lambda (mod N)
    ///
    /// Uses Babai's nearest plane algorithm with the precomputed reduced basis:
    ///   1. Find c2 = round( projection of (k,0) onto v2* / ||v2*||² )
    ///      where v2* = v2 - mu*v1 is the Gram-Schmidt component
    ///   2. Subtract c2*v2 from (k, 0)
    ///   3. Find c1 = round( projection of remainder onto v1 / ||v1||² )
    ///   4. Subtract c1*v1
    ///   5. What remains is (k1, k2)
    pub fn decompose(&self, k: &BigUint) -> GLVDecomposition {
        let k_int = BigInt::from(k.clone());

        // Babai's nearest plane algorithm
        // Target: t = (k, 0)
        // Gram-Schmidt: v1* = v1, v2* = v2 - mu*v1

        // Step 1: c2 = round(<t, v2*> / <v2*, v2*>)
        // <t, v2*> = <(k,0), (b2x - mu*b1x, b2y - mu*b1y)>
        //          = k * (b2x - mu*b1x)
        // <v2*, v2*> = |v2*|²

        // Simpler approach: use the "round-off" method directly
        // c1 = round(<t, v1> / <v1, v1>)
        // c2 = round(<t, v2*> / <v2*, v2*>)

        // <(k, 0), v1> = k * b1x
        // <(k, 0), v2*> = k * (b2x - mu*b1x)
        // <v2*, v2*> = <v2, v2*> = <v2, v2> - mu * <v1, v2>

        let v2s_x = &self.b2.0 - &self.mu * &self.b1.0;
        let v2s_y = &self.b2.1 - &self.mu * &self.b1.1;

        let v2s_sq = &v2s_x * &v2s_x + &v2s_y * &v2s_y;

        // c2 = round(k * v2s_x / v2s_sq)  — since <(k,0), (v2s_x, v2s_y)> = k*v2s_x
        let t_dot_v2s = &k_int * &v2s_x;
        let c2 = round_div(&t_dot_v2s, &v2s_sq);

        // Subtract c2*v2 from target
        let t1_x = &k_int - &c2 * &self.b2.0;
        let t1_y = BigInt::from(0) - &c2 * &self.b2.1;

        // c1 = round(<(t1_x, t1_y), v1> / <v1, v1>)
        let t1_dot_v1 = &t1_x * &self.b1.0 + &t1_y * &self.b1.1;
        let c1 = round_div(&t1_dot_v1, &self.v1_sq);

        // Compute (k1, k2) = (t1_x, t1_y) - c1*v1
        let k1 = t1_x - &c1 * &self.b1.0;
        let k2 = t1_y - &c1 * &self.b1.1;

        // Verify
        let verified = self.verify(&k1, &k2, k);

        let k1_bits = k1.clone().abs().bits() as u64;
        let k2_bits = k2.clone().abs().bits() as u64;

        GLVDecomposition { k1, k2, verified, k1_bits, k2_bits }
    }

    /// Verify: k1 + k2*lambda ≡ k (mod N)
    fn verify(&self, k1: &BigInt, k2: &BigInt, k_expected: &BigUint) -> bool {
        // Compute k1 + k2*lambda mod N using BigUint
        let n = &self.n;
        let k1_abs = k1.abs().to_biguint().unwrap();
        let k2_abs = k2.abs().to_biguint().unwrap();

        let k2_lambda = &k2_abs * &self.lambda % n;

        let k_computed = match (k1.sign(), k2.sign()) {
            (Sign::Plus | Sign::NoSign, Sign::Plus | Sign::NoSign) => {
                (&k1_abs + &k2_lambda) % n
            }
            (Sign::Plus | Sign::NoSign, Sign::Minus) => {
                if k1_abs >= k2_lambda {
                    (&k1_abs - &k2_lambda) % n
                } else {
                    (n - &k2_lambda + &k1_abs) % n
                }
            }
            (Sign::Minus, Sign::Plus | Sign::NoSign) => {
                if k2_lambda >= k1_abs {
                    (&k2_lambda - &k1_abs) % n
                } else {
                    (n - &k1_abs + &k2_lambda) % n
                }
            }
            (Sign::Minus, Sign::Minus) => {
                let sum = k1_abs + k2_lambda;
                if sum >= *n { sum - n } else { n - &sum }
            }
        };

        k_computed == *k_expected
    }

    /// ENS filter check: does k have a valid GLV decomposition?
    ///
    /// The GLV decomposition always gives |k1|, |k2| < 2^128 for any k < N.
    /// However, the verification step (k1 + k2*λ ≡ k mod N) catches bugs/errors.
    /// Additionally, for k in a specific range, we can check that the
    /// decomposition is consistent — the verified flag must be true and
    /// components must be within the 2^128 GLV bound.
    ///
    /// For puzzle-range keys, this primarily serves as a CORRECTNESS check
    /// rather than a filter. The actual range filtering is done separately.
    pub fn ens_check(&self, k: &BigUint, range_bits: u32) -> bool {
        let decomp = self.decompose(k);
        if !decomp.verified { return false; }
        // Standard GLV bound: |k1|, |k2| < 2^128
        // For puzzle-range keys, we expect even smaller (≈2^125)
        // Use range_bits/2 + margin as an additional sanity check
        // but with a floor of 128 (the theoretical GLV bound)
        let glv_bound = 128u32;
        let range_bound = range_bits / 2 + 8;
        let max_bits = std::cmp::max(glv_bound, range_bound);
        decomp.is_small(max_bits)
    }

    /// Selftest
    pub fn selftest() -> bool {
        println!("  [GLV] Running exact decomposition selftest...");

        let decomposer = GLVDecomposer::new();
        let mut all_pass = true;

        // Test 1: k = 1
        let k = BigUint::from(1u64);
        let d = decomposer.decompose(&k);
        println!("    k=1: k1={} ({}b), k2={} ({}b), verified={}",
                 d.k1, d.k1_bits, d.k2, d.k2_bits, d.verified);
        if !d.verified { all_pass = false; }

        // Test 2: k = lambda (should give k1≈0, k2≈1)
        let k = decomposer.lambda.clone();
        let d = decomposer.decompose(&k);
        println!("    k=lambda: k1_bits={}, k2_bits={}, verified={}",
                 d.k1_bits, d.k2_bits, d.verified);
        if !d.verified { all_pass = false; }
        if d.k2_bits > 5 {
            println!("    [WARN] k=lambda: |k2| should be ~1 but is 2^{}", d.k2_bits);
        }

        // Test 3: k = N-1
        let k = &decomposer.n - BigUint::from(1u64);
        let d = decomposer.decompose(&k);
        println!("    k=N-1: k1_bits={}, k2_bits={}, verified={}",
                 d.k1_bits, d.k2_bits, d.verified);
        if !d.verified { all_pass = false; }

        // Test 4: Random k values in puzzle range [2^134, 2^135)
        println!("    Testing random keys in [2^134, 2^135)...");
        let mut rng_seed = 0x123456789ABCDEF0u64;
        let mut next_rand = || -> u64 {
            rng_seed = rng_seed.wrapping_mul(6364136223846793005)
                              .wrapping_add(1442695040888963407);
            rng_seed
        };

        let range_start = BigUint::from(1u64) << 134;
        let mut ens_passes = 0u32;
        let mut ens_fails = 0u32;

        for i in 0..20 {
            let offset = BigUint::from(next_rand() % 1_000_000);
            let k = &range_start + &offset;
            let d = decomposer.decompose(&k);

            if !d.verified {
                println!("    [FAIL] Random key #{}: verification failed!", i);
                all_pass = false;
            }

            if i < 3 {
                println!("    k=2^134+{}: k1_bits={}, k2_bits={}, norm={}b, verified={}",
                         offset, d.k1_bits, d.k2_bits, d.eisenstein_norm_bits(), d.verified);
            }

            if decomposer.ens_check(&k, 135) {
                ens_passes += 1;
            } else {
                ens_fails += 1;
            }
        }

        println!("    ENS on puzzle-range keys: {}/20 pass", ens_passes);

        // Test 5: ENS rejection of random out-of-range keys
        println!("    Testing ENS rejection of random keys...");
        let mut rejections = 0u32;
        let mut tested = 0u32;
        for _ in 0..200 {
            let r = BigUint::from(next_rand()) * &BigUint::from(next_rand())
                  * &BigUint::from(next_rand()) * &BigUint::from(next_rand());
            let k = r % &decomposer.n;
            let range_end = BigUint::from(1u64) << 135;
            if k >= range_start && k < range_end { continue; }
            tested += 1;
            if !decomposer.ens_check(&k, 135) {
                rejections += 1;
            }
        }
        if tested > 0 {
            println!("    ENS rejection rate for random k: {}/{} = {:.1}%",
                     rejections, tested, (rejections as f64 / tested as f64) * 100.0);
        }

        if all_pass {
            println!("  [GLV] Selftest PASSED");
        } else {
            println!("  [GLV] Selftest FAILED");
        }

        all_pass
    }
}

// ============================================================
// 2D LATTICE KANGAROO
// ============================================================

pub struct LatticeKangaroo {
    pub g_lambda: crate::point::Point,
    decomposer: GLVDecomposer,
}

impl LatticeKangaroo {
    pub fn new() -> Self {
        use crate::field::Fe;
        use crate::point::Point;

        let g = Point::generator();
        let lambda_fe = Fe { limbs: crate::field::LAMBDA };
        let g_lambda = g.scalar_mul(&lambda_fe);

        LatticeKangaroo {
            g_lambda,
            decomposer: GLVDecomposer::new(),
        }
    }

    pub fn decomposer(&self) -> &GLVDecomposer {
        &self.decomposer
    }

    /// Compute k*G using GLV decomposition for ~2x faster scalar mul
    pub fn glv_scalar_mul(&self, k: &BigUint) -> crate::point::Point {
        use crate::field::Fe;
        use crate::point::Point;

        let decomp = self.decomposer.decompose(k);
        let g = Point::generator();

        let k1_abs = decomp.k1.abs().to_biguint().unwrap();
        let k2_abs = decomp.k2.abs().to_biguint().unwrap();

        let k1_fe = Fe::from_biguint_mod_n(&k1_abs);
        let k2_fe = Fe::from_biguint_mod_n(&k2_abs);

        let p1 = g.scalar_mul(&k1_fe);
        let p2 = self.g_lambda.scalar_mul(&k2_fe);

        let p1_signed = if decomp.k1.sign() == Sign::Minus { p1.neg() } else { p1 };
        let p2_signed = if decomp.k2.sign() == Sign::Minus { p2.neg() } else { p2 };

        p1_signed.add(&p2_signed)
    }
}
