//! Implémentation scalaire SHA-256 (pure Rust, portable)
//!
//! Optimisations incluses :
//! - Inlining agressif des rounds
//! - Schedule de message en place (w[0..63])
//! - Support de la compression intermédiaire pour le midstate

/// Constantes K du SHA-256 (64 valeurs 32-bit dérivées des racines cubiques des premiers)
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// État interne SHA-256 (8 mots 32-bit)
#[derive(Clone, Copy, Debug)]
pub struct Sha256State {
    pub h: [u32; 8],
}

impl Sha256State {
    /// État initial SHA-256 (constantes H0..H7)
    pub const INITIAL: Sha256State = Sha256State {
        h: [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
            0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
        ],
    };

    /// Créer un état à partir de 8 mots
    #[inline(always)]
    pub fn new(h: [u32; 8]) -> Self {
        Self { h }
    }

    /// Extraire le hash final en big-endian (32 octets)
    #[inline(always)]
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..8 {
            out[i * 4..i * 4 + 4].copy_from_slice(&self.h[i].to_be_bytes());
        }
        out
    }
}

/// Rotations et fonctions logiques SHA-256
#[inline(always)]
fn rotr(x: u32, n: u32) -> u32 {
    x.rotate_right(n)
}

#[inline(always)]
fn ch(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (!x & z)
}

#[inline(always)]
fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (x & z) ^ (y & z)
}

#[inline(always)]
fn big_sigma0(x: u32) -> u32 {
    rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22)
}

#[inline(always)]
fn big_sigma1(x: u32) -> u32 {
    rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25)
}

#[inline(always)]
fn small_sigma0(x: u32) -> u32 {
    rotr(x, 7) ^ rotr(x, 18) ^ (x >> 3)
}

#[inline(always)]
fn small_sigma1(x: u32) -> u32 {
    rotr(x, 17) ^ rotr(x, 19) ^ (x >> 10)
}

/// Compression SHA-256 : traite un bloc de 64 octets (512 bits)
///
/// C'est le cœur du moteur. Prend un état et un bloc, retourne le nouvel état.
/// L'ancien état n'est PAS modifié (copy semantics en Rust).
#[inline(always)]
pub fn compress_block(state: &Sha256State, block: &[u8; 64]) -> Sha256State {
    debug_assert_eq!(block.len(), 64);

    // --- Schedule de message ---
    let mut w = [0u32; 64];

    // Les 16 premiers mots proviennent directement du bloc (big-endian)
    for i in 0..16 {
        w[i] = u32::from_be_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }

    // Extension : w[16..63] via sigma0 et sigma1
    for i in 16..64 {
        w[i] = small_sigma1(w[i - 2])
            .wrapping_add(w[i - 7])
            .wrapping_add(small_sigma0(w[i - 15]))
            .wrapping_add(w[i - 16]);
    }

    // --- 64 rounds de compression ---
    let mut a = state.h[0];
    let mut b = state.h[1];
    let mut c = state.h[2];
    let mut d = state.h[3];
    let mut e = state.h[4];
    let mut f = state.h[5];
    let mut g = state.h[6];
    let mut h = state.h[7];

    for i in 0..64 {
        let t1 = h
            .wrapping_add(big_sigma1(e))
            .wrapping_add(ch(e, f, g))
            .wrapping_add(K[i])
            .wrapping_add(w[i]);

        let t2 = big_sigma0(a).wrapping_add(maj(a, b, c));

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    Sha256State {
        h: [
            state.h[0].wrapping_add(a),
            state.h[1].wrapping_add(b),
            state.h[2].wrapping_add(c),
            state.h[3].wrapping_add(d),
            state.h[4].wrapping_add(e),
            state.h[5].wrapping_add(f),
            state.h[6].wrapping_add(g),
            state.h[7].wrapping_add(h),
        ],
    }
}

/// SHA-256 complet sur un message arbitraire
///
/// Gère le padding (0x80 + zéros + longueur 64-bit big-endian).
pub fn sha256_raw(message: &[u8]) -> [u8; 32] {
    let len_bits = (message.len() as u64) * 8;

    // Padding : message + 0x80 + zéros + 8 octets de longueur
    let mut padded = message.to_vec();
    padded.push(0x80);

    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }

    padded.extend_from_slice(&len_bits.to_be_bytes());

    // Compression bloc par bloc
    let mut state = Sha256State::INITIAL;

    for chunk in padded.chunks_exact(64) {
        let block: [u8; 64] = chunk.try_into().unwrap();
        state = compress_block(&state, &block);
    }

    state.to_bytes()
}

/// SHA-256d (double hash) — la fonction utilisée dans le minage Bitcoin
///
/// SHA256d(x) = SHA256(SHA256(x))
///
/// C'est la primitive fondamentale du mining Bitcoin. Le bloc d'en-tête
/// (80 octets) est haché deux fois pour obtenir le hash final.
#[inline]
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = sha256_raw(data);
    sha256_raw(&first)
}

/// SHA-256d optimisé pour les en-têtes de bloc Bitcoin (80 octets)
///
/// Un en-tête de bloc fait exactement 80 octets, ce qui tient dans
/// un seul bloc de 64 octets + padding. Cette fonction évite les
/// allocations inutiles et le calcul de la longueur dynamique.
pub fn sha256d_block_header(header: &[u8; 80]) -> [u8; 32] {
    // Premier hash : padding de 80 octets → 128 octets (2 blocs)
    // Bloc 1 = octets 0..63 du header, Bloc 2 = octets 64..79 + padding
    let mut state = Sha256State::INITIAL;

    // Bloc 1 : premiers 64 octets de l'en-tête
    let block1: [u8; 64] = header[0..64].try_into().unwrap();
    state = compress_block(&state, &block1);

    // Bloc 2 : octets 64..79 + padding + longueur
    let mut block2 = [0u8; 64];
    block2[0..16].copy_from_slice(&header[64..80]);
    block2[16] = 0x80; // bit de padding
    // Les octets 17..56 sont déjà 0x00
    // Longueur en bits : 80 * 8 = 640 = 0x0280
    block2[56..64].copy_from_slice(&640u64.to_be_bytes());

    state = compress_block(&state, &block2);
    let first_hash = state.to_bytes();

    // Deuxième hash : SHA-256 d'un message de 32 octets
    // 32 + 1(0x80) + 23(zéros) + 8(longueur) = 64 → exactement 1 bloc
    let mut padded_second = [0u8; 64];
    padded_second[0..32].copy_from_slice(&first_hash);
    padded_second[32] = 0x80;
    // octets 33..56 = 0x00 (déjà)
    padded_second[56..64].copy_from_slice(&256u64.to_be_bytes()); // 32 * 8

    let state2 = Sha256State::INITIAL;
    let final_state = compress_block(&state2, &padded_second);
    final_state.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let result = sha256_raw(&[]);
        assert_eq!(
            hex::encode(result),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_abc() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let result = sha256_raw(b"abc");
        assert_eq!(
            hex::encode(result),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_sha256d() {
        // SHA256d("abc") = SHA256(SHA256("abc"))
        let first = sha256_raw(b"abc");
        let expected = sha256_raw(&first);
        let actual = sha256d(b"abc");
        assert_eq!(actual, expected);
    }

    /// Helper pour encoder en hex (minimal, sans crate externe)
    mod hex {
        pub fn encode(bytes: [u8; 32]) -> String {
            bytes.iter().map(|b| format!("{:02x}", b)).collect()
        }
    }
}
