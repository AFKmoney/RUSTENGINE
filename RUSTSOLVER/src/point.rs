//! RUSTSOLVER — secp256k1 EC Point with Jacobian Coordinates
//! ============================================================

use crate::field::Fe;

pub const BETA: [u64; 4] = crate::field::BETA;

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

    pub fn generator() -> Self {
        Point {
            x: Fe::from_hex("79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798"),
            y: Fe::from_hex("483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8"),
            inf: false,
        }
    }

    pub fn neg(&self) -> Self {
        if self.inf { return *self; }
        Point { x: self.x, y: self.y.neg_mod_p(), inf: false }
    }

    #[inline]
    pub fn to_jacobian(&self) -> JacobianPoint {
        if self.inf {
            JacobianPoint { x: Fe::ONE, y: Fe::ONE, z: Fe::ZERO }
        } else {
            JacobianPoint { x: self.x, y: self.y, z: Fe::ONE }
        }
    }

    pub fn add(&self, other: &Point) -> Point {
        self.to_jacobian().add_affine(other).to_affine()
    }

    /// Scalar multiplication using double-and-add with Jacobian + mixed addition.
    pub fn scalar_mul(&self, k: &Fe) -> Point {
        if k.is_zero() || self.inf { return Point::infinity(); }
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

    /// GLV endomorphism: phi(P) = (beta*x, y)
    pub fn glv_phi(&self) -> Self {
        if self.inf { return *self; }
        let beta = Fe { limbs: BETA };
        Point { x: self.x.mul(&beta), y: self.y, inf: false }
    }

    pub fn is_on_curve(&self) -> bool {
        if self.inf { return true; }
        let y_sq = self.y.mul(&self.y);
        let x_cu = self.x.mul(&self.x).mul(&self.x);
        let rhs = x_cu.add(&Fe::from_u64(7));
        y_sq == rhs
    }
}

// ============================================================
// JACOBIAN POINT
// ============================================================

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

    #[inline]
    pub fn is_infinity(&self) -> bool {
        self.z.is_zero()
    }

    pub fn to_affine(&self) -> Point {
        if self.z.is_zero() { return Point::infinity(); }
        let z_inv = self.z.modinv();
        let z_inv_sq = z_inv.mul(&z_inv);
        let z_inv_cu = z_inv_sq.mul(&z_inv);
        Point {
            x: self.x.mul(&z_inv_sq),
            y: self.y.mul(&z_inv_cu),
            inf: false,
        }
    }

    /// Point doubling in Jacobian (a=0 curve).
    /// Cost: 4M + 4S
    pub fn double(&self) -> Self {
        if self.z.is_zero() || self.y.is_zero() { return Self::infinity(); }

        let a = self.y.sqr();
        let b = self.x.mul(&a);
        let b4 = b.add(&b).add(&b).add(&b); // 4*X*Y^2
        let d = self.x.sqr().add(&self.x.sqr()).add(&self.x.sqr()); // 3*X^2 (a=0)
        let c = a.sqr();
        let c8 = c.add(&c).add(&c).add(&c).add(&c).add(&c).add(&c).add(&c); // 8*Y^4

        let x3 = d.sqr().sub(&b4).sub(&b4);
        let y3 = d.mul(&b4.sub(&x3)).sub(&c8);
        let z3 = self.y.add(&self.y).mul(&self.z);

        JacobianPoint { x: x3, y: y3, z: z3 }
    }

    /// Mixed addition: Jacobian + Affine. Cost: 8M + 3S
    /// This is the HOT PATH in the kangaroo solver.
    #[inline]
    pub fn add_affine(&self, other: &Point) -> Self {
        if self.z.is_zero() { return other.to_jacobian(); }
        if other.inf { return *self; }

        let z1_sq = self.z.sqr();
        let u2 = other.x.mul(&z1_sq);
        let z1_cu = z1_sq.mul(&self.z);
        let s2 = other.y.mul(&z1_cu);

        if self.x == u2 {
            if self.y == s2 { return self.double(); }
            return Self::infinity();
        }

        let h = u2.sub(&self.x);
        let r = s2.sub(&self.y);
        let h_sq = h.sqr();
        let h_cu = h_sq.mul(&h);

        let x3 = r.sqr().sub(&h_cu).sub(&self.x.add(&self.x).mul(&h_sq));
        let y3 = r.mul(&self.x.mul(&h_sq).sub(&x3)).sub(&self.y.mul(&h_cu));
        let z3 = h.mul(&self.z);

        JacobianPoint { x: x3, y: y3, z: z3 }
    }

    pub fn neg(&self) -> Self {
        if self.z.is_zero() { return *self; }
        JacobianPoint { x: self.x, y: self.y.neg_mod_p(), z: self.z }
    }
}
