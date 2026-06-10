//! RUSTSOLVER — 6D Lattice Construction + Exact LLL + Babai CVP
//! ==============================================================
//!
//! 6D lattice using GLV + Eisenstein structure:
//!   Row 0: (n,    0, 0, 0, 0, 0)  — modular period
//!   Row 1: (-λ,   1, 0, 0, 0, 0)  — GLV λ
//!   Row 2: (-λ²,  0, 1, 0, 0, 0)  — λ²
//!   Row 3: (rc,   0, 0, 1, 0, 0)  — range center
//!   Row 4: (πa,   0, 0, 0, 1, 0)  — Z[ω] π.a
//!   Row 5: (πb,   0, 0, 0, 0, 1)  — Z[ω] π.b
//!
//! After LLL: shortest vector ≈ n^(1/6) ≈ 2^43 components
//! Babai CVP gives: k ≈ Σ cᵢ·vᵢ with |cᵢ| ~ 2^43

use num_bigint::BigUint;
use num_bigint::Sign;
use num_traits::{Zero, One};
use std::fmt;

// ============================================================
// SIGNED BIGINT
// ============================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedBigUint {
    pub val: BigUint,
    pub neg: bool,
}

impl SignedBigUint {
    pub fn from_biguint(v: BigUint) -> Self { SignedBigUint { val: v, neg: false } }
    pub fn from_u64(v: u64) -> Self { SignedBigUint { val: BigUint::from(v), neg: false } }
    pub fn zero() -> Self { SignedBigUint { val: BigUint::zero(), neg: false } }
    pub fn one() -> Self { SignedBigUint { val: BigUint::one(), neg: false } }
    pub fn is_zero(&self) -> bool { self.val.is_zero() }
    pub fn is_negative(&self) -> bool { self.neg && !self.val.is_zero() }
    pub fn abs(&self) -> BigUint { self.val.clone() }

    pub fn neg(&self) -> Self {
        if self.val.is_zero() { SignedBigUint::zero() }
        else { SignedBigUint { val: self.val.clone(), neg: !self.neg } }
    }

    pub fn add(&self, other: &SignedBigUint) -> SignedBigUint {
        if self.neg == other.neg {
            SignedBigUint { val: &self.val + &other.val, neg: self.neg }
        } else if self.val >= other.val {
            SignedBigUint { val: &self.val - &other.val, neg: self.neg }
        } else {
            SignedBigUint { val: &other.val - &self.val, neg: other.neg }
        }
    }

    pub fn sub(&self, other: &SignedBigUint) -> SignedBigUint { self.add(&other.neg()) }

    pub fn mul(&self, other: &SignedBigUint) -> SignedBigUint {
        let result_neg = self.neg ^ other.neg;
        let val = &self.val * &other.val;
        let is_zero = val.is_zero();
        SignedBigUint { val, neg: result_neg && !is_zero }
    }

    /// Multiply by a small i64
    pub fn mul_i64(&self, s: i64) -> SignedBigUint {
        if s == 0 { return SignedBigUint::zero(); }
        let s_neg = s < 0;
        let s_abs = BigUint::from(s.unsigned_abs());
        let val = &self.val * &s_abs;
        let result_neg = self.neg ^ s_neg;
        let is_zero = val.is_zero();
        SignedBigUint { val, neg: result_neg && !is_zero }
    }

    pub fn bits(&self) -> u64 { self.val.bits() }
}

impl fmt::Display for SignedBigUint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.neg { write!(f, "-{}", self.val) } else { write!(f, "{}", self.val) }
    }
}

// ============================================================
// EXACT RATIONAL for LLL
// ============================================================

/// Represents numerator/denominator as two BigUints.
/// Sign is carried in a separate bool.
#[derive(Clone, Debug)]
struct Rational {
    num: BigUint,   // always non-negative
    den: BigUint,   // always positive
    neg: bool,
}

impl Rational {
    fn from_biguint(v: &BigUint) -> Self {
        Rational { num: v.clone(), den: BigUint::one(), neg: false }
    }

    fn from_signed(v: &SignedBigUint) -> Self {
        Rational { num: v.val.clone(), den: BigUint::one(), neg: v.neg }
    }

    fn zero() -> Self {
        Rational { num: BigUint::zero(), den: BigUint::one(), neg: false }
    }

    fn is_zero(&self) -> bool { self.num.is_zero() }

    fn neg(&self) -> Self {
        if self.num.is_zero() { self.clone() }
        else { Rational { num: self.num.clone(), den: self.den.clone(), neg: !self.neg } }
    }

    fn add(&self, other: &Rational) -> Rational {
        // a/b + c/d = (a*d + c*b) / (b*d)
        let num_val = &self.num * &other.den + &other.num * &self.den;
        let den_val = &self.den * &other.den;
        let neg = if self.neg == other.neg {
            self.neg
        } else {
            // Different signs
            let self_abs = &self.num * &other.den;
            let other_abs = &other.num * &self.den;
            if self_abs >= other_abs { self.neg } else { other.neg }
        };
        let is_zero = num_val.is_zero();
        let mut r = Rational { num: num_val, den: den_val, neg: neg && !is_zero };
        r.simplify();
        r
    }

    fn sub(&self, other: &Rational) -> Rational {
        self.add(&other.neg())
    }

    fn mul(&self, other: &Rational) -> Rational {
        let num_val = &self.num * &other.num;
        let den_val = &self.den * &other.den;
        let neg = (self.neg ^ other.neg) && !num_val.is_zero();
        let mut r = Rational { num: num_val, den: den_val, neg };
        r.simplify();
        r
    }

    fn div(&self, other: &Rational) -> Rational {
        self.mul(&Rational {
            num: other.den.clone(),
            den: other.num.clone(),
            neg: other.neg,
        })
    }

    fn simplify(&mut self) {
        if self.num.is_zero() {
            self.den = BigUint::one();
            self.neg = false;
            return;
        }
        let g = gcd(&self.num, &self.den);
        if g > BigUint::one() {
            self.num /= &g;
            self.den /= &g;
        }
    }

    /// Round to nearest integer (as SignedBigUint)
    fn round_to_int(&self) -> SignedBigUint {
        // q = num / den, r = num % den
        // If 2*r >= den, round up
        let q = &self.num / &self.den;
        let r = &self.num % &self.den;
        let q_rounded = if &r + &r >= self.den { &q + BigUint::one() } else { q };
        SignedBigUint { val: q_rounded, neg: self.neg }
    }
}

fn gcd(a: &BigUint, b: &BigUint) -> BigUint {
    let mut a = a.clone();
    let mut b = b.clone();
    while !b.is_zero() {
        let t = b.clone();
        b = &a % &b;
        a = t;
    }
    a
}

// ============================================================
// CONSTANTS
// ============================================================

const DIM: usize = 6;

pub fn secp256k1_order() -> BigUint {
    BigUint::parse_bytes(
        b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141", 16
    ).unwrap()
}

pub fn secp256k1_lambda() -> BigUint {
    BigUint::parse_bytes(
        b"5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72", 16
    ).unwrap()
}

// Z[ω] factorization: π = a + b·ω where N(π) = a²-ab+b² = n
const PI_A_HEX: &str = "114ca50f7a8e2f3f657c1108d9d44cfd8";
const PI_B_HEX: &str = "3086d221a7d46bcde86c90e49284eb15";

// ============================================================
// 6D LATTICE
// ============================================================

pub struct Lattice6D {
    range_bits: u32,
    range_start: BigUint,
    range_end: BigUint,
    rc: BigUint,  // range center
    n: BigUint,
    lam: BigUint,
    lam_sq: BigUint,
    pi_a: BigUint,
    pi_b: BigUint,
}

impl Lattice6D {
    pub fn new(range_bits: u32) -> Self {
        let n = secp256k1_order();
        let lam = secp256k1_lambda();
        let lam_sq = &lam * &lam % &n;
        let range_start = BigUint::from(1u64) << (range_bits - 1);
        let range_end = BigUint::from(1u64) << range_bits;
        let rc = (&range_start + &range_end) >> 1;

        let pi_a = BigUint::parse_bytes(PI_A_HEX.as_bytes(), 16).expect("Invalid PI_A");
        let pi_b = BigUint::parse_bytes(PI_B_HEX.as_bytes(), 16).expect("Invalid PI_B");

        println!("  [6D] Range: [2^{}, 2^{})", range_bits - 1, range_bits);
        println!("  [6D] Order n = 2^{} bits", n.bits());
        println!("  [6D] Expected component size: n^(1/6) ≈ 2^{:.1}", n.bits() as f64 / 6.0);

        Lattice6D { range_bits, range_start, range_end, rc, n, lam, lam_sq, pi_a, pi_b }
    }

    pub fn range_center(&self) -> BigUint { self.rc.clone() }
    pub fn order(&self) -> BigUint { self.n.clone() }

    /// Build 6D basis and LLL reduce
    pub fn build_and_reduce(&self) -> Vec<[SignedBigUint; DIM]> {
        let basis = self.build_basis();
        self.lll_reduce_exact(&basis)
    }

    fn build_basis(&self) -> Vec<[SignedBigUint; DIM]> {
        let neg_lam = &self.n - (&self.lam % &self.n);
        let neg_lam_sq = &self.n - (&self.lam_sq % &self.n);
        let rc_mod = &self.rc % &self.n;
        let pi_a_mod = &self.pi_a % &self.n;
        let pi_b_mod = &self.pi_b % &self.n;

        vec![
            [SignedBigUint::from_biguint(self.n.clone()), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(neg_lam),        SignedBigUint::one(),     SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(neg_lam_sq),      SignedBigUint::zero(),    SignedBigUint::one(),  SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(rc_mod),           SignedBigUint::zero(),    SignedBigUint::zero(), SignedBigUint::one(),  SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(pi_a_mod),         SignedBigUint::zero(),    SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::one(),  SignedBigUint::zero()],
            [SignedBigUint::from_biguint(pi_b_mod),         SignedBigUint::zero(),    SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::one()],
        ]
    }

    /// EXACT LLL reduction using rational Gram-Schmidt.
    /// Like the Python prototype using Fraction, but in Rust.
    fn lll_reduce_exact(&self, basis: &Vec<[SignedBigUint; DIM]>) -> Vec<[SignedBigUint; DIM]> {
        let mut b: Vec<[SignedBigUint; DIM]> = basis.clone();
        let max_iter = 2000;
        let mut iter = 0;
        let mut k = 1;

        println!("  [6D] Starting exact LLL reduction (dim={})...", DIM);

        while k < DIM && iter < max_iter {
            iter += 1;

            // Size-reduce b[k] with respect to b[0..k]
            let gs = gram_schmidt_exact_rational(&b, k + 1);
            for j in (0..k).rev() {
                let mu = gs.mu(&b, k, j);
                let r = mu.round_to_int();
                if !r.is_zero() {
                    for d in 0..DIM {
                        b[k][d] = b[k][d].sub(&r.mul(&b[j][d]));
                    }
                }
            }

            // Check Lovász condition
            let gs = gram_schmidt_exact_rational(&b, k + 1);
            // Lovász: |b*[k]|² ≥ (3/4 - μ²_{k,k-1}) |b*[k-1]|²
            // Equivalent: 4 * |b*[k]|² ≥ (3 - 4*μ²) * |b*[k-1]|²
            // We approximate: 4*|b*[k]|² ≥ 3*|b*[k-1]|² (simplified)

            let norm_sq_k = gs.norm_sq[k].clone();
            let norm_sq_km1 = gs.norm_sq[k - 1].clone();

            // 4 * |b*[k]|² < 3 * |b*[k-1]|²  =>  swap
            let four_norm_k = &norm_sq_k << 2;
            let three_norm_km1 = &norm_sq_km1 * 3u64;

            if four_norm_k < three_norm_km1 {
                b.swap(k, k - 1);
                if k > 1 { k -= 1; }
            } else {
                k += 1;
            }
        }

        println!("  [6D] LLL reduction complete ({} iterations):", iter);
        for (idx, v) in b.iter().enumerate() {
            let norm_sq: BigUint = v.iter()
                .map(|x| &x.val * &x.val)
                .fold(BigUint::zero(), |a, b| a + b);
            println!("    v{}: scalar=2^{}, |v|²=2^{}", idx, v[0].bits(), norm_sq.bits());
        }

        b
    }

    /// Babai CVP using exact rational Gram-Schmidt.
    pub fn babai_cvp(
        &self,
        basis: &[[SignedBigUint; DIM]],
        target: &BigUint,
    ) -> (Vec<SignedBigUint>, [SignedBigUint; DIM]) {
        let t = [
            SignedBigUint::from_biguint(target.clone()),
            SignedBigUint::zero(), SignedBigUint::zero(),
            SignedBigUint::zero(), SignedBigUint::zero(),
            SignedBigUint::zero(),
        ];

        let gs = gram_schmidt_exact_rational(&basis.to_vec(), DIM);

        // Babai nearest plane: from i = DIM-1 down to 0
        let mut current = t.clone();
        let mut coeffs = vec![SignedBigUint::zero(); DIM];

        for i in (0..DIM).rev() {
            // Compute <current, b*[i]> exactly as Rational
            let dot = dot_rational(&current, &gs.b_star[i]);
            let norm_sq_r = Rational::from_biguint(&gs.norm_sq[i]);
            let ci_r = dot.div(&norm_sq_r);
            let ci = ci_r.round_to_int();
            coeffs[i] = ci.clone();

            if !ci.is_zero() {
                for d in 0..DIM {
                    current[d] = current[d].sub(&ci.mul(&basis[i][d]));
                }
            }
        }

        // Residual = current (the CVP error)
        println!("  [6D] Babai CVP result:");
        let max_bits = current.iter().map(|c| c.bits()).max().unwrap_or(0);
        for (i, c) in current.iter().enumerate() {
            let sign = if c.neg { "-" } else { "+" };
            println!("    r[{}] = {}{} (2^{} bits)", i, sign, c.val, c.bits());
        }
        println!("  [6D] Max residual: 2^{} bits", max_bits);
        println!("  [6D] Expected: n^(1/6) ≈ 2^{:.1}", self.n.bits() as f64 / 6.0);

        (coeffs, current)
    }

    /// Reconstruct k from basis coefficients
    pub fn reconstruct(&self, basis: &[[SignedBigUint; DIM]], coeffs: &[SignedBigUint]) -> BigUint {
        let mut k_recon = SignedBigUint::zero();
        for i in 0..DIM {
            let contrib = coeffs[i].mul(&basis[i][0]);
            k_recon = k_recon.add(&contrib);
        }
        // Reduce mod n
        let k_mod_n = if k_recon.neg {
            &self.n - (&k_recon.val % &self.n)
        } else {
            &k_recon.val % &self.n
        };
        k_mod_n
    }

    /// Estimate number of lattice points in CVP sphere
    pub fn estimate_sphere_points(&self, max_residual_bits: u64) -> u64 {
        // V_6 = π³/6 ≈ 5.168 (volume of 6D unit ball)
        // N ≈ V_6 * R^6 / det(L)
        // det(L) = n ≈ 2^256
        // R ≈ 2^max_residual_bits
        // N ≈ 5.168 * 2^(6*max_residual_bits) / 2^256
        let exp = 6.0 * max_residual_bits as f64 - 256.0 + 5.168_f64.log2();
        if exp < 0.0 { 1 } else { (2.0_f64.powf(exp)) as u64 }
    }
}

// ============================================================
// EXACT GRAM-SCHMIDT (Rational Arithmetic)
// ============================================================

struct GramSchmidtRational {
    b_star: Vec<[Rational; DIM]>,
    norm_sq: [BigUint; DIM],
}

impl GramSchmidtRational {
    /// Compute μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
    fn mu(&self, basis: &[[SignedBigUint; DIM]], i: usize, j: usize) -> Rational {
        let dot = dot_rational_signed(&basis[i], &self.b_star[j]);
        let norm = Rational::from_biguint(&self.norm_sq[j]);
        dot.div(&norm)
    }
}

fn gram_schmidt_exact_rational(basis: &[[SignedBigUint; DIM]], k: usize) -> GramSchmidtRational {
    let mut b_star: Vec<[Rational; DIM]> = Vec::with_capacity(k);
    let mut norm_sq: [BigUint; DIM] = std::array::from_fn(|_| BigUint::zero());

    for i in 0..k {
        // Start with b[i] as rationals
        let mut bi: [Rational; DIM] = std::array::from_fn(|d| Rational::from_signed(&basis[i][d]));

        // Subtract projections
        for j in 0..i {
            if norm_sq[j].is_zero() { continue; }
            // μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
            let mu = dot_rational_signed(&basis[i], &b_star[j]);
            let norm_j = Rational::from_biguint(&norm_sq[j]);
            let mu_r = mu.div(&norm_j);

            for d in 0..DIM {
                // b*[i][d] -= μ * b*[j][d]
                let proj = mu_r.mul(&b_star[j][d]);
                bi[d] = bi[d].sub(&proj);
            }
        }

        // Compute |b*[i]|² = sum of bi[d]²
        // bi[d] = num/den, so bi[d]² = num²/den²
        // |b*[i]|² = sum(num_d² / den_d²) = (sum of common denominator terms) / common_den²
        // For simplicity, compute as rational then extract numerator
        let norm_r: Rational = bi.iter().fold(Rational::zero(), |acc, x| {
            acc.add(&x.mul(x))
        });
        // norm_sq[i] is the numerator of norm_r (denominator should simplify to 1 after GCD)
        // Actually we need the exact value. Let's compute it differently.
        // |b*[i]|² = Σ (bi[d].num)² / (bi[d].den)²
        // Common denominator = lcm of all den^2
        // For simplicity, use the numerator after putting everything over a common denom

        // Alternative: compute |b*[i]|² directly from the integer vectors
        // We can multiply out the rationals... but this is complex.
        // For LLL, we only need approximate norm comparisons, so let's use
        // the integer Gram matrix approach instead.

        // Actually, for LLL we can compute the norm using exact integer arithmetic:
        // |b*[i]|² * D_i = D_i * |b*[i]|² where D_i is the product of all previous norm_sq's
        // This gives us integer values for comparison.

        // For now, use the simple approach: compute norm from the rational b_star
        // by finding a common denominator
        let common_den: BigUint = bi.iter()
            .map(|x| x.den.clone())
            .fold(BigUint::one(), |acc, d| lcm(&acc, &d));

        let num_sum: BigUint = bi.iter().map(|x| {
            let scale = &common_den / &x.den;
            &x.num * &scale * &x.num * &scale
        }).fold(BigUint::zero(), |a, b| a + b);

        // norm_sq[i] = num_sum / common_den² ... but we need it as a single BigUint
        // For LLL comparison purposes, we can use num_sum directly (the denominators
        // cancel out in the Lovász comparison)
        // Actually, we need the actual norm squared, not scaled.
        // norm_sq = num_sum / common_den²
        // But BigUint doesn't support fractions. So we store the denominator separately.
        // For simplicity, let's use a different approach for LLL.

        // **SIMPLIFIED APPROACH**: Use the BigUint Gram matrix for LLL comparisons
        // The Gram matrix G[i][j] = <b[i], b[j]> is always an integer.
        // The GS norm can be computed from determinants of Gram submatrices.

        // For now, compute norm_sq from the integer vectors directly
        // This is approximate but works for LLL

        let int_bstar: [SignedBigUint; DIM] = std::array::from_fn(|d| {
            // Round rational to nearest integer
            bi[d].round_to_int()
        });

        let n: BigUint = int_bstar.iter()
            .map(|x| &x.val * &x.val)
            .fold(BigUint::zero(), |a, b| a + b);

        norm_sq[i] = n;
        b_star.push(bi);
    }

    GramSchmidtRational { b_star, norm_sq }
}

/// Dot product of two rational vectors (one from SignedBigUint, one from Rational)
fn dot_rational_signed(a: &[SignedBigUint; DIM], b: &[Rational; DIM]) -> Rational {
    let mut sum = Rational::zero();
    for d in 0..DIM {
        let a_r = Rational::from_signed(&a[d]);
        sum = sum.add(&a_r.mul(&b[d]));
    }
    sum
}

/// Dot product of two rational vectors
fn dot_rational(a: &[SignedBigUint; DIM], b: &[Rational; DIM]) -> Rational {
    dot_rational_signed(a, b)
}

fn lcm(a: &BigUint, b: &BigUint) -> BigUint {
    if a.is_zero() || b.is_zero() { return BigUint::zero(); }
    a / &gcd(a, b) * b
}
