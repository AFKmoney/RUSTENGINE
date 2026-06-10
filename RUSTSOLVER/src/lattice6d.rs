//! RUSTSOLVER v2 — 6D Lattice with EXACT Rational LLL + Babai CVP
//! ================================================================
//! 
//! Uses exact rational arithmetic (BigUint numerator/denominator pairs)
//! for LLL reduction, matching the Python Fraction-based LLL that
//! produces correct 2^43 residuals.
//!
//! The key insight: the Python prototype uses Fraction for exact GS,
//! and this Rust version does the same with BigUint rationals.

use num_bigint::BigUint;
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
    pub fn from_i64(v: i64) -> Self {
        if v < 0 { SignedBigUint { val: BigUint::from((-v) as u64), neg: true } }
        else { SignedBigUint { val: BigUint::from(v as u64), neg: false } }
    }
    pub fn zero() -> Self { SignedBigUint { val: BigUint::zero(), neg: false } }
    pub fn one() -> Self { SignedBigUint { val: BigUint::one(), neg: false } }
    pub fn is_zero(&self) -> bool { self.val.is_zero() }
    pub fn is_negative(&self) -> bool { self.neg && !self.val.is_zero() }
    pub fn abs(&self) -> BigUint { self.val.clone() }
    pub fn bits(&self) -> u64 { self.val.bits() }

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
}

impl fmt::Display for SignedBigUint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.neg { write!(f, "-{}", self.val) } else { write!(f, "{}", self.val) }
    }
}

// ============================================================
// EXACT RATIONAL (like Python's Fraction)
// ============================================================

#[derive(Clone, Debug)]
struct Rat {
    num: BigUint,  // always non-negative
    den: BigUint,  // always positive
    neg: bool,
}

impl Rat {
    fn from_u64(v: u64) -> Self {
        Rat { num: BigUint::from(v), den: BigUint::one(), neg: false }
    }
    fn from_i64(v: i64) -> Self {
        if v < 0 { Rat { num: BigUint::from((-v) as u64), den: BigUint::one(), neg: true } }
        else { Rat { num: BigUint::from(v as u64), den: BigUint::one(), neg: false } }
    }
    fn from_signed(sb: &SignedBigUint) -> Self {
        Rat { num: sb.val.clone(), den: BigUint::one(), neg: sb.neg }
    }
    fn zero() -> Self { Rat { num: BigUint::zero(), den: BigUint::one(), neg: false } }
    fn is_zero(&self) -> bool { self.num.is_zero() }

    fn neg(&self) -> Self {
        if self.num.is_zero() { self.clone() }
        else { Rat { num: self.num.clone(), den: self.den.clone(), neg: !self.neg } }
    }
    fn abs(&self) -> Self { Rat { num: self.num.clone(), den: self.den.clone(), neg: false } }

    fn add(&self, other: &Rat) -> Rat {
        // a/b + c/d = (a*d + c*b) / (b*d)
        let num_ad = &self.num * &other.den;
        let num_cb = &other.num * &self.den;
        let den = &self.den * &other.den;
        let (num, neg) = if self.neg == other.neg {
            (&num_ad + &num_cb, self.neg)
        } else if num_ad >= num_cb {
            (&num_ad - &num_cb, self.neg)
        } else {
            (&num_cb - &num_ad, other.neg)
        };
        let is_zero = num.is_zero();
        let mut r = Rat { num, den, neg: neg && !is_zero };
        r.simplify();
        r
    }

    fn sub(&self, other: &Rat) -> Rat { self.add(&other.neg()) }

    fn mul(&self, other: &Rat) -> Rat {
        let num = &self.num * &other.num;
        let den = &self.den * &other.den;
        let neg = (self.neg ^ other.neg) && !num.is_zero();
        let mut r = Rat { num, den, neg };
        r.simplify();
        r
    }

    fn div(&self, other: &Rat) -> Rat {
        if other.num.is_zero() { return Rat::zero(); }
        self.mul(&Rat { num: other.den.clone(), den: other.num.clone(), neg: other.neg })
    }

    fn simplify(&mut self) {
        if self.num.is_zero() { self.den = BigUint::one(); self.neg = false; return; }
        let g = gcd(&self.num, &self.den);
        if g > BigUint::one() { self.num /= &g; self.den /= &g; }
    }

    /// Round to nearest integer (as SignedBigUint)
    fn round_to_int(&self) -> SignedBigUint {
        let q = &self.num / &self.den;
        let r = &self.num % &self.den;
        let q_rounded = if &r + &r >= self.den { &q + BigUint::one() } else { q };
        SignedBigUint { val: q_rounded, neg: self.neg }
    }

    /// Absolute value comparison
    fn abs_cmp(&self, other: &Rat) -> std::cmp::Ordering {
        // |a/b| vs |c/d| => a*d vs c*b
        let ad = &self.num * &other.den;
        let cb = &other.num * &self.den;
        ad.cmp(&cb)
    }
}

fn gcd(a: &BigUint, b: &BigUint) -> BigUint {
    let mut a = a.clone();
    let mut b = b.clone();
    while !b.is_zero() { let t = b.clone(); b = &a % &b; a = t; }
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

const PI_A_HEX: &str = "114ca50f7a8e2f3f657c1108d9d44cfd8";
const PI_B_HEX: &str = "3086d221a7d46bcde86c90e49284eb15";

// ============================================================
// 6D LATTICE
// ============================================================

pub struct Lattice6D {
    range_bits: u32,
    rc: BigUint,
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
        let pi_a = BigUint::parse_bytes(PI_A_HEX.as_bytes(), 16).unwrap();
        let pi_b = BigUint::parse_bytes(PI_B_HEX.as_bytes(), 16).unwrap();

        // Verify Z[ω] factorization
        let norm = &pi_a * &pi_a - &pi_a * &pi_b + &pi_b * &pi_b;
        if norm == n { println!("  [6D] Z[ω] factorization verified ✓"); }
        else { println!("  [6D] WARNING: Z[ω] norm mismatch"); }

        println!("  [6D] Range: [2^{}, 2^{})", range_bits - 1, range_bits);
        println!("  [6D] Expected after LLL: n^(1/6) ≈ 2^{:.1}", n.bits() as f64 / 6.0);

        Lattice6D { range_bits, rc, n, lam, lam_sq, pi_a, pi_b }
    }

    pub fn range_center(&self) -> BigUint { self.rc.clone() }
    pub fn order(&self) -> BigUint { self.n.clone() }
    pub fn range_bits(&self) -> u32 { self.range_bits }

    pub fn build_and_reduce(&self) -> Vec<[SignedBigUint; DIM]> {
        let basis = self.build_basis();
        self.lll_exact(&basis)
    }

    fn build_basis(&self) -> Vec<[SignedBigUint; DIM]> {
        let neg_lam = &self.n - (&self.lam % &self.n);
        let neg_lam_sq = &self.n - (&self.lam_sq % &self.n);
        let rc_mod = &self.rc % &self.n;
        let pi_a_mod = &self.pi_a % &self.n;
        let pi_b_mod = &self.pi_b % &self.n;

        vec![
            [SignedBigUint::from_biguint(self.n.clone()), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(neg_lam),        SignedBigUint::from_u64(1),     SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(neg_lam_sq),      SignedBigUint::zero(),    SignedBigUint::from_u64(1),  SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(rc_mod),           SignedBigUint::zero(),    SignedBigUint::zero(), SignedBigUint::from_u64(1),  SignedBigUint::zero(), SignedBigUint::zero()],
            [SignedBigUint::from_biguint(pi_a_mod),         SignedBigUint::zero(),    SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::from_u64(1),  SignedBigUint::zero()],
            [SignedBigUint::from_biguint(pi_b_mod),         SignedBigUint::zero(),    SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::from_u64(1)],
        ]
    }

    // ================================================================
    // EXACT LLL using rational Gram-Schmidt (like Python's Fraction)
    // ================================================================

    fn lll_exact(&self, basis: &Vec<[SignedBigUint; DIM]>) -> Vec<[SignedBigUint; DIM]> {
        let n = DIM;
        let mut b: Vec<[SignedBigUint; DIM]> = basis.clone();
        let max_iter = 5000;
        let mut iter = 0;
        let mut k: usize = 1;

        println!("  [6D] Starting exact rational LLL (dim={})...", n);

        while k < n && iter < max_iter {
            iter += 1;

            // Compute Gram-Schmidt for b[0..k+1]
            let gs = gram_schmidt(&b, k + 1);

            // Size-reduce b[k] with respect to b[0..k]
            for j in (0..k).rev() {
                let mu = gs.mu(&b, k, j);
                let r = mu.round_to_int();
                if !r.is_zero() {
                    for d in 0..DIM {
                        b[k][d] = b[k][d].sub(&r.mul(&b[j][d]));
                    }
                }
            }

            // Recompute GS after size reduction
            let gs = gram_schmidt(&b, k + 1);

            // Lovász condition: |b*[k]|² >= (3/4 - μ²_{k,k-1}) * |b*[k-1]|²
            // Equivalently: |b*[k]|² + μ²_{k,k-1} * |b*[k-1]|² >= (3/4) * |b*[k-1]|²
            // Or: 4*|b*[k]|² + 4*μ²_{k,k-1}*|b*[k-1]|² >= 3*|b*[k-1]|²
            // Or: 4*|b*[k]|² >= (3 - 4*μ²_{k,k-1}) * |b*[k-1]|²
            // 
            // Using rationals: nsq[k] >= (3/4 - mu²_{k,k-1}) * nsq[k-1]
            // nsq[k] + mu²_{k,k-1} * nsq[k-1] >= (3/4) * nsq[k-1]
            // 4*(nsq[k] + mu² * nsq[k-1]) >= 3 * nsq[k-1]

            let mu_kk1 = gs.mu(&b, k, k - 1);
            let mu_sq = mu_kk1.mul(&mu_kk1);
            let nsq_k = gs.norm_sq[k].clone();
            let nsq_km1 = gs.norm_sq[k - 1].clone();

            // Check: nsq_k >= (Rat::from_u64(3) / Rat::from_u64(4) - mu_sq) * nsq_km1
            // = nsq_k + mu_sq * nsq_km1 >= (3/4) * nsq_km1
            // 4 * (nsq_k + mu_sq * nsq_km1) >= 3 * nsq_km1

            let three_quarters = Rat { num: BigUint::from(3u64), den: BigUint::from(4u64), neg: false };
            let lhs = nsq_k.add(&mu_sq.mul(&nsq_km1));
            let rhs = three_quarters.mul(&nsq_km1);

            if lhs.abs_cmp(&rhs) == std::cmp::Ordering::Less {
                // Lovász violated: swap b[k] and b[k-1]
                b.swap(k, k - 1);
                if k > 1 { k -= 1; }
            } else {
                k += 1;
            }
        }

        println!("  [6D] LLL complete ({} iterations):", iter);
        for (idx, v) in b.iter().enumerate() {
            let norm_sq: BigUint = v.iter()
                .map(|x| &x.val * &x.val)
                .fold(BigUint::zero(), |a, b| a + b);
            println!("    v{}: scalar=2^{}, ({},{},{},{},{},{}), |v|²=2^{}",
                     idx, v[0].bits(),
                     v[0].bits(), v[1].bits(), v[2].bits(),
                     v[3].bits(), v[4].bits(), v[5].bits(),
                     norm_sq.bits());
        }
        b
    }

    /// Babai CVP using exact rational Gram-Schmidt
    pub fn babai_cvp(
        &self,
        basis: &[[SignedBigUint; DIM]],
        target: &BigUint,
    ) -> (Vec<SignedBigUint>, [SignedBigUint; DIM]) {
        let t: [SignedBigUint; DIM] = [
            SignedBigUint::from_biguint(target.clone()),
            SignedBigUint::zero(), SignedBigUint::zero(),
            SignedBigUint::zero(), SignedBigUint::zero(),
            SignedBigUint::zero(),
        ];

        let gs = gram_schmidt(&basis.to_vec(), DIM);

        let mut current = t.clone();
        let mut coeffs = vec![SignedBigUint::zero(); DIM];

        for i in (0..DIM).rev() {
            let dot = dot_rat(&current, &gs.b_star[i]);
            let nsq = gs.norm_sq[i].clone();
            let ci_r = dot.div(&nsq);
            let ci = ci_r.round_to_int();
            coeffs[i] = ci.clone();

            if !ci.is_zero() {
                for d in 0..DIM {
                    current[d] = current[d].sub(&ci.mul(&basis[i][d]));
                }
            }
        }

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
            k_recon = k_recon.add(&coeffs[i].mul(&basis[i][0]));
        }
        if k_recon.neg { &self.n - (&k_recon.val % &self.n) }
        else { &k_recon.val % &self.n }
    }

    pub fn estimate_sphere_points(&self, max_residual_bits: u64) -> u64 {
        let exp = 6.0 * max_residual_bits as f64 - 256.0 + 5.168_f64.log2();
        if exp < 0.0 { 1 } else { (2.0_f64.powf(exp)) as u64 }
    }
}

// ============================================================
// GRAM-SCHMIDT WITH EXACT RATIONALS
// ============================================================

struct GramSchmidtRat {
    /// b*[i] as rational vectors
    b_star: Vec<[Rat; DIM]>,
    /// |b*[i]|² as rational
    norm_sq: Vec<Rat>,
}

impl GramSchmidtRat {
    /// Compute μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
    fn mu(&self, basis: &[[SignedBigUint; DIM]], i: usize, j: usize) -> Rat {
        let dot = dot_signed_rat(&basis[i], &self.b_star[j]);
        dot.div(&self.norm_sq[j])
    }
}

fn gram_schmidt(basis: &[[SignedBigUint; DIM]], k: usize) -> GramSchmidtRat {
    let mut b_star: Vec<[Rat; DIM]> = Vec::with_capacity(k);
    let mut norm_sq: Vec<Rat> = Vec::with_capacity(k);

    for i in 0..k {
        // Start with b[i] as rationals
        let mut bi: [Rat; DIM] = std::array::from_fn(|d| Rat::from_signed(&basis[i][d]));

        // Subtract projections: b*[i] = b[i] - Σ_{j<i} μ_{i,j} * b*[j]
        for j in 0..i {
            if norm_sq[j].is_zero() { continue; }
            // μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
            // IMPORTANT: Use original b[i], not current bi, for the dot product
            // This is the correct Gram-Schmidt formula
            let mu = dot_signed_rat(&basis[i], &b_star[j]).div(&norm_sq[j]);

            for d in 0..DIM {
                bi[d] = bi[d].sub(&mu.mul(&b_star[j][d]));
            }
        }

        // Compute |b*[i]|²
        let nsq: Rat = bi.iter().fold(Rat::zero(), |acc, x| acc.add(&x.mul(x)));
        norm_sq.push(nsq);
        b_star.push(bi);
    }

    GramSchmidtRat { b_star, norm_sq }
}

/// Dot product of a SignedBigUint vector with a Rat vector
fn dot_signed_rat(a: &[SignedBigUint; DIM], b: &[Rat; DIM]) -> Rat {
    let mut sum = Rat::zero();
    for d in 0..DIM {
        let a_r = Rat::from_signed(&a[d]);
        sum = sum.add(&a_r.mul(&b[d]));
    }
    sum
}

/// Dot product of two SignedBigUint vectors (both Rat)
fn dot_rat(a: &[SignedBigUint; DIM], b: &[Rat; DIM]) -> Rat {
    dot_signed_rat(a, b)
}
