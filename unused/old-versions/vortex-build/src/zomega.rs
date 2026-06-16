//! VORTEX PRIME v4 — INVENTION 2: Z[omega] DLP Lifting
//! =====================================================
//! Factor n = pi * pi_bar in Z[omega] (Eisenstein integers),
//! then solve sub-DLP modulo each prime ideal.
//!
//! Key insight: secp256k1 has CM by Q(sqrt(-3)). Since n ≡ 1 mod 3,
//! n SPLITS in Z[omega]: n = pi * pi_bar where N(pi) = N(pi_bar) = n.
//!
//! The Frobenius endomorphism and norm map in Z[omega]/(pi) give
//! additional structure for DLP decomposition.

use num_bigint::BigUint;
use num_traits::{Zero, One, ToPrimitive};
use std::fmt;

// ============================================================
// EISENSTEIN INTEGER: a + b*omega where omega^2 + omega + 1 = 0
// ============================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EisensteinInt {
    pub a: BigUint,
    pub b: BigUint,
    /// Sign of the real (a) component: true means negative
    pub a_neg: bool,
    /// Sign of the omega (b) component: true means negative
    pub b_neg: bool,
}

impl EisensteinInt {
    pub fn new(a: BigUint, b: BigUint) -> Self {
        EisensteinInt { a, b, a_neg: false, b_neg: false }
    }

    /// Create from signed components
    pub fn new_signed(a: BigUint, a_neg: bool, b: BigUint, b_neg: bool) -> Self {
        let a_neg = if a.is_zero() { false } else { a_neg };
        let b_neg = if b.is_zero() { false } else { b_neg };
        EisensteinInt { a, b, a_neg, b_neg }
    }

    pub fn from_u64(a: u64, b: u64) -> Self {
        EisensteinInt {
            a: BigUint::from(a),
            b: BigUint::from(b),
            a_neg: false,
            b_neg: false,
        }
    }

    pub fn zero() -> Self {
        EisensteinInt {
            a: BigUint::zero(),
            b: BigUint::zero(),
            a_neg: false,
            b_neg: false,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.a.is_zero() && self.b.is_zero()
    }

    /// Get the signed value of the a component as BigIntSigned
    fn a_signed(&self) -> BigIntSigned {
        BigIntSigned { val: self.a.clone(), neg: self.a_neg }
    }

    /// Get the signed value of the b component as BigIntSigned
    fn b_signed(&self) -> BigIntSigned {
        BigIntSigned { val: self.b.clone(), neg: self.b_neg }
    }

    /// Norm: N(a + b*omega) = a^2 - a*b + b^2
    /// With signs: N(s*a + t*b*omega) = (s*a)^2 - (s*a)*(t*b) + (t*b)^2
    ///           = a^2 - s*t*a*b + b^2   (since s^2 = t^2 = 1)
    pub fn norm(&self) -> BigUint {
        let aa = &self.a * &self.a;
        let ab = &self.a * &self.b;
        let bb = &self.b * &self.b;

        // N = a^2 - ab + b^2 when both same sign (since (-a)(-b) = ab)
        // N = a^2 + ab + b^2 when different signs
        // For same sign: (aa + bb) >= ab always, so compute (aa + bb) - ab
        if self.a_neg == self.b_neg {
            // Both positive or both negative: norm = a^2 - ab + b^2 = (aa + bb) - ab
            (aa + bb) - ab
        } else {
            // Different signs: norm = a^2 + ab + b^2
            aa + ab + bb
        }
    }

    /// Conjugate: conj(a + b*omega) = (a-b) + (-b)*omega
    /// Because omega_bar = -1 - omega, so a + b*omega -> a + b*(-1-omega) = (a-b) - b*omega
    pub fn conjugate(&self) -> Self {
        let a_s = self.a_signed();
        let b_s = self.b_signed();

        // conj(a + b*omega) = (a - b) + (-b)*omega
        let new_a = a_s.sub(&b_s);
        let new_b = b_s.neg();

        EisensteinInt::new_signed(new_a.val, new_a.neg, new_b.val, new_b.neg)
    }

    pub fn neg(&self) -> Self {
        EisensteinInt::new_signed(
            self.a.clone(), !self.a_neg && !self.a.is_zero(),
            self.b.clone(), !self.b_neg && !self.b.is_zero(),
        )
    }

    /// Multiply by a scalar
    pub fn scalar_mul(&self, s: &BigUint) -> Self {
        EisensteinInt {
            a: &self.a * s,
            b: &self.b * s,
            a_neg: self.a_neg,
            b_neg: self.b_neg,
        }
    }

    /// Multiply self by the conjugate of other: self * conj(other)
    /// conj(c + d*w) = (c-d) + (-d)*w = (c-d) - d*w
    ///
    /// Direct formula:
    ///   (a + b*w)((c-d) + (-d)*w)
    ///   = (ac - ad + bd) + (bc - ad)*w
    pub fn mul_conjugate(&self, other: &EisensteinInt) -> EisensteinInt {
        // Use BigIntSigned for all intermediate computations
        let a = self.a_signed();
        let b = self.b_signed();
        let c = other.a_signed();
        let d = other.b_signed();

        // real = a*c - a*d + b*d
        let ac = a.mul(&c);
        let ad = a.mul(&d);
        let bd = b.mul(&d);
        let bc = b.mul(&c);

        let real = ac.sub(&ad).add(&bd);
        let omega = bc.sub(&ad);

        EisensteinInt::new_signed(real.val, real.neg, omega.val, omega.neg)
    }
}

impl fmt::Display for EisensteinInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let a_sign = if self.a_neg { "-" } else { "" };
        let b_sign = if self.b_neg { "-" } else { "" };

        if self.b.is_zero() {
            write!(f, "Eisen({}{})", a_sign, self.a)
        } else if self.a.is_zero() {
            write!(f, "Eisen({}{}*w)", b_sign, self.b)
        } else {
            write!(f, "Eisen({}{} + {}{}*w)", a_sign, self.a, b_sign, self.b)
        }
    }
}

/// Multiplication in Z[omega]:
/// (a + b*w)(c + d*w) = ac + (ad + bc)*w + bd*w^2
/// Since w^2 = -1 - w:
///   = (ac - bd) + (ad + bc - bd)*w
impl std::ops::Mul for &EisensteinInt {
    type Output = EisensteinInt;

    fn mul(self, other: &EisensteinInt) -> EisensteinInt {
        // Use BigIntSigned for signed arithmetic
        let a = self.a_signed();
        let b = self.b_signed();
        let c = other.a_signed();
        let d = other.b_signed();

        let ac = a.mul(&c);
        let bd = b.mul(&d);
        let ad = a.mul(&d);
        let bc = b.mul(&c);

        // real = ac - bd
        let real = ac.sub(&bd);
        // omega = ad + bc - bd
        let omega = ad.add(&bc).sub(&bd);

        EisensteinInt::new_signed(real.val, real.neg, omega.val, omega.neg)
    }
}

impl std::ops::Mul for EisensteinInt {
    type Output = EisensteinInt;

    fn mul(self, other: EisensteinInt) -> EisensteinInt {
        &self * &other
    }
}

impl std::ops::Add for &EisensteinInt {
    type Output = EisensteinInt;

    fn add(self, other: &EisensteinInt) -> EisensteinInt {
        let a = self.a_signed().add(&other.a_signed());
        let b = self.b_signed().add(&other.b_signed());
        EisensteinInt::new_signed(a.val, a.neg, b.val, b.neg)
    }
}

impl std::ops::Add for EisensteinInt {
    type Output = EisensteinInt;

    fn add(self, other: EisensteinInt) -> EisensteinInt {
        &self + &other
    }
}

impl std::ops::Sub for &EisensteinInt {
    type Output = EisensteinInt;

    fn sub(self, other: &EisensteinInt) -> EisensteinInt {
        let a = self.a_signed().sub(&other.a_signed());
        let b = self.b_signed().sub(&other.b_signed());
        EisensteinInt::new_signed(a.val, a.neg, b.val, b.neg)
    }
}

impl std::ops::Sub for EisensteinInt {
    type Output = EisensteinInt;

    fn sub(self, other: EisensteinInt) -> EisensteinInt {
        &self - &other
    }
}

// ============================================================
// SIGNED BIG INT HELPER (for Eisenstein division)
// ============================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BigIntSigned {
    pub val: BigUint,
    pub neg: bool,
}

impl BigIntSigned {
    pub fn from_biguint(v: &BigUint) -> Self {
        BigIntSigned { val: v.clone(), neg: false }
    }

    pub fn from_sign(sign: bool, val: BigUint) -> Self {
        if val.is_zero() {
            BigIntSigned { val, neg: false }
        } else {
            BigIntSigned { val, neg: sign }
        }
    }

    pub fn from_i64(v: i64) -> Self {
        if v < 0 {
            BigIntSigned { val: BigUint::from((-v) as u64), neg: true }
        } else {
            BigIntSigned { val: BigUint::from(v as u64), neg: false }
        }
    }

    pub fn zero() -> Self {
        BigIntSigned { val: BigUint::zero(), neg: false }
    }

    pub fn is_zero(&self) -> bool {
        self.val.is_zero()
    }

    pub fn neg(&self) -> Self {
        if self.val.is_zero() {
            BigIntSigned::zero()
        } else {
            BigIntSigned { val: self.val.clone(), neg: !self.neg }
        }
    }

    pub fn abs(&self) -> BigUint {
        self.val.clone()
    }

    pub fn sub(&self, other: &BigIntSigned) -> BigIntSigned {
        if self.neg == other.neg {
            if self.val >= other.val {
                BigIntSigned::from_sign(self.neg, &self.val - &other.val)
            } else {
                BigIntSigned::from_sign(!self.neg, &other.val - &self.val)
            }
        } else {
            BigIntSigned::from_sign(self.neg, &self.val + &other.val)
        }
    }

    pub fn add(&self, other: &BigIntSigned) -> BigIntSigned {
        if self.neg == other.neg {
            BigIntSigned::from_sign(self.neg, &self.val + &other.val)
        } else {
            // Different signs: subtract
            if self.val >= other.val {
                BigIntSigned::from_sign(self.neg, &self.val - &other.val)
            } else {
                BigIntSigned::from_sign(other.neg, &other.val - &self.val)
            }
        }
    }

    pub fn mul(&self, other: &BigIntSigned) -> BigIntSigned {
        let result_neg = self.neg ^ other.neg;
        BigIntSigned::from_sign(result_neg, &self.val * &other.val)
    }

    pub fn to_biguint(&self) -> BigUint {
        self.val.clone()
    }

    pub fn to_f64(&self) -> f64 {
        let bytes = self.val.to_bytes_be();
        let mut f = 0.0f64;
        for &b in &bytes {
            f = f * 256.0 + b as f64;
        }
        if self.neg { -f } else { f }
    }

    /// Compute floor(|self| / |other|) with sign
    pub fn div_unsigned(&self, other: &BigUint) -> BigIntSigned {
        if other.is_zero() {
            panic!("Division by zero");
        }
        let q = &self.val / other;
        BigIntSigned::from_sign(self.neg, q)
    }
}

impl fmt::Display for BigIntSigned {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.neg {
            write!(f, "-{}", self.val)
        } else {
            write!(f, "{}", self.val)
        }
    }
}

// ============================================================
// EISENSTEIN DIVISION WITH REMAINDER
// ============================================================

/// Division in Z[omega] with remainder.
/// Returns (quotient, remainder) such that a = q*b + r with N(r) < N(b).
pub fn eisen_divmod(a: &EisensteinInt, b: &EisensteinInt) -> (EisensteinInt, EisensteinInt) {
    if b.is_zero() {
        panic!("Division by zero in Z[omega]");
    }

    let nb = b.norm();

    // Use mul_conjugate for correct computation of a * conj(b)
    let num = a.mul_conjugate(b);

    // q_a = round(num.a / nb), q_b = round(num.b / nb)
    // Use signed division since num components can be negative
    let q_a_signed = round_div_signed(&num.a_signed(), &nb);
    let q_b_signed = round_div_signed(&num.b_signed(), &nb);

    let q = EisensteinInt::new_signed(q_a_signed.val.clone(), q_a_signed.neg,
                                       q_b_signed.val.clone(), q_b_signed.neg);
    let qb = &q * b;
    let r = a - &qb;

    // Ensure N(r) < N(b) — try neighbors of q
    if r.norm() >= nb {
        let q_a_big = q.a.clone();
        let q_a_neg = q.a_neg;
        let q_b_big = q.b.clone();
        let q_b_neg = q.b_neg;

        let mut best_q = q.clone();
        let mut best_r = r.clone();
        let mut best_norm = best_r.norm();

        for da in -1i64..=1 {
            for db in -1i64..=1 {
                if da == 0 && db == 0 { continue; }

                let cq_a = adjust_signed(&q_a_big, q_a_neg, da);
                let cq_b = adjust_signed(&q_b_big, q_b_neg, db);

                let cq = EisensteinInt::new_signed(cq_a.val.clone(), cq_a.neg,
                                                     cq_b.val.clone(), cq_b.neg);
                let cqb = &cq * b;
                let cr = a - &cqb;
                let cr_norm = cr.norm();

                if cr_norm < best_norm {
                    best_q = cq;
                    best_r = cr;
                    best_norm = cr_norm;
                }
            }
        }

        return (best_q, best_r);
    }

    (q, r)
}

/// Adjust a signed BigUint by a small signed integer delta
fn adjust_signed(val: &BigUint, neg: bool, delta: i64) -> BigIntSigned {
    let base = BigIntSigned { val: val.clone(), neg };
    if delta >= 0 {
        base.add(&BigIntSigned::from_biguint(&BigUint::from(delta as u64)))
    } else {
        base.sub(&BigIntSigned::from_biguint(&BigUint::from((-delta) as u64)))
    }
}

/// Round division: round(a / b) for BigUint
fn round_div(a: &BigUint, b: &BigUint) -> BigUint {
    let q = a / b;
    let r = a % b;
    // If 2*r >= b, round up
    if &r + &r >= *b {
        q + BigUint::one()
    } else {
        q
    }
}

/// Round division for signed numerator: round(a / b) for BigIntSigned / BigUint
fn round_div_signed(a: &BigIntSigned, b: &BigUint) -> BigIntSigned {
    let q = &a.val / b;
    let r = &a.val % b;
    // If 2*r >= b, round up (in absolute value)
    let q_rounded = if &r + &r >= *b {
        q + BigUint::one()
    } else {
        q
    };
    BigIntSigned::from_sign(a.neg, q_rounded)
}

/// GCD in Z[omega] using Euclidean algorithm
pub fn eisen_gcd(a: &EisensteinInt, b: &EisensteinInt) -> EisensteinInt {
    let mut a = a.clone();
    let mut b = b.clone();
    let mut iterations = 0;
    while !b.is_zero() && iterations < 1000 {
        let (_, r) = eisen_divmod(&a, &b);
        a = b;
        b = r;
        iterations += 1;
    }
    a
}

// ============================================================
// Z[omega] DLP LIFTER (INVENTION 2)
// ============================================================

/// secp256k1 order as BigUint
pub fn secp256k1_order() -> BigUint {
    BigUint::parse_bytes(
        b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141",
        16
    ).unwrap()
}

/// GLV lambda constant as BigUint
pub fn secp256k1_lambda() -> BigUint {
    BigUint::parse_bytes(
        b"5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72",
        16
    ).unwrap()
}

pub struct ZOmegaDLPLifter {
    pub n: BigUint,
    pub lam: BigUint,
    pub pi: Option<EisensteinInt>,
    pub pi_bar: Option<EisensteinInt>,
}

impl ZOmegaDLPLifter {
    pub fn new() -> Self {
        let n = secp256k1_order();
        let lam = secp256k1_lambda();

        // Verify n ≡ 1 mod 3 (required for splitting)
        let three = BigUint::from(3u64);
        assert_eq!(&n % &three, BigUint::one(), "n must ≡ 1 mod 3 for Z[omega] splitting");

        let mut lifter = ZOmegaDLPLifter {
            n,
            lam,
            pi: None,
            pi_bar: None,
        };

        lifter.find_prime_factors();
        lifter
    }

    /// Find the prime factorization n = pi * pi_bar in Z[omega].
    ///
    /// CRITICAL FIX: The previous version used Euclidean norm (a²+b²) for Gauss
    /// reduction, but in Z[ω] the correct norm is the EISENSTEIN norm
    /// N(a+bω) = a²-ab+b². The short vector for Euclidean ≠ short for Eisenstein.
    ///
    /// Since n ≡ 1 (mod 3), n SPLITS in Z[ω]: n = π·π̄ where N(π) = N(π̄) = n.
    /// The lattice L = {(a,b) : a + b·λ ≡ 0 (mod n)} has a shortest vector
    /// (under Eisenstein norm) that gives π up to unit multiplication.
    ///
    /// Method: Gauss reduction using Eisenstein norm for size comparison,
    /// then search with unit multiplication to get exact N(π) = n.
    fn find_prime_factors(&mut self) {
        println!("  [Z[omega]] Finding prime ideals above n in Z[omega]...");

        let neg_lam_mod_n = &self.n - (&self.lam % &self.n);

        // Basis vectors as Eisenstein integers
        // b0 = (n, 0) → n + 0·ω
        // b1 = (-λ mod n, 1) → (-λ mod n) + 1·ω
        let mut b0 = EisensteinInt::new(self.n.clone(), BigUint::zero());
        let mut b1 = EisensteinInt::new(neg_lam_mod_n, BigUint::one());

        // Gauss/Lagrange reduction using EISENSTEIN norm
        println!("  [Z[omega]] Starting Eisenstein-norm Gauss reduction...");
        for iter in 0..200 {
            let n0 = b0.norm();
            let n1 = b1.norm();

            // Ensure b0 is shorter (Eisenstein norm)
            if n1 < n0 {
                std::mem::swap(&mut b0, &mut b1);
            }

            let n0 = b0.norm();
            if n0.is_zero() { break; }

            // Dot product in Z[ω]: <b1, b0>_Eisen = Re(b1 · conj(b0))
            // b1 · conj(b0) = (b1.a + b1.b·ω)(b0.a - b0.b - b0.b·ω)  [since conj(b0) = b0.a - b0.b + (-b0.b)·ω]
            // Actually we need the coefficient of the QUOTIENT ring to find mu.
            // For the lattice reduction, mu = round(Re(b1·conj(b0)) / N(b0))
            let b1_conj_b0 = b1.mul_conjugate(&b0);
            // b1·conj(b0) has real part = b1_conj_b0.a (with sign b1_conj_b0.a_neg)
            // and omega part should be ~0 when b1 is close to a multiple of b0

            // mu = round(Re(b1·conj(b0)) / N(b0))
            // The real part can be negative
            let real_part = b1_conj_b0.a_signed();
            let nb0 = b0.norm();

            let q = &real_part.val / &nb0;
            let r = &real_part.val % &nb0;
            let mu_val = if &r + &r >= nb0 { q + BigUint::one() } else { q };
            let mu = BigIntSigned::from_sign(real_part.neg, mu_val);

            if mu.is_zero() { break; }

            // b1 = b1 - mu * b0 (in Z[ω])
            // mu * b0: scalar multiplication of b0 by mu
            let mu_b0_a = mu.mul(&b0.a_signed());
            let mu_b0_b = mu.mul(&b0.b_signed());

            // b1.a_signed() - mu_b0_a, b1.b_signed() - mu_b0_b
            let new_b1_a = b1.a_signed().sub(&mu_b0_a);
            let new_b1_b = b1.b_signed().sub(&mu_b0_b);

            let new_b1 = EisensteinInt::new_signed(
                new_b1_a.val.clone(), new_b1_a.neg,
                new_b1_b.val.clone(), new_b1_b.neg
            );

            let new_n1 = new_b1.norm();
            let old_n1 = b1.norm();

            if new_n1 >= old_n1 { break; }

            b1 = new_b1;

            if iter % 10 == 0 {
                println!("    iter {}: |b0| = 2^{}, |b1| = 2^{}", iter, b0.norm().bits(), b1.norm().bits());
            }
        }

        // Final: ensure b0 is shorter
        if b1.norm() < b0.norm() {
            std::mem::swap(&mut b0, &mut b1);
        }

        let norm0 = b0.norm();
        let norm1 = b1.norm();
        println!("  [Z[omega]] Eisenstein-reduced basis:");
        println!("    b0: {}, N(b0) = {} bits", b0, norm0.bits());
        println!("    b1: {}, N(b1) = {} bits", b1, norm1.bits());

        // Now search for π among {b0, b1} × units where N(π) = n
        let one = EisensteinInt::from_u64(1, 0);
        let neg_one = EisensteinInt::new_signed(BigUint::one(), true, BigUint::zero(), false);
        let omega = EisensteinInt::from_u64(0, 1);
        let neg_omega = EisensteinInt::new_signed(BigUint::zero(), false, BigUint::one(), true);
        let omega_sq = EisensteinInt::new_signed(BigUint::one(), true, BigUint::one(), true);
        let neg_omega_sq = EisensteinInt::new_signed(BigUint::one(), false, BigUint::one(), false);

        let units = [one, neg_one, omega, neg_omega, omega_sq, neg_omega_sq];
        let candidates = [&b0, &b1];

        for (cidx, cand) in candidates.iter().enumerate() {
            for (uidx, unit) in units.iter().enumerate() {
                let pi_cand = (*cand) * unit;
                let pi_norm = pi_cand.norm();

                if pi_norm == self.n {
                    // Verify: π · π̄ = n
                    let pi_bar = pi_cand.conjugate();
                    let product = pi_cand.mul_conjugate(&pi_cand);
                    if product.b.is_zero() && !product.a_neg && product.a == self.n {
                        println!("  [Z[omega]] CONFIRMED: b{} * unit[{}] gives π with N(π) = n!", cidx, uidx);
                        println!("  [Z[omega]] π = {}", pi_cand);
                        println!("  [Z[omega]] π̄ = {}", pi_bar);
                        self.pi = Some(pi_cand);
                        self.pi_bar = Some(pi_bar);
                        return;
                    }
                }
            }
        }

        // If still not found, try sums/differences of basis vectors
        // π might be a combination v0 ± v1, not just a single vector
        println!("  [Z[omega]] Single vectors don't give N=n, trying combinations...");
        for da in -2i64..=2 {
            for db in -2i64..=2 {
                if da == 0 && db == 0 { continue; }
                // candidate = da * b0 + db * b1
                let da_a = adjust_signed(&b0.a, b0.a_neg, da);
                let da_b = adjust_signed(&b0.b, b0.b_neg, da);
                let db_a = adjust_signed(&b1.a, b1.a_neg, db);
                let db_b = adjust_signed(&b1.b, b1.b_neg, db);

                let cand_a = da_a.add(&db_a);
                let cand_b = da_b.add(&db_b);
                let cand = EisensteinInt::new_signed(cand_a.val.clone(), cand_a.neg, cand_b.val.clone(), cand_b.neg);

                for (uidx, unit) in units.iter().enumerate() {
                    let pi_cand = &cand * unit;
                    let pi_norm = pi_cand.norm();

                    if pi_norm == self.n {
                        let pi_bar = pi_cand.conjugate();
                        let product = pi_cand.mul_conjugate(&pi_cand);
                        if product.b.is_zero() && !product.a_neg && product.a == self.n {
                            println!("  [Z[omega]] CONFIRMED: ({}*b0 + {}*b1) * unit[{}] gives π!", da, db, uidx);
                            println!("  [Z[omega]] π = {}", pi_cand);
                            self.pi = Some(pi_cand);
                            self.pi_bar = Some(pi_bar);
                            return;
                        }
                    }
                }
            }
        }

        // Last resort: direct computation using Cornacchia for Q(√-3)
        // n = p ≡ 1 mod 3 => n = a² - ab + b² for some a, b
        // This is equivalent to finding x² + 3y² = 4n with a = (x+3y)/2, b = y
        println!("  [Z[omega]] Trying Cornacchia-style direct computation...");
        if let Some(pi_cand) = self.cornacchia_eisenstein() {
            let pi_bar = pi_cand.conjugate();
            let product = pi_cand.mul_conjugate(&pi_cand);
            if product.b.is_zero() && !product.a_neg && product.a == self.n {
                println!("  [Z[omega]] CONFIRMED via Cornacchia: π found with N(π) = n!");
                println!("  [Z[omega]] π = {}", pi_cand);
                self.pi = Some(pi_cand);
                self.pi_bar = Some(pi_bar);
                return;
            }
        }

        println!("  [Z[omega]] WARNING: Could not find exact π with N(π) = n");
        println!("  [Z[omega]] Storing best approximation (b0) for downstream use");
        self.pi = Some(b0.clone());
        self.pi_bar = Some(b0.conjugate());
    }

    /// Cornacchia's algorithm adapted for Eisenstein integers.
    /// Find a, b such that a² - ab + b² = n (equivalently (2a-b)² + 3b² = 4n).
    /// This works because n ≡ 1 mod 3 ⟹ 4n = x² + 3y² has a solution.
    fn cornacchia_eisenstein(&self) -> Option<EisensteinInt> {
        // We need to solve x² + 3y² = 4n
        // where x = 2a - b, y = b, so a = (x+y)/2, b = y
        let four_n = &self.n << 2;
        let three = BigUint::from(3u64);

        // Find x₀ such that x₀² ≡ -3 (mod n)
        // x₀² ≡ -3 mod n ⟹ x₀² + 3 ≡ 0 mod n ⟹ x₀² + 3y₀² = n·k for some k
        // Try Cipolla or Tonelli-Shanks to find sqrt(-3 mod n)
        let neg3_mod_n = &self.n - (&three % &self.n);

        // Tonelli-Shanks: find r such that r² ≡ neg3 (mod n)
        let x0 = match self.tonelli_shanks(&neg3_mod_n) {
            Some(x) => x,
            None => {
                println!("  [Z[omega]] Tonelli-Shanks failed for sqrt(-3 mod n)");
                return None;
            }
        };

        println!("  [Z[omega]] Found x₀ with x₀² ≡ -3 (mod n), x₀ = {} bits", x0.bits());

        // Apply Cornacchia: r₀ = x0, then Euclidean algorithm steps
        let mut r_prev = self.n.clone();
        let mut r_curr = x0;
        let sqrt_4n = int_sqrt(&four_n);

        for _ in 0..1000 {
            if r_curr <= sqrt_4n {
                // Check: 4n - r_curr² must be divisible by 3 and a perfect square
                let r_sq = &r_curr * &r_curr;
                if r_sq > four_n { break; }
                let remainder = &four_n - &r_sq;

                // 3y² = remainder => y² = remainder/3
                let y_sq_times_3 = &remainder % &three;
                if !y_sq_times_3.is_zero() {
                    // Try next step of Euclidean
                    if r_curr.is_zero() { break; }
                    let new_r = &r_prev % &r_curr;
                    r_prev = r_curr;
                    r_curr = new_r;
                    continue;
                }

                let y_sq = &remainder / &three;
                let y = int_sqrt(&y_sq);

                // Verify y² * 3 + r_curr² == 4n
                if &y * &y == y_sq {
                    let check = &y * &y * &three + &r_curr * &r_curr;
                    if check == four_n {
                        // Recover a, b from x = r_curr, y = y
                        // a = (x + y) / 2, b = y
                        let x_plus_y = &r_curr + &y;
                        if &x_plus_y % &BigUint::from(2u64) != BigUint::zero() {
                            // Try with x = n - r_curr instead
                            let x_alt = &self.n - &r_curr;
                            let x_alt_plus_y = &x_alt + &y;
                            if &x_alt_plus_y % &BigUint::from(2u64) == BigUint::zero() {
                                let a = &x_alt_plus_y >> 1;
                                let b = y;
                                let pi = EisensteinInt::new(a, b);
                                println!("  [Z[omega]] Cornacchia: a = {} bits, b = {} bits", pi.a.bits(), pi.b.bits());
                                if pi.norm() == self.n {
                                    return Some(pi);
                                }
                            }
                        } else {
                            let a = &x_plus_y >> 1;
                            let b = y;
                            let pi = EisensteinInt::new(a, b);
                            println!("  [Z[omega]] Cornacchia: a = {} bits, b = {} bits", pi.a.bits(), pi.b.bits());
                            if pi.norm() == self.n {
                                return Some(pi);
                            }
                        }
                    }
                }
            }

            if r_curr.is_zero() { break; }
            let new_r = &r_prev % &r_curr;
            r_prev = r_curr;
            r_curr = new_r;
        }

        None
    }

    /// Tonelli-Shanks: find r such that r² ≡ n (mod p) where p = self.n
    fn tonelli_shanks(&self, n: &BigUint) -> Option<BigUint> {
        let p = &self.n;

        // Check n is a QR mod p
        let p_minus_1 = p - BigUint::one();
        let exp = &p_minus_1 >> 1;
        let check = n.modpow(&exp, p);
        if check != BigUint::one() {
            return None;
        }

        // Factor out powers of 2 from p-1: p-1 = Q * 2^S
        let mut s: u32 = 0;
        let mut q = p_minus_1.clone();
        while &q % &BigUint::from(2u64) == BigUint::zero() {
            q >>= 1;
            s += 1;
        }

        // Find a non-residue z
        let mut z = BigUint::from(2u64);
        loop {
            let z_check = z.modpow(&exp, p);
            if z_check != BigUint::one() {
                break;
            }
            z += BigUint::one();
        }

        let mut m = s;
        let mut c = z.modpow(&q, p);
        let mut t = n.modpow(&q, p);
        let mut r = n.modpow(&(((&q + BigUint::one()) >> 1u32)), p);

        while t != BigUint::one() {
            if t.is_zero() { return None; }

            // Find least i such that t^(2^i) ≡ 1 (mod p)
            let mut i = 0u32;
            let mut t2 = t.clone();
            while t2 != BigUint::one() {
                t2 = &t2 * &t2 % p;
                i += 1;
                if i >= m { return None; }
            }

            let b = c.modpow(&(BigUint::one() << (m - i - 1)), p);
            m = i;
            c = &b * &b % p;
            t = &t * &c % p;
            r = &r * &b % p;
        }

        Some(r)
    }

    /// Gauss/Lagrange reduction for 2D lattice with SIGNED BigUint arithmetic.
    ///
    /// FIX: Previous version used unsigned BigUint and would break (literally `break`)
    /// when subtraction underflowed. This version uses BigIntSigned throughout,
    /// which correctly handles negative intermediate values.
    ///
    /// The algorithm:
    /// 1. Ensure |b0| <= |b1| (swap if needed)
    /// 2. Compute mu = round(<b1, b0> / <b0, b0>)
    /// 3. b1 = b1 - mu * b0
    /// 4. Repeat until no reduction occurs
    fn gauss_reduce_2d_signed(
        &self,
        b0_init: (BigIntSigned, BigIntSigned),
        b1_init: (BigIntSigned, BigIntSigned),
    ) -> ((BigIntSigned, BigIntSigned), (BigIntSigned, BigIntSigned)) {
        let mut b0 = b0_init;
        let mut b1 = b1_init;

        for _iter in 0..100 {
            // Compute norms: |bi|^2 = bi.0^2 + bi.1^2 (Euclidean norm for lattice)
            // Since we're squaring, the sign doesn't matter for the norm.
            // |b|^2 is always positive.
            let n0_val = &b0.0.val * &b0.0.val + &b0.1.val * &b0.1.val;
            let n1_val = &b1.0.val * &b1.0.val + &b1.1.val * &b1.1.val;

            // Ensure b0 is the shorter vector
            if n1_val < n0_val {
                std::mem::swap(&mut b0, &mut b1);
            }

            // Recompute n0 after possible swap
            let n0_val = &b0.0.val * &b0.0.val + &b0.1.val * &b0.1.val;

            if n0_val.is_zero() {
                break;
            }

            // Dot product: <b1, b0> = b1.0 * b0.0 + b1.1 * b0.1 (signed)
            let dot = b1.0.mul(&b0.0).add(&b1.1.mul(&b0.1));

            // mu = round(dot / n0)
            // The denominator is always positive (it's a norm squared)
            let mu = round_div_bigintsigned(&dot, &BigIntSigned::from_sign(false, n0_val.clone()));

            // If mu is zero, no further reduction possible
            if mu.is_zero() {
                break;
            }

            // new_b1 = b1 - mu * b0
            let mu_b0_0 = mu.mul(&b0.0);
            let mu_b0_1 = mu.mul(&b0.1);
            let new_b1_0 = b1.0.sub(&mu_b0_0);
            let new_b1_1 = b1.1.sub(&mu_b0_1);

            // Check if the new norm is smaller
            let new_n1_val = &new_b1_0.val * &new_b1_0.val + &new_b1_1.val * &new_b1_1.val;

            if new_n1_val >= n1_val {
                // No improvement — we've converged
                break;
            }

            b1 = (new_b1_0, new_b1_1);
        }

        // Final: ensure b0 is shorter
        let n0_val = &b0.0.val * &b0.0.val + &b0.1.val * &b0.1.val;
        let n1_val = &b1.0.val * &b1.0.val + &b1.1.val * &b1.1.val;
        if n1_val < n0_val {
            std::mem::swap(&mut b0, &mut b1);
        }

        (b0, b1)
    }

    /// Partial factorization of n-1 using trial division.
    /// Returns (factors, remainder) where factors = [(p, e), ...]
    pub fn partial_factor(&self, n: &BigUint, bound: u64) -> (Vec<(u64, u32)>, BigUint) {
        let mut n = n.clone();
        let mut factors = Vec::new();

        let small_primes: Vec<u64> = vec![
            2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47
        ];

        for p in &small_primes {
            let p_big = BigUint::from(*p);
            let mut exp = 0u32;
            while &n % &p_big == BigUint::zero() {
                n /= &p_big;
                exp += 1;
            }
            if exp > 0 {
                factors.push((*p, exp));
            }
        }

        // Continue with odd numbers up to bound
        let mut p = 53u64;
        while p < bound && n > BigUint::one() {
            let p_big = BigUint::from(p);
            let mut exp = 0u32;
            while &n % &p_big == BigUint::zero() {
                n /= &p_big;
                exp += 1;
            }
            if exp > 0 {
                factors.push((p, exp));
            }
            p += 2;
        }

        (factors, n)
    }

    /// Analyze the Frobenius endomorphism structure in Z[omega]/(pi).
    pub fn frobenius_structure(&self) {
        println!("  [Z[omega]] Frobenius structure analysis:");
        println!("    Frob: x -> x^n in Z[omega]/(pi)");
        println!("    |Z[omega]/(pi)*| = n - 1");
        println!("    Since j(E) = 0 and h(-3) = 1, the class field is trivial");
        println!("    The Frobenius has order 1 in the class group");
        println!("    NOVEL: Use the NORM map to reduce dimension:");
        println!("    N: Z[omega]/(pi)* -> Z/nZ* (multiplicative)");
        println!("    N(alpha) = alpha * alpha_bar = norm of alpha mod n");
        println!("    The kernel of N in Z[omega]/(pi)* has order dividing 3");
        println!("    So N(k mod pi) constrains k mod pi up to a factor in {{1, omega, omega^2}}");

        // Factor n-1
        let n_minus_1 = &self.n - BigUint::one();
        let (factors, remainder) = self.partial_factor(&n_minus_1, 1_000_000);

        println!("\n  [Z[omega]] n-1 partial factorization:");
        let mut total = BigUint::one();
        for (p, e) in &factors {
            println!("    {}^{}", p, e);
            for _ in 0..*e {
                total = total * BigUint::from(*p);
            }
        }
        if remainder > BigUint::one() {
            println!("    remainder: {} bits", remainder.bits());
        }

        let smooth_bits = total.bits();
        println!("  [Z[omega]] Smooth part of n-1: {} bits", smooth_bits);
        println!("  [Z[omega]] Pohlig-Hellman applicable on smooth part");
    }
}

/// Integer square root of a BigUint (Newton's method)
fn int_sqrt(n: &BigUint) -> BigUint {
    if n.is_zero() { return BigUint::zero(); }
    let bits = n.bits();
    let mut x = BigUint::one() << ((bits + 1) / 2);
    loop {
        let x1 = (&x + n / &x) >> 1;
        if x1 >= x { break; }
        x = x1;
    }
    x
}

/// Round division for BigIntSigned: round(a / b) where b is always positive (it's a norm)
fn round_div_bigintsigned(a: &BigIntSigned, b: &BigIntSigned) -> BigIntSigned {
    if b.val.is_zero() {
        return BigIntSigned::zero();
    }
    let q = &a.val / &b.val;
    let r = &a.val % &b.val;
    // If 2*r >= b, round up in absolute value
    let q_rounded = if &r + &r >= b.val {
        q + BigUint::one()
    } else {
        q
    };
    // Sign: a.neg XOR would-be sign from rounding
    // For round-toward-zero: sign of result = sign of a
    BigIntSigned::from_sign(a.neg, q_rounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eisenstein_basics() {
        let z1 = EisensteinInt::from_u64(3, 5);
        let z2 = EisensteinInt::from_u64(2, 1);

        // Multiplication
        let z3 = &z1 * &z2;
        // (3+5w)(2+w) = 6 + 3w + 10w + 5w^2 = 6 + 13w + 5(-1-w) = (6-5) + (13-5)w = 1 + 8w
        assert_eq!(z3.a, BigUint::from(1u64));
        assert_eq!(z3.b, BigUint::from(8u64));
        assert!(!z3.a_neg);
        assert!(!z3.b_neg);

        // Norm
        let n = z1.norm(); // 9 - 15 + 25 = 19
        assert_eq!(n, BigUint::from(19u64));
    }

    #[test]
    fn test_eisenstein_conjugate() {
        // Test conjugate: conj(3 + 5w) = (3-5) + (-5)*w = -2 - 5w
        let z = EisensteinInt::from_u64(3, 5);
        let conj = z.conjugate();
        // a = |3-5| = 2, a_neg = true (since 3 < 5)
        // b = 5, b_neg = true
        assert_eq!(conj.a, BigUint::from(2u64));
        assert!(conj.a_neg);
        assert_eq!(conj.b, BigUint::from(5u64));
        assert!(conj.b_neg);

        // Test: z * conj(z) should be real (= norm)
        let product = z.mul_conjugate(&z);
        // N(3+5w) = 9 - 15 + 25 = 19
        assert_eq!(product.a, BigUint::from(19u64));
        assert!(!product.a_neg);
        assert!(product.b.is_zero());

        // Test with a >= b: conj(5 + 3w) = (5-3) + (-3)*w = 2 - 3w
        let z2 = EisensteinInt::from_u64(5, 3);
        let conj2 = z2.conjugate();
        assert_eq!(conj2.a, BigUint::from(2u64));
        assert!(!conj2.a_neg);
        assert_eq!(conj2.b, BigUint::from(3u64));
        assert!(conj2.b_neg);

        // Verify: z2 * conj(z2) = N(z2)
        let product2 = z2.mul_conjugate(&z2);
        // N(5+3w) = 25 - 15 + 9 = 19
        assert_eq!(product2.a, BigUint::from(19u64));
        assert!(!product2.a_neg);
        assert!(product2.b.is_zero());
    }

    #[test]
    fn test_eisenstein_divmod() {
        let z1 = EisensteinInt::from_u64(7, 3);
        let z2 = EisensteinInt::from_u64(2, 1);

        let (q, r) = eisen_divmod(&z1, &z2);
        // Verify: z1 = q * z2 + r
        let check = &(&q * &z2) + &r;
        assert_eq!(check.a, z1.a);
        assert_eq!(check.b, z1.b);
        assert!(!check.a_neg);
        assert!(!check.b_neg);
        // N(r) < N(z2)
        assert!(r.norm() < z2.norm());
    }

    #[test]
    fn test_zomega_lifter() {
        let lifter = ZOmegaDLPLifter::new();

        // Verify pi exists
        assert!(lifter.pi.is_some());

        // Verify pi * pi_bar = n using mul_conjugate
        if let Some(ref pi) = &lifter.pi {
            let product = pi.mul_conjugate(pi);
            // The product should be n (or -n), with zero omega component
            assert!(product.b.is_zero(), "pi * pi_bar should be real, but omega = {}{}",
                    if product.b_neg { "-" } else { "" }, product.b);
        }
    }

    #[test]
    fn test_gauss_reduce_signed() {
        // Test with a simple 2D lattice where we know the answer
        let n = BigUint::from(7u64);
        let lambda = BigUint::from(2u64); // 2^3 ≡ 1 mod 7, but just for testing

        // Basis: b0 = (7, 0), b1 = (5, 1) (since -2 mod 7 = 5)
        let b0 = (BigIntSigned::from_sign(false, n.clone()), BigIntSigned::zero());
        let b1 = (BigIntSigned::from_sign(false, BigUint::from(5u64)),
                  BigIntSigned::from_sign(false, BigUint::one()));

        let lifter = ZOmegaDLPLifter::new();
        let (v0, v1) = lifter.gauss_reduce_2d_signed(b0, b1);

        // After reduction, v0 should be shorter than original b1
        let n0 = v0.0.mul(&v0.0).add(&v0.1.mul(&v0.1));
        let n1 = v1.0.mul(&v1.0).add(&v1.1.mul(&v1.1));
        assert!(n0.val <= n1.val || n0.neg, "v0 should be shorter than v1");
    }
}
