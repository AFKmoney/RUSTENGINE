//! 256-bit modular arithmetic for secp256k1
//! Uses BigUint internally for CORRECT results.
//! Performance: ~10x slower than native u64x4, but mathematically correct.

use std::cmp::Ordering;
use num_bigint::BigUint;
use num_traits::{Zero, One};

// secp256k1 prime
const P_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F";
// secp256k1 order
const N_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fe {
    pub limbs: [u64; 4],
}

impl Fe {
    pub const ZERO: Fe = Fe { limbs: [0, 0, 0, 0] };
    pub const ONE: Fe = Fe { limbs: [1, 0, 0, 0] };

    #[inline]
    pub const fn from_u64(v: u64) -> Self { Fe { limbs: [v, 0, 0, 0] } }
    #[inline]
    pub const fn from_u64_limbs(l: [u64; 4]) -> Self { Fe { limbs: l } }

    /// Get the prime P as BigUint
    fn p_big() -> BigUint {
        BigUint::parse_bytes(P_HEX.as_bytes(), 16).unwrap()
    }

    /// Get the order N as BigUint
    fn n_big() -> BigUint {
        BigUint::parse_bytes(N_HEX.as_bytes(), 16).unwrap()
    }

    /// Convert Fe to BigUint
    pub fn to_biguint(&self) -> BigUint {
        let bytes = self.to_bytes();
        BigUint::from_bytes_be(&bytes)
    }

    /// Convert BigUint to Fe (mod P)
    fn from_biguint_mod(v: &BigUint) -> Self {
        let p = Self::p_big();
        let reduced = v % &p;
        let bytes = reduced.to_bytes_be();
        let mut arr = [0u8; 32];
        let start = 32 - bytes.len().min(32);
        arr[start..32].copy_from_slice(&bytes[..bytes.len().min(32)]);
        Self::from_bytes(&arr)
    }

    /// Convert BigUint to Fe (no mod, assumes v < P)
    fn from_biguint_raw(v: &BigUint) -> Self {
        let bytes = v.to_bytes_be();
        let mut arr = [0u8; 32];
        let start = 32 - bytes.len().min(32);
        arr[start..32].copy_from_slice(&bytes[..bytes.len().min(32)]);
        Self::from_bytes(&arr)
    }

    pub fn from_bytes(b: &[u8; 32]) -> Self {
        let mut l = [0u64; 4];
        for i in 0..4 {
            let s = (3 - i) * 8;
            l[i] = u64::from_be_bytes([b[s],b[s+1],b[s+2],b[s+3],b[s+4],b[s+5],b[s+6],b[s+7]]);
        }
        Fe { limbs: l }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        let mut o = [0u8; 32];
        for i in 0..4 {
            let s = (3 - i) * 8;
            o[s..s+8].copy_from_slice(&self.limbs[i].to_be_bytes());
        }
        o
    }

    pub fn from_hex(s: &str) -> Self {
        let s = s.trim_start_matches("0x");
        let bytes = hex::decode(s).expect("Invalid hex");
        let mut arr = [0u8; 32];
        let start = 32 - bytes.len().min(32);
        arr[start..32].copy_from_slice(&bytes[..bytes.len().min(32)]);
        // Convert to BigUint then mod P
        let big = BigUint::from_bytes_be(&arr);
        Self::from_biguint_mod(&big)
    }

    #[inline] pub fn is_zero(&self) -> bool { self.limbs.iter().all(|&x| x == 0) }

    pub fn cmp_val(&self, other: &[u64; 4]) -> Ordering {
        for i in (0..4).rev() {
            if self.limbs[i] < other[i] { return Ordering::Less; }
            if self.limbs[i] > other[i] { return Ordering::Greater; }
        }
        Ordering::Equal
    }

    /// Modular addition: (self + other) mod P
    pub fn add(&self, o: &Fe) -> Self {
        let a = self.to_biguint();
        let b = o.to_biguint();
        let p = Self::p_big();
        let sum = (&a + &b) % &p;
        Self::from_biguint_mod(&sum)
    }

    /// Modular subtraction: (self - other) mod P
    pub fn sub(&self, o: &Fe) -> Self {
        let a = self.to_biguint();
        let b = o.to_biguint();
        let p = Self::p_big();
        let mut result = &a + &p - &b;
        if result >= p {
            result = &result - &p;
        }
        Self::from_biguint_raw(&result)
    }

    /// Modular negation: (-self) mod P
    pub fn neg_mod_p(&self) -> Self {
        if self.is_zero() { return *self; }
        let a = self.to_biguint();
        let p = Self::p_big();
        Self::from_biguint_raw(&(&p - &a))
    }

    /// Modular multiplication: (self * other) mod P
    pub fn mul(&self, o: &Fe) -> Self {
        let a = self.to_biguint();
        let b = o.to_biguint();
        Self::from_biguint_mod(&(&a * &b))
    }

    /// Modular inverse: self^(-1) mod P via Fermat's little theorem
    pub fn modinv(&self) -> Self {
        if self.is_zero() { panic!("modinv of zero"); }
        // a^(-1) = a^(P-2) mod P
        let p = Self::p_big();
        let exp = &p - BigUint::from(2u64);
        self.pow_biguint(&exp)
    }

    /// Modular exponentiation using BigUint exponent
    fn pow_biguint(&self, exp: &BigUint) -> Self {
        let mut result = Fe::ONE;
        let mut base = *self;
        let mut e = exp.clone();
        let zero = BigUint::zero();
        
        while e > zero {
            if &e & BigUint::one() == BigUint::one() {
                result = result.mul(&base);
            }
            base = base.mul(&base);
            e >>= 1;
        }
        result
    }

    /// Modular exponentiation using Fe exponent (for scalar_pow on order N)
    pub fn pow(&self, exp: &Fe) -> Self {
        let exp_big = exp.to_biguint();
        self.pow_biguint(&exp_big)
    }

    pub fn shr1(&self) -> Self {
        let a = self.to_biguint();
        let result = &a >> 1;
        Self::from_biguint_raw(&result)
    }

    pub fn shl_bits(&self, n: usize) -> Self {
        if n == 0 { return *self; }
        if n >= 256 { return Fe::ZERO; }
        let a = self.to_biguint();
        let p = Self::p_big();
        let result = (&a << n) % &p;
        Self::from_biguint_mod(&result)
    }

    pub fn bit_length(&self) -> u32 {
        let a = self.to_biguint();
        a.bits() as u32
    }

    pub fn power_of_2(n: u32) -> Self {
        let result = BigUint::from(1u64) << n as usize;
        Self::from_biguint_raw(&result)
    }
}

impl std::ops::Add for Fe {
    type Output = Fe;
    fn add(self, o: Fe) -> Fe { Fe::add(&self, &o) }
}
impl std::ops::Sub for Fe {
    type Output = Fe;
    fn sub(self, o: Fe) -> Fe { Fe::sub(&self, &o) }
}
impl std::ops::Mul for Fe {
    type Output = Fe;
    fn mul(self, o: Fe) -> Fe { Fe::mul(&self, &o) }
}
