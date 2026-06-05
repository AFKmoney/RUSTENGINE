//! 256-bit modular arithmetic for secp256k1
//! [u64; 4] limbs, big-endian across limbs

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fe {
    pub limbs: [u64; 4],
}

impl Fe {
    pub const ZERO: Fe = Fe { limbs: [0, 0, 0, 0] };
    pub const ONE: Fe = Fe { limbs: [1, 0, 0, 0] };

    const P: [u64; 4] = [
        0xFFFFFFFEFFFFFC2F, 0xFFFFFFFFFFFFFFFF,
        0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF,
    ];

    #[inline]
    pub const fn from_u64(v: u64) -> Self { Fe { limbs: [v, 0, 0, 0] } }
    #[inline]
    pub const fn from_u64_limbs(l: [u64; 4]) -> Self { Fe { limbs: l } }

    pub fn from_bytes(b: &[u8; 32]) -> Self {
        // Big-endian input: b[0] is MSB
        // limbs[0] is LSB, limbs[3] is MSB
        let mut l = [0u64; 4];
        for i in 0..4 {
            let s = (3 - i) * 8;
            l[i] = u64::from_be_bytes([b[s],b[s+1],b[s+2],b[s+3],b[s+4],b[s+5],b[s+6],b[s+7]]);
        }
        Fe { limbs: l }.normalize()
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
        Self::from_bytes(&arr)
    }

    #[inline] pub fn is_zero(&self) -> bool { self.limbs.iter().all(|&x| x == 0) }

    pub fn cmp_val(&self, other: &[u64; 4]) -> Ordering {
        for i in (0..4).rev() {
            if self.limbs[i] < other[i] { return Ordering::Less; }
            if self.limbs[i] > other[i] { return Ordering::Greater; }
        }
        Ordering::Equal
    }

    pub fn normalize(&self) -> Self {
        if self.cmp_val(&Self::P) != Ordering::Less { self.sub_impl(&Self::P) } else { *self }
    }

    fn sub_impl(&self, other: &[u64; 4]) -> Self {
        let mut borrow = 0i128;
        let mut r = [0u64; 4];
        for i in 0..4 {
            let d = self.limbs[i] as i128 - other[i] as i128 - borrow;
            if d < 0 { r[i] = (d + (1i128 << 64)) as u64; borrow = 1; }
            else { r[i] = d as u64; borrow = 0; }
        }
        Fe { limbs: r }
    }

    pub fn add(&self, o: &Fe) -> Self {
        let mut carry = 0u64;
        let mut r = [0u64; 4];
        for i in 0..4 {
            let (s1, c1) = self.limbs[i].overflowing_add(o.limbs[i]);
            let (s2, c2) = s1.overflowing_add(carry);
            r[i] = s2;
            carry = c1 as u64 + c2 as u64;
        }
        let fe = Fe { limbs: r };
        if carry > 0 || fe.cmp_val(&Self::P) != Ordering::Less { fe.sub_impl(&Self::P) } else { fe }
    }

    pub fn sub(&self, o: &Fe) -> Self {
        let mut borrow = 0u64;
        let mut r = [0u64; 4];
        for i in 0..4 {
            let (d1, b1) = self.limbs[i].overflowing_sub(o.limbs[i]);
            let (d2, b2) = d1.overflowing_sub(borrow);
            r[i] = d2;
            borrow = b1 as u64 + b2 as u64;
        }
        if borrow > 0 {
            let mut carry = 0u64;
            for i in 0..4 {
                let (s1, c1) = r[i].overflowing_add(Self::P[i]);
                let (s2, c2) = s1.overflowing_add(carry);
                r[i] = s2;
                carry = c1 as u64 + c2 as u64;
            }
        }
        Fe { limbs: r }
    }

    pub fn neg_mod_p(&self) -> Self {
        if self.is_zero() { *self } else {
            let p = Fe { limbs: Self::P };
            p.sub_impl(&self.limbs)
        }
    }

    pub fn mul(&self, o: &Fe) -> Self {
        // Use num-bigint for correct modular multiplication
        let a_big = self.to_biguint();
        let b_big = o.to_biguint();
        let p_big = Self::P_big();
        let result = (a_big * b_big) % p_big;
        Self::from_biguint(&result)
    }

    fn P_big() -> num_bigint::BigUint {
        use num_bigint::BigUint;
        BigUint::parse_bytes(
            b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F",
            16
        ).unwrap()
    }

    fn to_biguint(&self) -> num_bigint::BigUint {
        use num_bigint::BigUint;
        let bytes = self.to_bytes();
        BigUint::from_bytes_be(&bytes)
    }

    fn from_biguint(v: &num_bigint::BigUint) -> Self {
        let bytes = v.to_bytes_be();
        let mut arr = [0u8; 32];
        let start = 32 - bytes.len().min(32);
        arr[start..32].copy_from_slice(&bytes[..bytes.len().min(32)]);
        Self::from_bytes(&arr)
    }

    pub fn modinv(&self) -> Self {
        if self.is_zero() { panic!("modinv of zero"); }
        let exp = Fe { limbs: [0xFFFFFFFEFFFFFC2D, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF] };
        self.pow(&exp)
    }

    pub fn pow(&self, exp: &Fe) -> Self {
        let mut result = Fe::ONE;
        let mut base = *self;
        let mut e = *exp;
        for _ in 0..512 {
            if e.is_zero() { break; }
            if e.limbs[0] & 1 == 1 { result = result.mul(&base); }
            base = base.mul(&base);
            e = e.shr1();
        }
        result
    }

    pub fn shr1(&self) -> Self {
        let mut r = [0u64; 4];
        for i in (0..4).rev() {
            r[i] = self.limbs[i] >> 1;
            if i > 0 { r[i] |= self.limbs[i-1] << 63; }
        }
        Fe { limbs: r }
    }

    pub fn shl_bits(&self, n: usize) -> Self {
        if n == 0 { return *self; }
        if n >= 256 { return Fe::ZERO; }
        let ws = n / 64;
        let bs = n % 64;
        let mut r = [0u64; 4];
        for i in (ws..4).rev() {
            r[i] = self.limbs[i - ws] << bs;
            if bs > 0 && i + 1 < 4 { r[i + 1] |= self.limbs[i - ws] >> (64 - bs); }
        }
        // Simpler approach for our use case
        Fe { limbs: r }.normalize()
    }

    pub fn bit_length(&self) -> u32 {
        for i in (0..4).rev() {
            if self.limbs[i] != 0 { return (i as u32) * 64 + 64 - self.limbs[i].leading_zeros(); }
        }
        0
    }

    pub fn power_of_2(n: u32) -> Self {
        let word = n as usize / 64;
        let bit = n % 64;
        let mut l = [0u64; 4];
        if word < 4 { l[word] = 1u64 << bit; }
        Fe { limbs: l }
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
