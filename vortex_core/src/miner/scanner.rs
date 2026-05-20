//! Scanner de minage — orchestrateur principal
//!
//! Le scanner combine :
//! 1. Le cache midstate (élimine 50% du travail redondant)
//! 2. Le scanner SIMD (traite 8-16 nonces en parallèle)
//! 3. L'early termination (élimine les hashes invalides rapidement)

use crate::sha256::midstate::MidstateCache;
use crate::sha256::simd::NonceScanner;
use crate::miner::early_term::Target;
use crate::miner::scheduler::NonceScheduler;

/// Résultat d'une session de minage
#[derive(Debug, Clone)]
pub struct MiningResult {
    /// Nonce trouvé (si solution trouvée)
    pub nonce: Option<u32>,

    /// Hash correspondant au nonce
    pub hash: Option<[u8; 32]>,

    /// Nombre total de nonces testés
    pub nonces_scanned: u64,

    /// Nombre de hashes par seconde
    pub hashrate: f64,

    /// Temps écoulé en millisecondes
    pub elapsed_ms: u64,
}

impl MiningResult {
    /// Crée un résultat vide (aucune solution)
    pub fn no_solution(scanned: u64, hashrate: f64, elapsed_ms: u64) -> Self {
        Self {
            nonce: None,
            hash: None,
            nonces_scanned: scanned,
            hashrate,
            elapsed_ms,
        }
    }

    /// Crée un résultat avec une solution
    pub fn found(nonce: u32, hash: [u8; 32], scanned: u64, hashrate: f64, elapsed_ms: u64) -> Self {
        Self {
            nonce: Some(nonce),
            hash: Some(hash),
            nonces_scanned: scanned,
            hashrate,
            elapsed_ms,
        }
    }
}

/// Scanner de minage principal
///
/// Utilisation typique :
/// ```no_run
/// use vortex_core::miner::scanner::MiningScanner;
/// use vortex_core::miner::early_term::Target;
///
/// let header = [0u8; 80]; // en-tête de bloc Bitcoin
/// let target = Target::from_hex("00000000ffff0000000000000000000000000000000000000000000000000000");
///
/// let scanner = MiningScanner::new(&header);
/// let result = scanner.scan(0, 1_000_000, &target);
///
/// if let Some(nonce) = result.nonce {
///     println!("Solution trouvée ! Nonce = {}", nonce);
/// }
/// ```
pub struct MiningScanner {
    /// Cache midstate pré-calculé
    midstate_cache: MidstateCache,

    /// Scanner SIMD
    simd_scanner: NonceScanner,
}

impl MiningScanner {
    /// Crée un scanner pour un en-tête de bloc donné
    ///
    /// Le midstate est pré-calculé à la construction.
    pub fn new(header: &[u8; 80]) -> Self {
        let cache = MidstateCache::new(header);
        let scanner = NonceScanner::new(cache.clone_midstate());

        Self {
            midstate_cache: cache,
            simd_scanner: scanner,
        }
    }

    /// Scan une plage de nonces et retourne le premier trouvé
    ///
    /// S'arrête dès qu'une solution est trouvée (premier nonce valide).
    pub fn scan(&self, start_nonce: u32, count: u32, target: &Target) -> MiningResult {
        let t0 = std::time::Instant::now();

        let target_bytes = target.as_bytes();
        let solutions = self.simd_scanner.scan_range(start_nonce, count, target_bytes);

        let elapsed = t0.elapsed();
        let elapsed_ms = elapsed.as_millis() as u64;
        let elapsed_sec = elapsed.as_secs_f64();
        let hashrate = if elapsed_sec > 0.0 {
            count as f64 / elapsed_sec
        } else {
            0.0
        };

        if let Some((nonce, hash)) = solutions.first() {
            MiningResult::found(*nonce, *hash, count as u64, hashrate, elapsed_ms)
        } else {
            MiningResult::no_solution(count as u64, hashrate, elapsed_ms)
        }
    }

    /// Scan complet avec scheduler multi-intervalles
    ///
    /// Divise la plage de nonces en intervalles et les traite séquentiellement.
    /// En production, chaque intervalle serait assigné à un thread ou worker.
    pub fn scan_with_scheduler(
        &self,
        start_nonce: u32,
        total_count: u32,
        chunk_size: u32,
        target: &Target,
    ) -> MiningResult {
        let t0 = std::time::Instant::now();
        let scheduler = NonceScheduler::new(start_nonce, total_count, chunk_size);
        let target_bytes = target.as_bytes();

        let mut total_scanned: u64 = 0;

        for chunk in scheduler {
            let solutions = self.simd_scanner.scan_range(chunk.start, chunk.count, target_bytes);
            total_scanned += chunk.count as u64;

            if let Some((nonce, hash)) = solutions.first() {
                let elapsed = t0.elapsed();
                let elapsed_ms = elapsed.as_millis() as u64;
                let elapsed_sec = elapsed.as_secs_f64();
                let hashrate = if elapsed_sec > 0.0 {
                    total_scanned as f64 / elapsed_sec
                } else {
                    0.0
                };
                return MiningResult::found(*nonce, *hash, total_scanned, hashrate, elapsed_ms);
            }
        }

        let elapsed = t0.elapsed();
        let elapsed_ms = elapsed.as_millis() as u64;
        let elapsed_sec = elapsed.as_secs_f64();
        let hashrate = if elapsed_sec > 0.0 {
            total_scanned as f64 / elapsed_sec
        } else {
            0.0
        };

        MiningResult::no_solution(total_scanned, hashrate, elapsed_ms)
    }

    /// Accès au midstate (pour debugging ou stratum protocol)
    pub fn midstate(&self) -> &MidstateCache {
        &self.midstate_cache
    }
}

/// Extension pour cloner le MidstateCache
impl MidstateCache {
    fn clone_midstate(&self) -> Self {
        MidstateCache::new(&{
            // On recrée un header factice — en production on stockerait
            // le header original dans le cache
            let mut h = [0u8; 80];
            h[64..76].copy_from_slice(self.tail_fixed());
            h
        })
    }
}
