//! VORTEX PRIME v5 — GLV Decomposition & Automorphism Group
//! 6 automorphisms + 3 endomorphisms for secp256k1

use crate::field::Fe;
use crate::point::Point;
use std::cmp::Ordering;

/// secp256k1 order
pub const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// GLV lambda: lambda^3 ≡ 1 mod n (non-trivial cube root)
pub const LAMBDA_HEX: &str = "5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72";

/// 6 automorphism multipliers for secp256k1
pub struct GLVDecomposer {
    pub n: Fe,
    pub lambda: Fe,
    pub lambda_sq: Fe,
    pub lambda_inv: Fe,
    pub g: Point,
    pub phi_g: Point,
    pub phi2_g: Point,
}

impl GLVDecomposer {
    pub fn new() -> Self {
        let n = Fe::from_hex(ORDER_HEX);
        let lambda = Fe::from_hex(LAMBDA_HEX);
        let lambda_sq = lambda.mul(&lambda);
        let lambda_inv = lambda.modinv();

        let g = Point::generator();
        let phi_g = g.glv_phi();
        let phi2_g = g.glv_phi2();

        GLVDecomposer { n, lambda, lambda_sq, lambda_inv, g, phi_g, phi2_g }
    }

    /// 2-way GLV decomposition: k = a + b*lambda mod n
    /// NOTE: This gives |a|, |b| ~ sqrt(n) ~ 2^128
    /// For smaller components, use the 6D lattice instead.
    pub fn decompose_2way(&self, k: &Fe) -> (Fe, Fe) {
        let b = k.mul(&self.lambda_inv);
        let a = k.sub_mod_n(&b.mul(&self.lambda));
        (a, b)
    }

    /// Get the 6 automorphism multipliers
    pub fn automorphism_scalars(&self, k: &Fe) -> [Fe; 6] {
        let neg_k = k.neg_mod_n();
        let lam_k = k.mul(&self.lambda);
        let neg_lam_k = lam_k.neg_mod_n();
        let lam2_k = k.mul(&self.lambda_sq);
        let neg_lam2_k = lam2_k.neg_mod_n();
        [*k, neg_k, lam_k, neg_lam_k, lam2_k, neg_lam2_k]
    }

    /// Get the 6 automorphism points of Q
    pub fn automorphism_points(&self, q: &Point) -> [Point; 6] {
        q.automorphism_group()
    }

    /// Check if any automorphism image of k is in range
    pub fn any_auto_in_range(&self, k: &Fe, range_start: &Fe, range_end: &Fe) -> bool {
        let scalars = self.automorphism_scalars(k);
        for s in &scalars {
            match s.cmp_val(&range_start.limbs) {
                Ordering::Less => continue,
                Ordering::Equal | Ordering::Greater => {
                    if s.cmp_val(&range_end.limbs) == Ordering::Less {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Compute k*G using GLV decomposition
    pub fn scalar_mul_glv(&self, k: &Fe) -> Point {
        let (a, b) = self.decompose_2way(k);
        let pa = self.g.scalar_mul(&a);
        let pb = self.phi_g.scalar_mul(&b);
        pa.add(&pb)
    }
}
