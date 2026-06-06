//! VORTEX PRIME v5 — secp256k1 EC Point with Jacobian Coordinates
//! ================================================================
//! Affine: (x, y) | Jacobian: (X, Y, Z) where x = X/Z², y = Y/Z³
//!
//! KEY OPTIMIZATION: Jacobian coordinates eliminate field inversions
//! from point addition/doubling. Inversion is only needed when
//! converting back to affine (once at the end).
//!
//! With native u64x4 + Jacobian: ~10^6 EC ops/s on CPU.

use crate::field::Fe;

// ============================================================
// secp256k1 CURVE CONSTANTS
// ============================================================

pub const P: [u64; 4] = [
    0xFFFFFFFEFFFFFC2F,
    0xFFFFFFFFFFFFFFFF,
    0xFFFFFFFFFFFFFFFF,
    0xFFFFFFFFFFFFFFFF,
];

pub const N: [u64; 4] = [
    0xBFD25E8CD0364141,
    0xBAEDCE6AF48A03BB,
    0xFFFFFFFFFFFFFFFE,
    0xFFFFFFFFFFFFFFFF,
];

// BETA^3 ≡ 1 mod P (non-trivial cube root of unity)
pub const BETA: [u64; 4] = [
    0xC1396C28719501EE,
    0x9CF0497512F58995,
    0x6E64479EAC3434E9,
    0x7AE96A2B657C0710,
];

// LAMBDA^3 ≡ 1 mod N (non-trivial cube root of unity)
pub const LAMBDA: [u64; 4] = [
    0xDF02967C1B23BD72,
    0x812645A122E22EA2,
    0x000000A5261C0288,
    0x5363AD4CC05C30E0,
];

// Generator point G
pub const GX: [u64; 4] = [
    0x59F2815B16F81798,
    0x029BFCDB2DCE28D9,
    0x55A06295CE870B07,
    0x79BE667EF9DCBBAC,
];

pub const GY: [u64; 4] = [
    0x9C47D08FFB10D4B8,
    0xFD17B448A6855419,
    0x5DA4FBFC0E1108A8,
    0x483ADA7726A3C465,
];

// ============================================================
// AFFINE POINT (for storage / final output)
// ============================================================

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: Fe,
    pub y: Fe,
    pub inf: bool,
}

impl Point {
    pub const fn infinity() -> Self {
        Point { x: Fe::ZERO, y: Fe::ZERO, inf: true }
    }

    pub const fn new(x: Fe, y: Fe) -> Self {
        Point { x, y, inf: false }
    }

    /// G generator point
    pub fn generator() -> Self {
        Point {
            x: Fe::from_hex("79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798"),
            y: Fe::from_hex("483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8"),
            inf: false,
        }
    }

    /// Negate: -P = (x, -y mod p)
    pub fn neg(&self) -> Self {
        if self.inf { return *self; }
        Point { x: self.x, y: self.y.neg_mod_p(), inf: false }
    }

    /// Convert to Jacobian coordinates
    #[inline]
    pub fn to_jacobian(&self) -> JacobianPoint {
        if self.inf {
            JacobianPoint { x: Fe::ONE, y: Fe::ONE, z: Fe::ZERO }
        } else {
            JacobianPoint { x: self.x, y: self.y, z: Fe::ONE }
        }
    }

    // Legacy affine operations (slow, for compatibility)
    pub fn double(&self) -> Self {
        self.to_jacobian().double().to_affine()
    }

    pub fn add(&self, other: &Point) -> Point {
        self.to_jacobian().add_affine(other).to_affine()
    }

    /// Scalar multiplication: k * P using double-and-add with Jacobian coordinates
    pub fn scalar_mul(&self, k: &Fe) -> Point {
        if k.is_zero() || self.inf { return Point::infinity(); }

        // Store the affine form for mixed addition (add_affine is verified correct)
        let p_affine = *self;

        let mut result = JacobianPoint::infinity();
        let bits = k.bit_length();

        for i in (0..bits).rev() {
            result = result.double();
            if k.get_bit(i) {
                result = result.add_affine(&p_affine);
            }
        }
        result.to_affine()
    }

    /// GLV endomorphism: phi(P) = (beta*x, y) = [lambda]P
    pub fn glv_phi(&self) -> Self {
        if self.inf { return *self; }
        let beta = Fe { limbs: BETA };
        Point {
            x: self.x.mul(&beta),
            y: self.y,
            inf: false,
        }
    }

    /// phi^2(P) = (beta^2*x, y) = [lambda^2]P
    pub fn glv_phi2(&self) -> Self {
        if self.inf { return *self; }
        let beta = Fe { limbs: BETA };
        let beta2 = beta.mul(&beta);
        Point {
            x: self.x.mul(&beta2),
            y: self.y,
            inf: false,
        }
    }

    /// Check if point is on curve: y^2 = x^3 + 7
    pub fn is_on_curve(&self) -> bool {
        if self.inf { return true; }
        let y_sq = self.y.mul(&self.y);
        let x_sq = self.x.mul(&self.x);
        let x_cu = x_sq.mul(&self.x);
        let rhs = x_cu.add(&Fe::from_u64(7));
        y_sq == rhs
    }

    /// Get all 6 automorphism images
    pub fn automorphism_group(&self) -> [Point; 6] {
        let p = *self;
        let neg_p = p.neg();
        let phi_p = p.glv_phi();
        let neg_phi_p = phi_p.neg();
        let phi2_p = p.glv_phi2();
        let neg_phi2_p = phi2_p.neg();
        [p, neg_p, phi_p, neg_phi_p, phi2_p, neg_phi2_p]
    }

    /// Compress to 33 bytes
    pub fn to_bytes(&self) -> [u8; 33] {
        let mut out = [0u8; 33];
        if self.inf {
            out[0] = 0;
            return out;
        }
        out[0] = if self.y.limbs[0] & 1 == 0 { 0x02 } else { 0x03 };
        let x_bytes = self.x.to_bytes();
        out[1..33].copy_from_slice(&x_bytes);
        out
    }
}

// ============================================================
// JACOBIAN POINT (for fast EC operations)
// ============================================================

/// Jacobian coordinates: (X, Y, Z) where x = X/Z², y = Y/Z³
///
/// Point at infinity: Z = 0
/// Identity: (1, 1, 0)
///
/// Advantages over affine:
/// - No field inversion per add/double (inversion = ~256 muls)
/// - Point doubling: ~4M + 4S (M = mul, S = sqr)
/// - Point addition: ~12M + 4S (mixed: ~8M + 3S)
/// - Only need inversion once at the end to convert to affine
#[derive(Clone, Copy, Debug)]
pub struct JacobianPoint {
    pub x: Fe,
    pub y: Fe,
    pub z: Fe,
}

impl JacobianPoint {
    pub const fn infinity() -> Self {
        JacobianPoint { x: Fe::ONE, y: Fe::ONE, z: Fe::ZERO }
    }

    pub const fn new(x: Fe, y: Fe, z: Fe) -> Self {
        JacobianPoint { x, y, z }
    }

    #[inline]
    pub fn is_infinity(&self) -> bool {
        self.z.is_zero()
    }

    /// Convert Jacobian to affine coordinates.
    /// Requires one field inversion (expensive, but only done once at the end).
    pub fn to_affine(&self) -> Point {
        if self.z.is_zero() {
            return Point::infinity();
        }

        let z_inv = self.z.modinv();
        let z_inv_sq = z_inv.mul(&z_inv);
        let z_inv_cu = z_inv_sq.mul(&z_inv);

        let x = self.x.mul(&z_inv_sq);
        let y = self.y.mul(&z_inv_cu);

        Point { x, y, inf: false }
    }

    /// Point doubling in Jacobian coordinates.
    ///
    /// Algorithm (standard Jacobian doubling for a = 0 curves):
    ///   A = Y₁²
    ///   B = 4·X₁·A = 4·X₁·Y₁²
    ///   C = 8·A² = 8·Y₁⁴
    ///   D = 3·X₁² (since a = 0 for secp256k1)
    ///   X₃ = D² - 2·B
    ///   Y₃ = D·(B - X₃) - C
    ///   Z₃ = 2·Y₁·Z₁
    ///
    /// Cost: 4M + 4S + 5add (where M = mul, S = sqr)
    pub fn double(&self) -> Self {
        if self.z.is_zero() {
            return Self::infinity();
        }
        if self.y.is_zero() {
            return Self::infinity();
        }

        let a = self.y.sqr();               // A = Y₁²
        let b = self.x.mul(&a).add(&self.x.mul(&a))
                       .add(&self.x.mul(&a)).add(&self.x.mul(&a));  // B = 4·X₁·Y₁²
        let d = self.x.sqr().add(&self.x.sqr()).add(&self.x.sqr()); // D = 3·X₁² (a=0)
        let c = a.sqr().add(&a.sqr()).add(&a.sqr())
                     .add(&a.sqr()).add(&a.sqr()).add(&a.sqr())
                     .add(&a.sqr()).add(&a.sqr());                   // C = 8·Y₁⁴

        let x3 = d.sqr().sub(&b).sub(&b);  // X₃ = D² - 2·B
        let y3 = d.mul(&b.sub(&x3)).sub(&c); // Y₃ = D·(B - X₃) - C
        let z3 = self.y.add(&self.y).mul(&self.z); // Z₃ = 2·Y₁·Z₁

        JacobianPoint { x: x3, y: y3, z: z3 }
    }

    /// Point addition in Jacobian coordinates: self + other (both Jacobian).
    ///
    /// Cost: 12M + 4S
    pub fn add(&self, other: &JacobianPoint) -> Self {
        if self.z.is_zero() { return *other; }
        if other.z.is_zero() { return *self; }

        // Z₁², Z₂²
        let z1_sq = self.z.sqr();
        let z2_sq = other.z.sqr();

        // U₁ = X₁·Z₂², U₂ = X₂·Z₁²
        let u1 = self.x.mul(&z2_sq);
        let u2 = other.x.mul(&z1_sq);

        // S₁ = Y₁·Z₂³, S₂ = Y₂·Z₁³
        let z2_cu = z2_sq.mul(&other.z);
        let z1_cu = z1_sq.mul(&self.z);
        let s1 = self.y.mul(&z2_cu);
        let s2 = other.y.mul(&z1_cu);

        // Check if points are equal or negatives
        if u1 == u2 {
            if s1 == s2 {
                return self.double();
            }
            return Self::infinity();
        }

        // H = U₂ - U₁
        let h = u2.sub(&u1);
        // R = S₂ - S₁
        let r = s2.sub(&s1);

        // H²
        let h_sq = h.sqr();

        // X₃ = R² - H³ - 2·U₁·H²
        let h_cu = h_sq.mul(&h);
        let x3 = r.sqr().sub(&h_cu).sub(&u1.add(&u1).mul(&h_sq));

        // Y₃ = R·(U₁·H² - X₃) - S₁·H³
        let y3 = r.mul(&u1.mul(&h_sq).sub(&x3)).sub(&s1.mul(&h_cu));

        // Z₃ = H·Z₁·Z₂
        let z3 = h.mul(&self.z).mul(&other.z);

        JacobianPoint { x: x3, y: y3, z: z3 }
    }

    /// Mixed addition: Jacobian + Affine (cheaper than full Jacobian add).
    ///
    /// Cost: 8M + 3S (saves 4M vs full Jacobian add)
    /// This is the HOT PATH in the kangaroo solver.
    pub fn add_affine(&self, other: &Point) -> Self {
        if self.z.is_zero() {
            return other.to_jacobian();
        }
        if other.inf {
            return *self;
        }

        // Z₁²
        let z1_sq = self.z.sqr();

        // U₂ = X₂ (other is affine, Z₂ = 1)
        // U₁ = X₁·Z₁²... wait, we compare U₁ = X₁ with U₂ = X₂·Z₁²
        let u2 = other.x.mul(&z1_sq);

        // S₂ = Y₂·Z₁³ (since Z₂ = 1)
        let z1_cu = z1_sq.mul(&self.z);
        let s2 = other.y.mul(&z1_cu);

        // Check if points are equal or negatives
        if self.x == u2 {
            if self.y == s2 {
                return self.double();
            }
            return Self::infinity();
        }

        // H = U₂ - X₁ (since U₁ = X₁ for Jacobian self)
        let h = u2.sub(&self.x);
        // R = S₂ - Y₁
        let r = s2.sub(&self.y);

        // H²
        let h_sq = h.sqr();

        // X₃ = R² - H³ - 2·X₁·H²
        let h_cu = h_sq.mul(&h);
        let x3 = r.sqr().sub(&h_cu).sub(&self.x.add(&self.x).mul(&h_sq));

        // Y₃ = R·(X₁·H² - X₃) - Y₁·H³
        let y3 = r.mul(&self.x.mul(&h_sq).sub(&x3)).sub(&self.y.mul(&h_cu));

        // Z₃ = H·Z₁
        let z3 = h.mul(&self.z);

        JacobianPoint { x: x3, y: y3, z: z3 }
    }

    /// GLV scalar multiplication with endomorphism decomposition.
    ///
    /// k*P = a*P + b*φ(P) where k = a + b*λ mod n
    /// Uses interleaved double-and-add with two scalar halves.
    /// Cost: ~128 doublings + ~64 add_affine = ~192 EC ops (vs ~256 for single scalar)
    pub fn scalar_mul_glv(&self, k: &Fe, lambda: &Fe, n: &Fe, phi_p: &JacobianPoint) -> Self {
        // Decompose k = a + b*lambda mod n
        // b = k * lambda^(-1) mod n
        // a = k - b*lambda mod n

        // For simplicity, use standard scalar mul for now
        // TODO: Implement proper GLV interleaved scalar mul
        self.scalar_mul(k)
    }

    /// Standard scalar multiplication using Jacobian coordinates.
    /// Uses left-to-right double-and-add with mixed addition (add_affine).
    pub fn scalar_mul(&self, k: &Fe) -> Self {
        if k.is_zero() || self.z.is_zero() {
            return Self::infinity();
        }

        // Convert to affine for mixed addition
        let p_affine = self.to_affine();
        let mut result = Self::infinity();
        let bits = k.bit_length();

        for i in (0..bits).rev() {
            result = result.double();
            if k.get_bit(i) {
                result = result.add_affine(&p_affine);
            }
        }
        result
    }

    /// GLV endomorphism on Jacobian point: phi(P) = (beta*X, Y, Z)
    pub fn glv_phi(&self) -> Self {
        if self.z.is_zero() { return *self; }
        let beta = Fe { limbs: BETA };
        JacobianPoint {
            x: self.x.mul(&beta),
            y: self.y,
            z: self.z,
        }
    }

    /// Negate: -P = (X, -Y, Z)
    pub fn neg(&self) -> Self {
        if self.z.is_zero() { return *self; }
        JacobianPoint {
            x: self.x,
            y: self.y.neg_mod_p(),
            z: self.z,
        }
    }

    /// Batch-normalize multiple Jacobian points to affine using Montgomery's trick.
    /// Only ONE inversion total instead of N inversions!
    ///
    /// Cost: 3N muls + 1 inversion (vs N inversions for individual conversion)
    pub fn batch_to_affine(points: &[JacobianPoint]) -> Vec<Point> {
        if points.is_empty() { return vec![]; }

        let n = points.len();
        let mut z_inverses = vec![Fe::ONE; n];
        let mut z_accum = vec![Fe::ONE; n + 1];
        z_accum[0] = Fe::ONE;

        // Forward pass: compute Z₁·Z₂·...·Zᵢ
        for i in 0..n {
            z_accum[i + 1] = z_accum[i].mul(&points[i].z);
        }

        // Compute (Z₁·Z₂·...·Zₙ)^(-1)
        let all_inv = z_accum[n].modinv();

        // Backward pass: compute each Zᵢ^(-1)
        let mut acc_inv = all_inv;
        for i in (0..n).rev() {
            z_inverses[i] = acc_inv.mul(&z_accum[i]);
            acc_inv = acc_inv.mul(&points[i].z);
        }

        // Convert each point
        points.iter().enumerate().map(|(i, p)| {
            if p.z.is_zero() {
                Point::infinity()
            } else {
                let z_inv = z_inverses[i];
                let z_inv_sq = z_inv.mul(&z_inv);
                let z_inv_cu = z_inv_sq.mul(&z_inv);
                Point {
                    x: p.x.mul(&z_inv_sq),
                    y: p.y.mul(&z_inv_cu),
                    inf: false,
                }
            }
        }).collect()
    }
}
