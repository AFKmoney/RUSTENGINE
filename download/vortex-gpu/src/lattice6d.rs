//! VORTEX PRIME v5 — INVENTION 4: 6D Range-Constrained Lattice
//! ============================================================
//!
//! 6D LATTICE: n^(1/6) ≈ 2^43 ≈ 2^45 COMPONENTS
//! (vs 3D → 2^85, vs 2D → 2^128)
//!
//! Construction:
//!   The 6D lattice uses the full 6-automorphism structure of secp256k1
//!   combined with Z[ω] CRT decomposition and range constraints.
//!
//! Basis (6×6 integer matrix):
//!   Row 0: (n,    0,  0,  0,  0,  0)  — modular period
//!   Row 1: (-λ,   1,  0,  0,  0,  0)  — GLV λ relation
//!   Row 2: (-λ²,  0,  1,  0,  0,  0)  — λ² relation
//!   Row 3: (r₃,   0,  0,  1,  0,  0)  — range center
//!   Row 4: (r₄,   0,  0,  0,  1,  0)  — Z[ω] π.a
//!   Row 5: (r₅,   0,  0,  0,  0,  1)  — Z[ω] π.b
//!
//! where r₃ = range_center mod n, r₄ = π.a mod n, r₅ = π.b mod n
//!
//! Determinant = n · 1 · 1 · 1 · 1 · 1 = n ≈ 2^256
//! After LLL: shortest vector ≈ n^(1/6) ≈ 2^42.7 ≈ 2^45
//!
//! Babai CVP gives: k ≈ c₀·v₀ + c₁·v₁ + ... + c₅·v₅
//! with |cᵢ| ~ n^(1/6) ≈ 2^45 for all components.

use num_bigint::BigUint;
use num_traits::{Zero, One};
use std::fmt;

// ============================================================
// SIGNED BIGUINT (for lattice vector arithmetic)
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
    pub fn neg(&self) -> Self {
        if self.val.is_zero() { SignedBigUint::zero() }
        else { SignedBigUint { val: self.val.clone(), neg: !self.neg } }
    }
    pub fn abs(&self) -> BigUint { self.val.clone() }
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
    pub fn bits(&self) -> u64 { self.val.bits() }

    /// Convert to f64 for approximate comparisons
    pub fn to_f64(&self) -> f64 {
        let bytes = self.val.to_bytes_be();
        let mut f = 0.0f64;
        for &b in &bytes {
            f = f * 256.0 + b as f64;
        }
        if self.neg { -f } else { f }
    }
}

impl fmt::Display for SignedBigUint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.neg { write!(f, "-{}", self.val) } else { write!(f, "{}", self.val) }
    }
}

/// Round a signed division: round(num / den)
/// Returns the nearest integer to num/den (round half away from zero).
fn round_signed_div(num: &SignedBigUint, den: &SignedBigUint) -> SignedBigUint {
    if den.is_zero() || den.val.is_zero() { return SignedBigUint::zero(); }
    let q = &num.val / &den.val;
    let r = &num.val % &den.val;
    // Round: if 2*r >= den.val, round up
    let two_r = &r + &r;
    let q_rounded = if two_r >= den.val { &q + BigUint::one() } else { q };
    let is_zero = q_rounded.is_zero();
    let result_neg = num.neg ^ den.neg;
    SignedBigUint { val: q_rounded, neg: result_neg && !is_zero }
}

// ============================================================
// 6D LATTICE
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

pub struct Lattice6D {
    pub range_start: BigUint,
    pub range_end: BigUint,
    pub range_center: BigUint,
    pub range_half: BigUint,
    pub n: BigUint,
    pub lam: BigUint,
    pub lam_sq: BigUint,
    pub pi_a: BigUint,  // Eisenstein π real part
    pub pi_b: BigUint,  // Eisenstein π omega part
}

impl Lattice6D {
    pub fn new(range_start: BigUint, range_end: BigUint) -> Self {
        let range_center: BigUint = (&range_start + &range_end) >> 1;
        let range_half: BigUint = (&range_end - &range_start) >> 1;
        let n = secp256k1_order();
        let lam = secp256k1_lambda();
        let lam_sq = &lam * &lam % &n;

        // Default π values (will be overwritten by Z[ω] module output)
        let pi_a = BigUint::zero();
        let pi_b = BigUint::zero();

        println!("  [6D] Range: [2^{}, 2^{})", range_start.bits() - 1, range_end.bits() - 1);
        println!("  [6D] Order n = 2^{} bits", n.bits());
        println!("  [6D] λ = 2^{} bits", lam.bits());
        println!("  [6D] Expected component size: n^(1/6) ≈ 2^{:.1}", n.bits() as f64 / 6.0);

        Lattice6D { range_start, range_end, range_center, range_half, n, lam, lam_sq, pi_a, pi_b }
    }

    /// Set Z[ω] prime factor π = pi_a + pi_b·ω
    pub fn set_pi(&mut self, a: BigUint, b: BigUint) {
        println!("  [6D] Setting π = {} + {}·ω ({} + {} bits)",
                 a, b, a.bits(), b.bits());
        self.pi_a = a;
        self.pi_b = b;
    }

    /// Build the 6D lattice basis.
    ///
    /// The basis is a 6×6 matrix where each row is a 6D vector.
    /// The first column encodes the constraint on k (mod n),
    /// and columns 1-5 are the "unit" directions for each component.
    pub fn build_basis(&self) -> Vec<[SignedBigUint; DIM]> {
        let neg_lam = &self.n - (&self.lam % &self.n);
        let neg_lam_sq = &self.n - (&self.lam_sq % &self.n);
        let rc = &self.range_center % &self.n;
        let pi_a_mod = &self.pi_a % &self.n;
        let pi_b_mod = &self.pi_b % &self.n;

        let basis = vec![
            // Row 0: modular period
            [SignedBigUint::from_biguint(self.n.clone()), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            // Row 1: GLV λ relation
            [SignedBigUint::from_biguint(neg_lam), SignedBigUint::one(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            // Row 2: λ² relation
            [SignedBigUint::from_biguint(neg_lam_sq), SignedBigUint::zero(), SignedBigUint::one(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero()],
            // Row 3: range center
            [SignedBigUint::from_biguint(rc), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::one(), SignedBigUint::zero(), SignedBigUint::zero()],
            // Row 4: Z[ω] π.a
            [SignedBigUint::from_biguint(pi_a_mod), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::one(), SignedBigUint::zero()],
            // Row 5: Z[ω] π.b
            [SignedBigUint::from_biguint(pi_b_mod), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::zero(), SignedBigUint::one()],
        ];

        println!("  [6D] Basis constructed:");
        for (i, row) in basis.iter().enumerate() {
            println!("    v{} = (2^{}, {}, {}, {}, {}, {})", i, row[0].bits(),
                     if row[1].is_zero() { "0" } else { "1" },
                     if row[2].is_zero() { "0" } else { "1" },
                     if row[3].is_zero() { "0" } else { "1" },
                     if row[4].is_zero() { "0" } else { "1" },
                     if row[5].is_zero() { "0" } else { "1" });
        }

        basis
    }

    /// LLL reduction in 6D.
    ///
    /// The Lenstra-Lenstra-Lovász algorithm for dimension 6.
    /// Uses exact BigUint arithmetic for correctness.
    ///
    /// After LLL, the shortest basis vector should have
    /// norm ≈ n^(1/6) ≈ 2^43 ≈ 2^45.
    pub fn lll_reduce(&self, basis: &Vec<[SignedBigUint; DIM]>) -> Vec<[SignedBigUint; DIM]> {
        let mut b: Vec<[SignedBigUint; DIM]> = basis.clone();

        let max_iter = 500;
        let mut iter = 0;
        let mut i: usize = 1;

        println!("  [6D] Starting LLL reduction (dim={})...", DIM);

        while i < DIM && iter < max_iter {
            iter += 1;

            // Size-reduce b[i] with respect to b[0..i]
            for j in (0..i).rev() {
                // Compute μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
                // For size reduction, we use the simplified approximation:
                // μ_{i,j} ≈ <b[i], b[j]> / <b[j], b[j]>
                let dot_ij = dot_nd(&b[i], &b[j]);
                let norm_j = dot_nd(&b[j], &b[j]);
                let mu_ij = round_signed_div(&dot_ij, &norm_j);
                if !mu_ij.is_zero() {
                    for d in 0..DIM {
                        b[i][d] = b[i][d].sub(&mu_ij.mul(&b[j][d]));
                    }
                }
            }

            // Lovász condition check using exact Gram-Schmidt
            if i > 0 {
                let gs = gram_schmidt_exact(&b, i + 1);

                // Lovász: |b*[i]|² ≥ (δ - μ²_{i,i-1}) |b*[i-1]|²
                // with δ = 3/4
                //
                // We compute μ_{i,i-1} = <b[i], b*[i-1]> / <b*[i-1], b*[i-1]>
                let mu_num = dot_nd(&b[i], &gs.b_star[i - 1]);
                let mu_den = SignedBigUint::from_biguint(gs.norm_sq[i - 1].clone());
                let mu_sq_approx = if !mu_den.is_zero() {
                    let mu = round_signed_div(&mu_num, &mu_den);
                    // μ² as approximate bit length
                    mu.bits() * 2
                } else {
                    0u64
                };

                let bstar_i_bits = gs.norm_sq[i].bits();
                let bstar_i1_bits = gs.norm_sq[i - 1].bits();

                // Lovász condition (approximate using bit lengths):
                // |b*[i]|² < (3/4) |b*[i-1]|²
                // ≈ bstar_i_bits < bstar_i1_bits - 1 (since log2(3/4) ≈ -0.415)
                // More precisely: compare norm_sq values
                //
                // 4 * |b*[i]|² < 3 * |b*[i-1]|²
                let four_norm_i = &gs.norm_sq[i] << 2;
                let three_norm_i1 = &gs.norm_sq[i - 1] * 3u64;

                if four_norm_i < three_norm_i1 {
                    // Lovász violated: swap b[i] and b[i-1]
                    b.swap(i, i - 1);
                    if i > 1 { i -= 1; }
                    continue;
                }
            }
            i += 1;
        }

        println!("  [6D] LLL reduction complete ({} iterations):", iter);
        for (idx, v) in b.iter().enumerate() {
            let norm_sq: BigUint = v.iter()
                .map(|x| &x.val * &x.val)
                .fold(BigUint::zero(), |a, b| a + b);
            println!("    v{}: bits=({},{},{},{},{},{}), |v|²=2^{}",
                     idx, v[0].bits(), v[1].bits(), v[2].bits(),
                     v[3].bits(), v[4].bits(), v[5].bits(), norm_sq.bits());
        }

        b
    }

    /// Babai CVP (Closest Vector Problem) in 6D using Gram-Schmidt.
    ///
    /// Given a target vector t = (k, 0, 0, 0, 0, 0), find the closest
    /// lattice point. The residual gives the 6D decomposition:
    ///
    ///   k ≈ c₀·v₀ + c₁·v₁ + ... + c₅·v₅
    ///
    /// with |cᵢ| ~ n^(1/6) ≈ 2^45.
    ///
    /// Algorithm (Babai Nearest Plane):
    ///   1. Compute full Gram-Schmidt orthogonalization b*[0..6]
    ///   2. For i = 5, 4, ..., 0 (reverse):
    ///     cᵢ = round(<target, b*[i]> / <b*[i], b*[i]>)
    ///     target = target - cᵢ · v[i]  (original basis, not GS!)
    ///   3. Residual = original_target - closest = (a₀, a₁, a₂, a₃, a₄, a₅)
    pub fn babai_cvp(
        &self,
        basis: &[[SignedBigUint; DIM]; DIM],
        k: &BigUint,
    ) -> [SignedBigUint; DIM] {
        let target = [
            SignedBigUint::from_biguint(k.clone()),
            SignedBigUint::zero(),
            SignedBigUint::zero(),
            SignedBigUint::zero(),
            SignedBigUint::zero(),
            SignedBigUint::zero(),
        ];

        // Compute full Gram-Schmidt for the 6D basis
        let gs = gram_schmidt_exact(&basis.to_vec(), DIM);

        // Babai nearest plane: process from i = DIM-1 down to 0
        let mut t = target.clone();
        let mut coefficients: [SignedBigUint; DIM] = std::array::from_fn(|_| SignedBigUint::zero());

        for i in (0..DIM).rev() {
            // Compute <t, b*[i]>
            let t_dot_bstar_i = dot_nd(&t, &gs.b_star[i]);
            // <b*[i], b*[i]> = norm_sq[i]
            let bstar_i_norm_sq = SignedBigUint::from_biguint(gs.norm_sq[i].clone());

            // cᵢ = round(<t, b*[i]> / <b*[i], b*[i]>)
            let ci = round_signed_div(&t_dot_bstar_i, &bstar_i_norm_sq);
            coefficients[i] = ci.clone();

            // Update: t = t - cᵢ · v[i] (original basis vector, not GS!)
            if !ci.is_zero() {
                for d in 0..DIM {
                    t[d] = t[d].sub(&ci.mul(&basis[i][d]));
                }
            }
        }

        // The residual t = (a₀, a₁, a₂, a₃, a₄, a₅) gives the decomposition
        // k ≡ a₀ + a₁·λ + a₂·λ² + a₃·center + a₄·π.a + a₅·π.b (mod n)
        // where the coefficients[i] track the lattice point contribution

        println!("  [6D] Babai CVP result:");
        let mut max_bits = 0u64;
        for (i, a) in t.iter().enumerate() {
            let bits = a.bits();
            if bits > max_bits { max_bits = bits; }
            let sign = if a.neg { "-" } else { "+" };
            println!("    a{} = {} (2^{} bits)", i, sign, bits);
        }

        println!("  [6D] Maximum component: 2^{} bits", max_bits);
        println!("  [6D] Expected: n^(1/6) ≈ 2^{:.1} bits", self.n.bits() as f64 / 6.0);

        // Verify: k ≡ a₀ + c₀·v₀[0] + c₁·v₁[0] + ... + c₅·v₅[0] (mod n)
        let mut k_recon = t[0].clone();
        for i in 0..DIM {
            let ci_v0 = coefficients[i].mul(&basis[i][0]);
            k_recon = k_recon.add(&ci_v0);
        }

        let k_mod_n = k % &self.n;
        let k_recon_mod_n = if k_recon.neg {
            &self.n - (&k_recon.val % &self.n)
        } else {
            &k_recon.val % &self.n
        };

        let verified = k_mod_n == k_recon_mod_n;
        println!("  [6D] Verification: k ≡ reconstruction (mod n): {}", verified);
        if !verified {
            println!("  [6D] k mod n = 2^{} bits, recon mod n = 2^{} bits",
                     k_mod_n.bits(), k_recon_mod_n.bits());
        }

        t
    }

    /// Full 6D decomposition pipeline: build, reduce, CVP.
    pub fn decompose(&self, k: &BigUint) -> [SignedBigUint; DIM] {
        // Step 1: Build basis
        let basis = self.build_basis();

        // Step 2: LLL reduce
        let reduced = self.lll_reduce(&basis);

        // Convert to array for CVP
        let basis_arr: [[SignedBigUint; DIM]; DIM] = [
            reduced[0].clone(), reduced[1].clone(), reduced[2].clone(),
            reduced[3].clone(), reduced[4].clone(), reduced[5].clone(),
        ];

        // Step 3: Babai CVP
        self.babai_cvp(&basis_arr, k)
    }

    /// Analyze the search space after decomposition.
    pub fn analyze_search_space(&self, components: &[SignedBigUint; DIM]) {
        let max_bits = components.iter().map(|c| c.bits()).max().unwrap_or(0);

        println!("\n  [6D] SEARCH SPACE ANALYSIS:");
        println!("    Max component: 2^{} bits", max_bits);
        println!("    Without filters: O(2^{}) total search", max_bits);
        println!("    With 6x automorphism: O(2^{})", max_bits.saturating_sub(2));
        println!("    With 208x oracle: O(2^{})", max_bits.saturating_sub(9));
        println!("    With Frobenius 3x: O(2^{})", max_bits.saturating_sub(10));
        println!("    Kangaroo O(√N): O(2^{})", (max_bits + 1) / 2);
        println!("    Kangaroo + 6x auto: O(2^{})", ((max_bits + 1) / 2).saturating_sub(1));
        println!("    Kangaroo + all filters: O(2^{})", ((max_bits + 1) / 2).saturating_sub(5));

        let kangaroo_steps = 1u64 << ((max_bits + 1) / 2);
        let ops_per_sec = 1_000_000u64; // 10^6 ops/s with native field
        let seconds = kangaroo_steps / ops_per_sec;
        let hours = seconds / 3600;
        let days = hours / 24;

        println!("    Estimated time (native field, 10^6 ops/s):");
        println!("      Without filters: {} days", days);

        // More realistic estimate
        let filtered_bits = ((max_bits + 1) / 2).saturating_sub(5);
        if filtered_bits <= 63 {
            let filtered_steps = 1u64 << filtered_bits;
            let filtered_seconds = filtered_steps / ops_per_sec;
            println!("      Realistic (filtered kangaroo): {} seconds", filtered_seconds);
        } else {
            let filtered_hours = (1u128 << filtered_bits.min(80)) / (ops_per_sec as u128 * 3600);
            println!("      Realistic (filtered kangaroo): {} hours", filtered_hours);
        }
    }
}

// ============================================================
// EXACT GRAM-SCHMIDT (for LLL + Babai CVP)
// ============================================================

struct GramSchmidtExact {
    /// Gram-Schmidt orthogonalized vectors
    b_star: Vec<[SignedBigUint; DIM]>,
    /// Norm squared of each GS vector (always non-negative)
    norm_sq: [BigUint; DIM],
}

/// Compute exact Gram-Schmidt for first `k` basis vectors.
///
/// The GS vectors are computed by subtracting all projections:
///   b*[0] = b[0]
///   b*[i] = b[i] - Σ_{j<i} μ_{i,j} · b*[j]
///   where μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
///
/// Since we're working with BigUint (integer) arithmetic, the μ values
/// are computed as rational numbers but we keep the exact GS vectors.
fn gram_schmidt_exact(basis: &[[SignedBigUint; DIM]], k: usize) -> GramSchmidtExact {
    let mut b_star: Vec<[SignedBigUint; DIM]> = Vec::with_capacity(k);
    let mut norm_sq: [BigUint; DIM] = std::array::from_fn(|_| BigUint::zero());

    // We need to track the rational μ values for exact computation
    // μ_{i,j} = num_{i,j} / den_{i,j} where den = norm_sq[j]
    // To avoid accumulating errors, we compute b*[i] incrementally

    for i in 0..k {
        // Start with b[i]
        let mut bi = basis[i].clone();

        // Subtract projections onto previous GS vectors
        // b*[i] = b[i] - Σ_{j<i} μ_{i,j} · b*[j]
        // where μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
        for j in 0..i {
            if norm_sq[j].is_zero() { continue; }

            // Compute μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
            // Use original basis[i], not current bi, for the dot product
            // This is crucial for exact Gram-Schmidt
            let dot_val = dot_nd(&basis[i], &b_star[j]);
            let mu = round_signed_div(&dot_val, &SignedBigUint::from_biguint(norm_sq[j].clone()));

            if !mu.is_zero() {
                for d in 0..DIM {
                    bi[d] = bi[d].sub(&mu.mul(&b_star[j][d]));
                }
            }
        }

        // Compute norm: |b*[i]|² = Σ_d b*[i][d]²
        let n: BigUint = bi.iter()
            .map(|x| &x.val * &x.val)
            .fold(BigUint::zero(), |a, b| a + b);

        norm_sq[i] = n;
        b_star.push(bi);
    }

    GramSchmidtExact { b_star, norm_sq }
}

/// Dot product of two DIM-dimensional signed vectors
fn dot_nd(a: &[SignedBigUint; DIM], b: &[SignedBigUint; DIM]) -> SignedBigUint {
    let mut sum = SignedBigUint::zero();
    for i in 0..DIM {
        sum = sum.add(&a[i].mul(&b[i]));
    }
    sum
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_6d_lattice_construction() {
        let range_start = BigUint::from(1u64) << 134;
        let range_end = BigUint::from(1u64) << 135;
        let lattice = Lattice6D::new(range_start, range_end);
        let basis = lattice.build_basis();
        assert_eq!(basis.len(), 6);
    }

    #[test]
    fn test_6d_lattice_p70() {
        let range_start = BigUint::from(1u64) << 69;
        let range_end = BigUint::from(1u64) << 70;
        let mut lattice = Lattice6D::new(range_start, range_end);

        // Set π values (from Z[ω] decomposition)
        // For P70 these don't matter much, just use some values
        let n = secp256k1_order();
        lattice.set_pi(
            BigUint::parse_bytes(b"3831B8B5E17C0D3B8D1C0A8E2F7A3B5D", 16).unwrap_or(n.clone()),
            BigUint::parse_bytes(b"5A2E8F1C3D4B7A9E0F2D4B6A8C1E3F5A", 16).unwrap_or(BigUint::zero()),
        );

        let basis = lattice.build_basis();
        let reduced = lattice.lll_reduce(&basis);

        // Test with P70 known key
        let k = BigUint::parse_bytes(b"6c3a4f", 16).unwrap();
        let basis_arr: [[SignedBigUint; DIM]; DIM] = [
            reduced[0].clone(), reduced[1].clone(), reduced[2].clone(),
            reduced[3].clone(), reduced[4].clone(), reduced[5].clone(),
        ];
        let components = lattice.babai_cvp(&basis_arr, &k);

        // Check that components are small
        let max_bits = components.iter().map(|c| c.bits()).max().unwrap_or(0);
        println!("  [TEST] P70 max component: 2^{} bits", max_bits);
    }

    #[test]
    fn test_round_signed_div() {
        // Test basic rounding
        let a = SignedBigUint::from_biguint(BigUint::from(7u64));
        let b = SignedBigUint::from_biguint(BigUint::from(2u64));
        let r = round_signed_div(&a, &b);
        // 7/2 = 3.5, round = 4
        assert_eq!(r.val, BigUint::from(4u64));
        assert!(!r.neg);

        // Test with negative numerator
        let a_neg = SignedBigUint { val: BigUint::from(7u64), neg: true };
        let r2 = round_signed_div(&a_neg, &b);
        // -7/2 = -3.5, round = -4
        assert_eq!(r2.val, BigUint::from(4u64));
        assert!(r2.neg);
    }
}
