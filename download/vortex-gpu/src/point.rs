//! VORTEX PRIME v4 — secp256k1 Field & Point Arithmetic
//! Pure Rust, zero dependencies, u64x4 limb representation.
//! Optimized for GPU kernel porting.

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
// Full: 0x7AE96A2B657C07106E64479EAC3434E99CF0497512F58995C1396C28719501EE
pub const BETA: [u64; 4] = [
    0x7AE96A2B657C0710,
    0x6E64479EAC3434E9,
    0x9CF0497512F58995,
    0xC1396C28719501EE,
];

// LAMBDA^3 ≡ 1 mod N (non-trivial cube root of unity)
// Full: 0x5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72
pub const LAMBDA: [u64; 4] = [
    0x5363AD4CC05C30E0,
    0xA5261C0288,
    0x812645A122E22EA2,
    0xDF02967C1B23BD72,
];

// Generator point G
// GX: 79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
pub const GX: [u64; 4] = [
    0x79BE667EF9DCBBAC,  // MSB
    0x55A06295CE870B07,
    0x029BFCDB2DCE28D9,
    0x59F2815B16F81798,  // LSB
];

// GY: 483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8
pub const GY: [u64; 4] = [
    0x483ADA7726A3C465,  // MSB
    0x5DA4FBFC0E1108A8,
    0xFD17B448A6855419,
    0x9C47D08FFB10D4B8,  // LSB
];

// Point at infinity represented as (0, 0) with a flag
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

    /// Point doubling: 2P
    pub fn double(&self) -> Self {
        if self.inf { return *self; }
        if self.y.is_zero() { return Point::infinity(); }

        // s = 3*x^2 / (2*y) mod p
        let x_sq = self.x.mul(&self.x);
        let three_x_sq = x_sq.add(&x_sq).add(&x_sq);
        let two_y = self.y.add(&self.y);
        let two_y_inv = two_y.modinv();
        let s = three_x_sq.mul(&two_y_inv);

        // x3 = s^2 - 2*x
        let s_sq = s.mul(&s);
        let two_x = self.x.add(&self.x);
        let x3 = s_sq.sub(&two_x);

        // y3 = s*(x - x3) - y
        let dx = self.x.sub(&x3);
        let y3 = s.mul(&dx).sub(&self.y);

        Point { x: x3, y: y3, inf: false }
    }

    /// Point addition: P + Q
    pub fn add(&self, other: &Point) -> Point {
        if self.inf { return *other; }
        if other.inf { return *self; }

        if self.x == other.x {
            if self.y != other.y { return Point::infinity(); }
            return self.double();
        }

        // s = (y2 - y1) / (x2 - x1)
        let dy = other.y.sub(&self.y);
        let dx = other.x.sub(&self.x);
        let dx_inv = dx.modinv();
        let s = dy.mul(&dx_inv);

        // x3 = s^2 - x1 - x2
        let s_sq = s.mul(&s);
        let x3 = s_sq.sub(&self.x).sub(&other.x);

        // y3 = s*(x1 - x3) - y1
        let y3 = s.mul(&self.x.sub(&x3)).sub(&self.y);

        Point { x: x3, y: y3, inf: false }
    }

    /// Scalar multiplication: k * P using double-and-add
    pub fn scalar_mul(&self, k: &Fe) -> Point {
        if k.is_zero() || self.inf { return Point::infinity(); }

        let mut result = Point::infinity();
        let mut addend = *self;
        let mut k_val = *k;

        while !k_val.is_zero() {
            if k_val.limbs[0] & 1 == 1 {
                result = result.add(&addend);
            }
            addend = addend.double();
            k_val = k_val.shr1();
        }
        result
    }

    /// GLV endomorphism: phi(P) = (beta*x, y) = [lambda]P
    pub fn glv_phi(&self) -> Self {
        if self.inf { return *self; }
        let beta = Fe::from_hex("7AE96A2B657C07106E64479EAC3434E99CF0497512F58995C1396C28719501EE");
        Point {
            x: self.x.mul(&beta),
            y: self.y,
            inf: false,
        }
    }

    /// phi^2(P) = (beta^2*x, y) = [lambda^2]P
    pub fn glv_phi2(&self) -> Self {
        if self.inf { return *self; }
        let beta = Fe::from_hex("7AE96A2B657C07106E64479EAC3434E99CF0497512F58995C1396C28719501EE");
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

    /// Compress to 33 bytes (prefix + x-coordinate)
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
