//! RUSTSOLVER v3 — Native u64x4 Modular Arithmetic for secp256k1
//! ================================================================
//! ZERO BigUint in the hot path. Pure u64x4 limb arithmetic.
//! FAST reduce512() for secp256k1 special prime P = 2^256 - 2^32 - 977.
//! BigUint fallback for mod N (no special form).

use std::cmp::Ordering;
use std::fmt;
use num_bigint::BigUint;

// ============================================================
// secp256k1 CONSTANTS
// ============================================================

/// P = 2^256 - 2^32 - 977
pub const P: [u64; 4] = [
    0xFFFFFFFEFFFFFC2F,
    0xFFFFFFFFFFFFFFFF,
    0xFFFFFFFFFFFFFFFF,
    0xFFFFFFFFFFFFFFFF,
];

/// Group order N
pub const N: [u64; 4] = [
    0xBFD25E8CD0364141,
    0xBAAEDCE6AF48A03B,
    0xFFFFFFFFFFFFFFFE,
    0xFFFFFFFFFFFFFFFF,
];

/// Beta: non-trivial cube root of unity mod P (beta^3 = 1 mod P)
pub const BETA: [u64; 4] = [
    0xC1396C28719501EE,
    0x9CF0497512F58995,
    0x6E64479EAC3434E9,
    0x7AE96A2B657C0710,
];

/// Lambda: non-trivial cube root of unity mod N (lambda^3 = 1 mod N)
pub const LAMBDA: [u64; 4] = [
    0xDF02967C1B23BD72,
    0x122E22EA20816678,
    0xA5261C028812645A,
    0x5363AD4CC05C30E0,
];

// ============================================================
// FIELD ELEMENT
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fe {
    pub limbs: [u64; 4],
}

impl Fe {
    pub const ZERO: Fe = Fe { limbs: [0, 0, 0, 0] };
    pub const ONE: Fe = Fe { limbs: [1, 0, 0, 0] };

    // ============================================================
    // CONSTRUCTORS
    // ============================================================

    #[inline]
    pub const fn from_u64(v: u64) -> Self {
        Fe { limbs: [v, 0, 0, 0] }
    }

    pub fn from_bytes(b: &[u8; 32]) -> Self {
        let mut l = [0u64; 4];
        for i in 0..4 {
            let s = (3 - i) * 8;
            l[i] = u64::from_be_bytes([
                b[s], b[s+1], b[s+2], b[s+3],
                b[s+4], b[s+5], b[s+6], b[s+7],
            ]);
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
        let fe = Fe::from_bytes(&arr);
        if fe.cmp_val(&P) != Ordering::Less {
            fe.sub_p()
        } else {
            fe
        }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.limbs.iter().all(|&x| x == 0)
    }

    // ============================================================
    // COMPARISON
    // ============================================================

    #[inline]
    pub fn cmp_val(&self, other: &[u64; 4]) -> Ordering {
        for i in (0..4).rev() {
            if self.limbs[i] < other[i] { return Ordering::Less; }
            if self.limbs[i] > other[i] { return Ordering::Greater; }
        }
        Ordering::Equal
    }

    // ============================================================
    // RAW ADD/SUB (no reduction)
    // ============================================================

    #[inline]
    fn add_raw(&self, o: &Fe) -> (Fe, u64) {
        let (r0, c0) = adc(self.limbs[0], o.limbs[0], 0);
        let (r1, c1) = adc(self.limbs[1], o.limbs[1], c0);
        let (r2, c2) = adc(self.limbs[2], o.limbs[2], c1);
        let (r3, c3) = adc(self.limbs[3], o.limbs[3], c2);
        (Fe { limbs: [r0, r1, r2, r3] }, c3)
    }

    #[inline]
    fn sub_raw(&self, o: &Fe) -> (Fe, u64) {
        let (r0, b0) = sbb(self.limbs[0], o.limbs[0], 0);
        let (r1, b1) = sbb(self.limbs[1], o.limbs[1], b0);
        let (r2, b2) = sbb(self.limbs[2], o.limbs[2], b1);
        let (r3, b3) = sbb(self.limbs[3], o.limbs[3], b2);
        (Fe { limbs: [r0, r1, r2, r3] }, b3)
    }

    // ============================================================
    // MODULAR ARITHMETIC (mod P) — THE HOT PATH
    // ============================================================

    /// Modular addition mod P
    #[inline]
    pub fn add(&self, o: &Fe) -> Self {
        let (mut r, mut carry) = self.add_raw(o);
        // Fold 2^256 → 2^32 + 977 = 0x1000003D1
        while carry > 0 {
            let correction = Fe { limbs: [0x1000003D1, 0, 0, 0] };
            let (r2, carry2) = r.add_raw(&correction);
            r = r2;
            carry = carry2;
        }
        // Conditional subtract P
        for _ in 0..2 {
            if r.cmp_val(&P) != Ordering::Less {
                let (s, borrow) = r.sub_raw(&Fe { limbs: P });
                if borrow > 0 { break; }
                r = s;
            } else {
                break;
            }
        }
        r
    }

    /// Modular subtraction mod P
    #[inline]
    pub fn sub(&self, o: &Fe) -> Self {
        let (r, borrow) = self.sub_raw(o);
        if borrow > 0 { r.add_p() } else { r }
    }

    /// Negation mod P
    #[inline]
    pub fn neg_mod_p(&self) -> Self {
        if self.is_zero() { return *self; }
        Fe { limbs: P }.sub(self)
    }

    #[inline]
    fn sub_p(&self) -> Self {
        let (r, _) = self.sub_raw(&Fe { limbs: P });
        r
    }

    #[inline]
    fn add_p(&self) -> Self {
        let (r, _carry) = self.add_raw(&Fe { limbs: P });
        r
    }

    /// *** CRITICAL: mul() uses FAST reduce512() ***
    #[inline]
    pub fn mul(&self, o: &Fe) -> Self {
        // Schoolbook 4x4 → 8 limbs
        let mut prod = [0u64; 8];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                carry += (self.limbs[i] as u128) * (o.limbs[j] as u128);
                carry += prod[i + j] as u128;
                prod[i + j] = carry as u64;
                carry >>= 64;
            }
            prod[i + 4] += carry as u64;
        }
        reduce512(&prod)
    }

    /// Modular squaring
    #[inline]
    pub fn sqr(&self) -> Self {
        self.mul(self)
    }

    // ============================================================
    // SCALAR ARITHMETIC (mod N)
    // ============================================================

    #[inline]
    pub fn add_mod_n(&self, o: &Fe) -> Self {
        let (r, carry) = self.add_raw(o);
        if carry > 0 || r.cmp_val(&N) != Ordering::Less {
            let (r2, _) = r.sub_raw(&Fe { limbs: N });
            r2
        } else {
            r
        }
    }

    #[inline]
    pub fn sub_mod_n(&self, o: &Fe) -> Self {
        let (r, borrow) = self.sub_raw(o);
        if borrow > 0 {
            let (r2, _) = r.add_raw(&Fe { limbs: N });
            r2
        } else {
            r
        }
    }

    #[inline]
    pub fn neg_mod_n(&self) -> Self {
        if self.is_zero() { return *self; }
        let n = Fe { limbs: N };
        n.sub(self)
    }

    /// mul mod N — uses BigUint (N has no special form for fast reduction)
    pub fn mul_mod_n(&self, o: &Fe) -> Self {
        let mut prod = [0u64; 8];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                carry += (self.limbs[i] as u128) * (o.limbs[j] as u128);
                carry += prod[i + j] as u128;
                prod[i + j] = carry as u64;
                carry >>= 64;
            }
            prod[i + 4] += carry as u64;
        }
        let mut bytes = [0u8; 64];
        for i in 0..8 {
            let b = prod[i].to_le_bytes();
            bytes[i*8..(i+1)*8].copy_from_slice(&b);
        }
        let big = BigUint::from_bytes_le(&bytes);
        let n = BigUint::parse_bytes(
            b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141", 16
        ).unwrap();
        let reduced = big % n;
        let r_bytes = reduced.to_bytes_be();
        let mut arr = [0u8; 32];
        let start = 32 - r_bytes.len().min(32);
        arr[start..32].copy_from_slice(&r_bytes[..r_bytes.len().min(32)]);
        Fe::from_bytes(&arr)
    }

    // ============================================================
    // BIT OPERATIONS
    // ============================================================

    #[inline]
    pub fn get_bit(&self, i: u32) -> bool {
        let limb = (i / 64) as usize;
        let bit = i % 64;
        if limb >= 4 { false } else { (self.limbs[limb] >> bit) & 1 == 1 }
    }

    pub fn bit_length(&self) -> u32 {
        for i in (0..4).rev() {
            if self.limbs[i] != 0 {
                return (i as u32) * 64 + 64 - self.limbs[i].leading_zeros();
            }
        }
        0
    }

    pub fn shr1(&self) -> Self {
        if self.limbs[0] & 1 != 0 {
            let (sum, _) = self.add_raw(&Fe { limbs: P });
            let mut r = sum;
            for i in 0..4 {
                r.limbs[i] >>= 1;
                if i + 1 < 4 && (sum.limbs[i + 1] & 1) != 0 {
                    r.limbs[i] |= 1 << 63;
                }
            }
            r
        } else {
            let mut r = *self;
            for i in 0..4 {
                r.limbs[i] >>= 1;
                if i + 1 < 4 && (self.limbs[i + 1] & 1) != 0 {
                    r.limbs[i] |= 1 << 63;
                }
            }
            r
        }
    }

    pub fn power_of_2(n: u32) -> Self {
        if n < 64 {
            Fe { limbs: [1u64 << n, 0, 0, 0] }
        } else {
            let mut result = Fe::ONE;
            for _ in 0..n {
                result = result.add(&result);
            }
            result
        }
    }

    // ============================================================
    // EXPONENTIATION (mod P)
    // ============================================================

    pub fn pow(&self, exp: &Fe) -> Self {
        let mut result = Fe::ONE;
        let base = *self;
        let bits = exp.bit_length();
        for i in (0..bits).rev() {
            result = result.sqr();
            if exp.get_bit(i) {
                result = result.mul(&base);
            }
        }
        result
    }

    /// Modular inverse mod P via Fermat: self^(P-2)
    pub fn modinv(&self) -> Self {
        if self.is_zero() { panic!("modinv of zero"); }
        let exp = Fe { limbs: [
            0xFFFFFFFEFFFFFC2D,
            0xFFFFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFFFFF,
        ]};
        self.pow(&exp)
    }

    // ============================================================
    // CONVERSION
    // ============================================================

    pub fn to_biguint(&self) -> BigUint {
        BigUint::from_bytes_be(&self.to_bytes())
    }

    pub fn from_biguint_mod_p(v: &BigUint) -> Self {
        let p_big = BigUint::parse_bytes(
            b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F", 16
        ).unwrap();
        let reduced = v % &p_big;
        let bytes = reduced.to_bytes_be();
        let mut arr = [0u8; 32];
        let start = 32 - bytes.len().min(32);
        arr[start..32].copy_from_slice(&bytes[..bytes.len().min(32)]);
        let fe = Fe::from_bytes(&arr);
        // Ensure fully reduced
        if fe.cmp_val(&P) != Ordering::Less { fe.sub_p() } else { fe }
    }

    pub fn from_biguint_mod_n(v: &BigUint) -> Self {
        let n_big = BigUint::parse_bytes(
            b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141", 16
        ).unwrap();
        let reduced = v % &n_big;
        let bytes = reduced.to_bytes_be();
        let mut arr = [0u8; 32];
        let start = 32 - bytes.len().min(32);
        arr[start..32].copy_from_slice(&bytes[..bytes.len().min(32)]);
        Fe::from_bytes(&arr)
    }
}

impl fmt::Display for Fe {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &b in &self.to_bytes() { write!(f, "{:02x}", b)?; }
        Ok(())
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

// ============================================================
// LOW-LEVEL HELPERS
// ============================================================

#[inline]
fn adc(a: u64, b: u64, carry_in: u64) -> (u64, u64) {
    let sum = a as u128 + b as u128 + carry_in as u128;
    (sum as u64, (sum >> 64) as u64)
}

#[inline]
fn sbb(a: u64, b: u64, borrow_in: u64) -> (u64, u64) {
    let borrow_bit = borrow_in & 1;
    let diff = (a as u128).wrapping_sub(b as u128).wrapping_sub(borrow_bit as u128);
    let borrow_out = if borrow_bit == 0 {
        if a < b { 1 } else { 0 }
    } else {
        if a <= b { 1 } else { 0 }
    };
    (diff as u64, borrow_out)
}

// ============================================================
// FAST 512-BIT REDUCTION mod P = 2^256 - 2^32 - 977
// ============================================================

/// Reduce a 512-bit number mod P using the special form of P.
/// P = 2^256 - 2^32 - 977  =>  2^256 = 2^32 + 977 (mod P)
/// MUL = 2^32 + 977 = 0x1000003D1 (33 bits)
fn reduce512(prod: &[u64; 8]) -> Fe {
    const MUL: u64 = 0x1000003D1;

    let mut t = [0u128; 5];

    // Load low 256 bits
    t[0] = prod[0] as u128;
    t[1] = prod[1] as u128;
    t[2] = prod[2] as u128;
    t[3] = prod[3] as u128;

    // Fold high 256 bits: hi * 2^256 = hi * MUL (mod P)
    for i in 0..4usize {
        let c = prod[4 + i] as u128 * MUL as u128;
        t[i] += c & 0xFFFFFFFFFFFFFFFF;
        t[i + 1] += c >> 64;
    }

    // Propagate carries
    for i in 0..4 {
        t[i + 1] += t[i] >> 64;
        t[i] &= 0xFFFFFFFFFFFFFFFF;
    }

    // Fold any overflow from t[4]
    for _ in 0..3 {
        if t[4] == 0 { break; }
        let c = t[4];
        t[4] = 0;
        let c_mul = c * MUL as u128;
        t[0] += c_mul & 0xFFFFFFFFFFFFFFFF;
        t[1] += c_mul >> 64;
        for i in 0..4 {
            t[i + 1] += t[i] >> 64;
            t[i] &= 0xFFFFFFFFFFFFFFFF;
        }
    }

    let mut result = Fe { limbs: [t[0] as u64, t[1] as u64, t[2] as u64, t[3] as u64] };

    // Final conditional subtraction (may need up to 2)
    for _ in 0..3 {
        if result.cmp_val(&P) != Ordering::Less {
            let (s, borrow) = result.sub_raw(&Fe { limbs: P });
            if borrow > 0 { break; }
            result = s;
        } else {
            break;
        }
    }

    result
}
