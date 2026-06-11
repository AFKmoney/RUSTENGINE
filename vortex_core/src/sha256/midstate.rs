//! Cache de Midstate SHA-256
//!
//! Dans le minage Bitcoin, l'en-tête de bloc fait 80 octets.
//! Le premier hash SHA-256 traite ces 80 octets en 2 blocs de 64 :
//!   - Bloc 1 : octets 0..63 (version + hash_prev_block + merkle_root_partiel)
//!   - Bloc 2 : octets 64..79 (merkle_root_suite + timestamp + bits + nonce) + padding
//!
//! Quand on itère sur les nonces, **seul le nonce change** (octets 76..79).
//! Le Bloc 1 est **identique** pour tous les nonces !
//!
//! Le midstate = état SHA-256 après compression du Bloc 1.
//! En le pré-calculant une seule fois, on économise 50% du travail
//! du premier hash.

use super::scalar::{compress_block, Sha256State};

/// Cache de midstate pour le minage
///
/// Pré-calcul la partie fixe de la compression SHA-256 sur l'en-tête
/// de bloc, puis permet de réutiliser ce résultat pour chaque nonce.
pub struct MidstateCache {
    /// État SHA-256 après compression du bloc 1 (octets 0..63 de l'en-tête)
    midstate: Sha256State,

    /// Octets 64..79 de l'en-tête (sans le nonce — 4 derniers octets = 0)
    /// Ces 12 octets sont fixes pour tous les nonces
    tail_fixed: [u8; 12],

    /// Padding pré-calculé pour le bloc 2
    /// Après les 16 octets (12 fixes + 4 nonce), on a :
    /// octet 16: 0x80, octets 17..55: 0x00, octets 56..63: longueur (640 bits)
    padding: [u8; 48],
}

impl MidstateCache {
    /// Construit le cache à partir d'un en-tête de bloc Bitcoin (80 octets)
    ///
    /// L'en-tête est structuré ainsi :
    ///   [0..4]   version (little-endian)
    ///   [4..36]  hash_prev_block (little-endian)
    ///   [36..68] merkle_root (little-endian)
    ///   [68..72] timestamp (little-endian)
    ///   [72..76] bits / target (little-endian)
    ///   [76..80] nonce (little-endian) — VARIE
    ///
    /// Le nonce est ignoré dans le cache car il change à chaque itération.
    pub fn new(header: &[u8; 80]) -> Self {
        // Compresser le bloc 1 (octets 0..63) une seule fois
        let block1: [u8; 64] = header[0..64].try_into().unwrap();
        let midstate = compress_block(&Sha256State::INITIAL, &block1);

        // Sauvegarder les octets fixes du bloc 2 (octets 64..75)
        let mut tail_fixed = [0u8; 12];
        tail_fixed.copy_from_slice(&header[64..76]);

        // Pré-calculer le padding du bloc 2
        let mut padding = [0u8; 48];
        padding[0] = 0x80; // bit de padding après les 16 octets (12 + 4 nonce)
        // octets 1..40 = 0x00 (déjà)
        padding[40..48].copy_from_slice(&640u64.to_be_bytes()); // longueur en bits

        MidstateCache {
            midstate,
            tail_fixed,
            padding,
        }
    }

    /// Calcule SHA-256d de l'en-tête avec un nonce donné
    ///
    /// Utilise le midstate pré-calculé pour éviter la recompression
    /// du bloc 1. Gains : ~50% du travail du premier hash économisé.
    #[inline(always)]
    pub fn sha256d_with_nonce(&self, nonce: u32) -> [u8; 32] {
        use super::scalar::compress_block;

        // --- Premier hash (SHA-256) ---
        // Bloc 2 : tail_fixed (12 octets) + nonce (4 octets) + padding (48 octets)
        let mut block2 = [0u8; 64];
        block2[0..12].copy_from_slice(&self.tail_fixed);
        block2[12..16].copy_from_slice(&nonce.to_le_bytes());
        block2[16..64].copy_from_slice(&self.padding);

        let state_after_block2 = compress_block(&self.midstate, &block2);
        let first_hash = state_after_block2.to_bytes();

        // --- Deuxième hash (SHA-256 du premier hash, 32 octets → 1 bloc) ---
        let mut padded_second = [0u8; 64];
        padded_second[0..32].copy_from_slice(&first_hash);
        padded_second[32] = 0x80;
        padded_second[56..64].copy_from_slice(&256u64.to_be_bytes());

        let final_state = compress_block(&Sha256State::INITIAL, &padded_second);
        final_state.to_bytes()
    }

    /// Retourne l'état midstate (utile pour le débogage ou l'envoi réseau)
    pub fn midstate(&self) -> &Sha256State {
        &self.midstate
    }

    /// Retourne les octets fixes de la queue (sans nonce)
    pub fn tail_fixed(&self) -> &[u8; 12] {
        &self.tail_fixed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midstate_matches_sha256d() {
        use crate::sha256::sha256d_block_header;

        // En-tête de bloc Bitcoin de test (80 octets)
        // Bloc #0 (genesis block header — valeurs connues)
        let mut header = [0u8; 80];
        // Version
        header[0..4].copy_from_slice(&1u32.to_le_bytes());
        // Hash prev block (tout zéros pour le genesis)
        // déjà 0
        // Merkle root (celui du genesis)
        let merkle_hex = "3ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a";
        for i in 0..32 {
            header[36 + i] = u8::from_str_radix(&merkle_hex[i * 2..i * 2 + 2], 16).unwrap();
        }
        // Timestamp
        header[68..72].copy_from_slice(&0x495fab29u32.to_le_bytes());
        // Bits
        header[72..76].copy_from_slice(&0x1d00ffffu32.to_le_bytes());
        // Nonce
        header[76..80].copy_from_slice(&0x7c2bac1du32.to_le_bytes());

        // Comparer les deux méthodes
        let cache = MidstateCache::new(&header);
        let result_midstate = cache.sha256d_with_nonce(0x7c2bac1d);
        let result_direct = sha256d_block_header(&header);

        assert_eq!(
            result_midstate, result_direct,
            "Le hash midstate doit être identique au hash direct"
        );
    }
}
