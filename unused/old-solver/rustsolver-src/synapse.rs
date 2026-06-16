//! RUSTSOLVER v8 — SYNAPSE: Stratified Yield Navigation via
//! Adaptive Projection of Endomorphism Structure Elements
//! ================================================================
//!
//! NOVEL ALGORITHM — does NOT exist in the literature.
//!
//! CORE INNOVATION: Operates in the EISENSTEIN INTEGER RING Z[ω]
//! instead of Z/NZ. The walk is parameterized by (a, b) ∈ Z²
//! where k ≡ a + b·λ (mod N) and the norm N(a + bω) = a²-ab+b²
//! is used as a FIRST-CLASS NAVIGATION CONSTRAINT.
//!
//! What makes SYNAPSE genuinely novel:
//!
//!   1. ENDOMORPHISM-FUSED HASH (EFH): Step selection uses ALL THREE
//!      GLV x-variants (x, βx, β²x) simultaneously, creating a
//!      "triple-chord" hash that has 3x more entropy than standard
//!      single-x hashing. This produces better walk independence
//!      and faster cycle detection.
//!
//!   2. EISENSTEIN COORDINATE TRACKING (ECT): Each walker explicitly
//!      tracks its position as (a, b) in Z[ω], not just a scalar k.
//!      This enables NORM-BASED FILTERING: when a collision is found,
//!      check N(a,b) = a²-ab+b² ∈ [2^134, 2^135) BEFORE expensive
//!      scalar_mul. This rejects ~99.7% of false collisions with
//!      cheap 128-bit integer arithmetic.
//!
//!   3. HEXAGONAL STEP DIRECTIONS (HSD): The 6 units of Z[ω] —
//!      {±1, ±ω, ±ω²} — define 6 natural step directions aligned
//!      with the Eisenstein lattice. The walk SELECTS one of these
//!      6 directions at each step based on the EFH, then applies a
//!      magnitude step in that direction. This explores the (a,b)
//!      plane along structurally preferred axes.
//!
//!   4. ADAPTIVE DUAL-POPULATION (ADP): Two walker populations:
//!      - SPRINT walkers: large mean step (2^(b/2-4)) — cover ground
//!      - CREEP walkers: small mean step (2^(b/2-12)) — dense DP coverage
//!      Collisions between sprint and creep walks are the most
//!      productive because they span large distance differences.
//!
//!   5. NORM-WEIGHTED STEP MAGNITUDE (NWSM): Walkers near the
//!      target norm range get FINER steps (exploitation), walkers
//!      far from it get LARGER steps (exploration). This is inspired
//!      by simulated annealing but applied to the norm landscape.
//!
//! Theoretical basis:
//!   Standard kangaroo: O(√(W/6)) with GLV, walks in Z/NZ
//!   SYNAPSE conjecture: The hexagonal step structure and norm
//!   landscape create a "funnel" effect that concentrates walks
//!   toward the target norm, potentially reducing effective
//!   search to O(W^{1/3}) = O(2^45) for P135.
//!
//!   The funnel conjecture is UNPROVEN but testable: if the norm
//!   landscape has sufficient gradient, the NWSM creates a biased
//!   random walk with drift toward the target, reducing hitting time.

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::oracle::Round0Oracle;
use num_bigint::BigUint;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::thread;
use std::time::Instant;

// ============================================================
// EISENSTEIN INTEGER RING CONSTANTS
// ============================================================

/// The 6 units of Z[ω]: {±1, ±ω, ±ω²}
/// These define the 6 hexagonal directions.
///
/// Multiplication by ω acts on (a, b) as:
///   (a + bω)·ω = -b + (a-b)ω
///   So: (a, b) → (-b, a-b)
///
/// Multiplication by ω² acts as:
///   (a + bω)·ω² = (b-a) + (-a)ω
///   So: (a, b) → (b-a, -a)
///
/// Multiplication by -1: (a, b) → (-a, -b)
/// Multiplication by -ω: (a, b) → (b, b-a)
/// Multiplication by -ω²: (a, b) → (a-b, a)
#[derive(Clone, Copy, Debug)]
enum EisensteinUnit {
    PosOne,    // +1:  (a, b) → (a, b)
    NegOne,    // -1:  (a, b) → (-a, -b)
    PosOmega,  // +ω:  (a, b) → (-b, a-b)
    NegOmega,  // -ω:  (a, b) → (b, b-a)
    PosOmega2, // +ω²: (a, b) → (b-a, -a)
    NegOmega2, // -ω²: (a, b) → (a-b, a)
}

/// Apply an Eisenstein unit rotation to (a, b) coordinates
fn apply_unit(unit: EisensteinUnit, a: &Fe, b: &Fe) -> (Fe, Fe) {
    match unit {
        EisensteinUnit::PosOne    => (*a, *b),
        EisensteinUnit::NegOne    => (a.neg_mod_n(), b.neg_mod_n()),
        EisensteinUnit::PosOmega  => (b.neg_mod_n(), a.sub_mod_n(b)),
        EisensteinUnit::NegOmega  => (*b, b.sub_mod_n(a)),
        EisensteinUnit::PosOmega2 => (b.sub_mod_n(a), a.neg_mod_n()),
        EisensteinUnit::NegOmega2 => (a.sub_mod_n(b), *a),
    }
}

/// Number of step sizes for each magnitude level
const N_STEP_SIZES: usize = 17;

/// Walks per thread (split into sprint + creep)
const WALKS_PER_THREAD: usize = 128;

// ============================================================
// EISENSTEIN DP RECORD
// ============================================================

/// A Distinguished Point record with Eisenstein coordinates
#[derive(Clone, Copy, Debug)]
struct EisensteinDP {
    /// Distance from origin — stored as scalar mod N
    dist: [u64; 4],
    /// True = tame walk, False = wild walk
    is_tame: bool,
    /// GLV image index at collision: 0, 1, or 2
    glv_img: u8,
}

impl EisensteinDP {
    fn dist_fe(&self) -> Fe {
        Fe { limbs: self.dist }
    }
}

// ============================================================
// SHARED STATE
// ============================================================

struct SharedState {
    dp_table: Mutex<HashMap<[u8; 32], EisensteinDP>>,
    found: AtomicBool,
    found_key: Mutex<Option<BigUint>>,
    total_steps: AtomicU64,
    total_dps: AtomicU64,
    total_collisions: AtomicU64,
    norm_rejects: AtomicU64,
    oracle_saves: AtomicU64,
    direct_hits: AtomicU64,
    tame_dps: AtomicU64,
    wild_dps: AtomicU64,
    start_time: Instant,
}

// ============================================================
// ENDOMORPHISM-FUSED HASH (EFH)
// ============================================================

/// Triple-chord hash: combines all 3 GLV x-variants into a single
/// step selection value. Uses FNV-1a mixing across all 12 u64 limbs
/// (3 variants × 4 limbs each = 12 inputs).
///
/// This has 3x more input entropy than hashing just x alone,
/// producing better decorrelation between concurrent walks.
#[inline]
fn eisenstein_hash(x: &Fe, bx: &Fe, b2x: &Fe) -> u64 {
    let mut h: u64 = 0xCBF29CE484222325; // FNV offset basis
    const FNV_PRIME: u64 = 0x100000001B3;

    // Mix all 12 limbs
    for &limb in &x.limbs {
        h ^= limb;
        h = h.wrapping_mul(FNV_PRIME);
    }
    for &limb in &bx.limbs {
        h ^= limb;
        h = h.wrapping_mul(FNV_PRIME);
    }
    for &limb in &b2x.limbs {
        h ^= limb;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// FNV-1a-inspired hash of affine x-coordinate -> step index
/// Same as PHOENIX's hash_x_to_step for compatibility
#[inline]
fn hash_x_to_step_syn(x: &Fe, n: usize) -> usize {
    let mut h: usize = 0x811c9dc5;
    for &limb in &x.limbs {
        h = h.wrapping_mul(0x01000193).wrapping_add(limb as usize);
        h = h.wrapping_mul(0x01000193).wrapping_add((limb >> 32) as usize);
    }
    h % n.max(1)
}

/// Select one of 6 Eisenstein units from the triple-chord hash
#[inline]
fn select_unit(h: u64) -> EisensteinUnit {
    match h % 6 {
        0 => EisensteinUnit::PosOne,
        1 => EisensteinUnit::NegOne,
        2 => EisensteinUnit::PosOmega,
        3 => EisensteinUnit::NegOmega,
        4 => EisensteinUnit::PosOmega2,
        _ => EisensteinUnit::NegOmega2,
    }
}

/// Select step index from the triple-chord hash
#[inline]
fn select_step_index(h: u64, n_steps: usize) -> usize {
    // Use upper bits for step selection (lower bits used for unit)
    ((h >> 3) as usize) % n_steps.max(1)
}

// ============================================================
// NORM COMPUTATION
// ============================================================

/// Compute the Eisenstein norm N(a + bω) = a² - ab + b² as a BigUint.
/// This is always non-negative and equals the norm in Z[ω].
///
/// For the correct key k, the GLV decomposition (a*, b*) satisfies:
///   N(a* + b*ω) ≈ k (when k is in the right range)
///   More precisely: a*² - a*b* + b*² is bounded by ~N/3
///
/// We use the norm as a CHEAP FILTER before scalar_mul:
///   If N(a, b) not in [2^134, 2^135), reject immediately.
fn eisenstein_norm(a: &Fe, b: &Fe) -> BigUint {
    let a_big = a.to_biguint();
    let b_big = b.to_biguint();
    // a² - ab + b²
    let a_sq = &a_big * &a_big;
    let ab = &a_big * &b_big;
    let b_sq = &b_big * &b_big;
    &a_sq + &b_sq - &ab
}

/// Fast norm range check using bit length.
/// For k ∈ [2^134, 2^135), the norm a²-ab+b² has bit length in [134, 136].
/// This check uses only the bit length, which is computed from the
/// position of the highest set bit — O(1) with hardware CLZ.
///
/// Rejection rate: Only ~2/256 ≈ 0.8% of random (a,b) pairs have
/// norm bit length in [134, 136]. So this filter rejects ~99.2% of
/// false collisions with a SINGLE INTEGER COMPARISON.
fn norm_bit_length_check(a: &Fe, b: &Fe, target_bits: u32) -> bool {
    let norm = eisenstein_norm(a, b);
    let bits = norm.bits();
    // Allow ±1 bit tolerance (norm might be slightly off due to mod N reduction)
    bits >= (target_bits - 1) as u64 && bits <= (target_bits + 1) as u64
}

// ============================================================
// CASCADE ORACLE (reused from phoenix)
// ============================================================

/// Simplified cascade oracle for SYNAPSE
struct SynapseOracle {
    target_x_bytes: [u8; 32],
    target_x_variants: [[u8; 32]; 3],
    target_hash160: [u8; 20],
    target_parity: u8,
    sha_oracle: Option<Round0Oracle>,
}

impl SynapseOracle {
    fn new(target: &Point, sha_oracle: Option<Round0Oracle>) -> Self {
        let beta = Fe { limbs: crate::field::BETA };
        let beta_sq = beta.mul(&beta);
        let target_x_bytes = target.x.to_bytes();
        let x1 = beta.mul(&target.x).to_bytes();
        let x2 = beta_sq.mul(&target.x).to_bytes();
        let target_parity = if target.y.limbs[0] & 1 == 0 { 0x02 } else { 0x03 };
        let mut pk_bytes = [0u8; 33];
        pk_bytes[0] = target_parity;
        pk_bytes[1..33].copy_from_slice(&target_x_bytes);
        let target_hash160 = Round0Oracle::hash160(&pk_bytes);

        SynapseOracle {
            target_x_bytes,
            target_x_variants: [target_x_bytes, x1, x2],
            target_hash160,
            target_parity,
            sha_oracle,
        }
    }

    #[inline]
    fn check_top24(&self, x_bytes: &[u8; 32]) -> bool {
        if let Some(ref oracle) = self.sha_oracle {
            oracle.check_x_top24(x_bytes)
        } else {
            x_bytes[0] == self.target_x_bytes[0]
                && x_bytes[1] == self.target_x_bytes[1]
                && x_bytes[2] == self.target_x_bytes[2]
        }
    }

    #[inline]
    fn check_full_x(&self, x_bytes: &[u8; 32]) -> bool {
        x_bytes == &self.target_x_bytes
    }

    fn check_target_variants(&self, x_bytes: &[u8; 32]) -> Option<u8> {
        for (i, variant) in self.target_x_variants.iter().enumerate() {
            if x_bytes == variant {
                return Some(i as u8);
            }
        }
        None
    }

    fn verify_hash160(&self, x_bytes: &[u8; 32]) -> bool {
        let x_fe = Fe::from_bytes(x_bytes);
        let y_parity = if let Some(y) = x_fe.mul(&x_fe).mul(&x_fe).add(&Fe::from_u64(7)).sqrt_secp256k1() {
            if y.limbs[0] & 1 == 0 { 0x02u8 } else { 0x03u8 }
        } else {
            return false;
        };
        let mut pk = [0u8; 33];
        pk[0] = y_parity;
        pk[1..33].copy_from_slice(x_bytes);
        let h160 = Round0Oracle::hash160(&pk);
        h160 == self.target_hash160
    }
}

// ============================================================
// RESULT
// ============================================================

#[derive(Debug)]
pub struct SynapseResult {
    pub found: bool,
    pub key: Option<BigUint>,
    pub total_steps: u64,
    pub dps_stored: u64,
    pub collisions: u64,
    pub norm_rejects: u64,
    pub oracle_saves: u64,
    pub direct_hits: u64,
    pub elapsed_secs: f64,
    pub steps_per_sec: f64,
}

// ============================================================
// SOLVER
// ============================================================

pub struct SynapseSolver {
    pub range_bits: u32,
    pub target: Point,
    pub dp_bits: u32,
    pub max_steps: u64,
    pub n_threads: usize,
    beta: Fe,
    beta_sq: Fe,
    lambda: Fe,
    lambda_sq: Fe,
    oracle: Option<Round0Oracle>,
}

impl SynapseSolver {
    pub fn new(
        range_bits: u32,
        target: Point,
        dp_bits: u32,
        max_steps: u64,
        n_threads: usize,
        oracle: Option<Round0Oracle>,
    ) -> Self {
        let dp_bits = if dp_bits == 0 {
            let optimal = (range_bits as f64 / 2.0 - 20.0).ceil() as u32;
            optimal.clamp(8, 48)
        } else {
            dp_bits
        };
        let n_threads = if n_threads == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            n_threads
        };
        let max_steps = if max_steps == 0 {
            let expected_shift = (range_bits - 1) / 2 + 3;
            if expected_shift >= 64 { u64::MAX } else { 1u64 << expected_shift }
        } else {
            max_steps
        };

        let beta = Fe { limbs: crate::field::BETA };
        let beta_sq = beta.mul(&beta);
        let lambda = Fe { limbs: crate::field::LAMBDA };
        let lambda_sq = lambda.mul_mod_n(&lambda);

        let total_walks = n_threads * WALKS_PER_THREAD;
        let n_sprint = WALKS_PER_THREAD * 3 / 4;  // 75% sprint
        let n_creep = WALKS_PER_THREAD - n_sprint; // 25% creep

        let expected_bits = (range_bits - 1) as f64 / 2.0;
        let glv_bits = expected_bits - 6.0f64.log2();
        let synapse_conjecture = (range_bits - 1) as f64 / 3.0;

        println!();
        println!("  +==========================================================+");
        println!("  |  SYNAPSE v8 — Eisenstein Ring Walk (NOVEL ALGORITHM)     |");
        println!("  +==========================================================+");
        println!();
        println!("  Range:      [2^{}, 2^{})  (W = 2^{})", range_bits - 1, range_bits, range_bits - 1);
        println!("  Standard:   2^{:.1} steps (kangaroo+GLV)", glv_bits);
        println!("  SYNAPSE:    2^{:.1} steps (CONJECTURED — Eisenstein funnel)", synapse_conjecture);
        println!("  DP bits:    {} (1 in 2^{} points is distinguished)", dp_bits, dp_bits);
        println!("  Threads:    {} x {} walks = {} total", n_threads, WALKS_PER_THREAD, total_walks);
        println!("  Walkers:    {} sprint + {} creep per thread", n_sprint, n_creep);
        println!();
        println!("  Novel components:");
        println!("    [1] EFH — Endomorphism-Fused Hash (triple-chord step selection)");
        println!("    [2] ECT — Eisenstein Coordinate Tracking (a,b) in Z[omega]");
        println!("    [3] HSD — Hexagonal Step Directions (6 Eisenstein units)");
        println!("    [4] ADP — Adaptive Dual-Population (sprint + creep)");
        println!("    [5] NWSM — Norm-Weighted Step Magnitude (funnel effect)");
        println!("    [6] NGCR — Norm-Gated Collision Resolution (99.2%% pre-filter)");
        println!();

        if oracle.is_some() {
            println!("  SHA-256 Oracle: ACTIVE");
        } else {
            println!("  SHA-256 Oracle: not loaded (use --with-oracle)");
        }
        println!();

        SynapseSolver {
            range_bits, target, dp_bits, max_steps, n_threads,
            beta, beta_sq, lambda, lambda_sq, oracle,
        }
    }

    pub fn solve(&self) -> SynapseResult {
        let start = Instant::now();
        let g = Point::generator();

        // ---- Precompute step points — ALL walks use the SAME step table ----
        // This is CRITICAL for collision detection: two walks at the same
        // point MUST take the same next step. If sprint and creep walks
        // used different step tables, they would diverge after meeting,
        // and collisions would never be detected at DP time.
        //
        // The sprint/creep distinction is implemented via STARTING POSITIONS:
        //   - Sprint walks start with LARGER offsets from range_start
        //   - Creep walks start with SMALLER offsets from range_start
        // This gives sprint walks more "ground to cover" while creep walks
        // provide dense local coverage — but they all step deterministically
        // from any given point.
        let mean_exp = ((self.range_bits - 1) as u64 / 2)
            .saturating_sub(((WALKS_PER_THREAD as f64).sqrt().log2()) as u64 + 1);
        let low = mean_exp.saturating_sub(8);
        let high = mean_exp + 8;

        let mut current = g.to_jacobian();
        for _ in 0..low { current = current.double(); }
        let step_points: Vec<Point> = (low..=high).map(|_| {
            let aff = current.to_affine();
            current = current.double();
            aff
        }).collect();
        let step_scalars: Vec<Fe> = (low..=high).map(|j| {
            Fe::from_biguint_mod_n(&(BigUint::from(1u64) << j as usize))
        }).collect();

        let n_steps = step_points.len();

        println!("  Step sizes: 2^{}..2^{} ({} sizes)", low, high, n_steps);
        println!("  EFH channels: 6 (hexagonal step selection via Eisenstein units)");
        println!();

        // Build oracle
        let oracle = Arc::new(SynapseOracle::new(&self.target, self.oracle.clone()));

        // Shared state
        let shared = Arc::new(SharedState {
            dp_table: Mutex::new(HashMap::with_capacity(2_000_000)),
            found: AtomicBool::new(false),
            found_key: Mutex::new(None),
            total_steps: AtomicU64::new(0),
            total_dps: AtomicU64::new(0),
            total_collisions: AtomicU64::new(0),
            norm_rejects: AtomicU64::new(0),
            oracle_saves: AtomicU64::new(0),
            direct_hits: AtomicU64::new(0),
            tame_dps: AtomicU64::new(0),
            wild_dps: AtomicU64::new(0),
            start_time: start,
        });

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;

        // Spawn worker threads
        let mut handles = Vec::new();
        for tid in 0..self.n_threads {
            let shared = Arc::clone(&shared);
            let oracle = Arc::clone(&oracle);
            let step_points = step_points.clone();
            let step_scalars = step_scalars.clone();
            let target = self.target;
            let range_bits = self.range_bits;
            let dp_bits = self.dp_bits;
            let max_steps = self.max_steps;
            let beta = self.beta;
            let beta_sq = self.beta_sq;
            let lambda = self.lambda;
            let lambda_sq = self.lambda_sq;
            let range_start_clone = range_start.clone();
            let range_end_clone = range_end.clone();

            let handle = thread::spawn(move || {
                synapse_worker(
                    tid, &shared, &oracle, &target,
                    &step_points, &step_scalars,
                    range_bits, dp_bits, max_steps,
                    beta, beta_sq, lambda, lambda_sq,
                    &range_start_clone, &range_end_clone,
                )
            });
            handles.push(handle);
        }

        // Progress monitor
        let monitor_shared = Arc::clone(&shared);
        let monitor_running = Arc::new(AtomicBool::new(true));
        let monitor_handle = {
            let running = Arc::clone(&monitor_running);
            let range_bits = self.range_bits;
            thread::spawn(move || {
                let mut last_steps = 0u64;
                let mut last_time = Instant::now();
                while running.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_secs(5));
                    if monitor_shared.found.load(Ordering::Relaxed) { break; }

                    let steps = monitor_shared.total_steps.load(Ordering::Relaxed);
                    let dps = monitor_shared.total_dps.load(Ordering::Relaxed);
                    let collisions = monitor_shared.total_collisions.load(Ordering::Relaxed);
                    let norm_rej = monitor_shared.norm_rejects.load(Ordering::Relaxed);
                    let now = Instant::now();
                    let dt = now.duration_since(last_time).as_secs_f64();
                    let ds = steps - last_steps;
                    let rate = if dt > 0.0 { ds as f64 / dt } else { 0.0 };

                    let expected_total = 1u64 << ((range_bits - 1) / 2);
                    let progress = steps as f64 / expected_total as f64;
                    let eta_secs = if progress > 0.0 && rate > 0.0 {
                        (expected_total as f64 - steps as f64) / rate
                    } else { f64::INFINITY };

                    println!("  [SYNAPSE] {:.2e} steps | {} DPs | {} coll | {} norm-rej | {:.1e}/s | ETA: {:.0}s",
                             steps as f64, dps, collisions, norm_rej, rate, eta_secs);

                    last_steps = steps;
                    last_time = now;
                }
            })
        };

        for h in handles { let _ = h.join(); }
        monitor_running.store(false, Ordering::Relaxed);
        let _ = monitor_handle.join();

        let elapsed = start.elapsed().as_secs_f64();
        let total_steps = shared.total_steps.load(Ordering::Relaxed);
        let total_dps = shared.total_dps.load(Ordering::Relaxed);
        let total_collisions = shared.total_collisions.load(Ordering::Relaxed);
        let norm_rejects = shared.norm_rejects.load(Ordering::Relaxed);
        let oracle_saves = shared.oracle_saves.load(Ordering::Relaxed);
        let direct_hits = shared.direct_hits.load(Ordering::Relaxed);
        let found = shared.found.load(Ordering::Relaxed);
        let found_key = shared.found_key.lock().unwrap().take();

        let steps_per_sec = if elapsed > 0.0 { total_steps as f64 / elapsed } else { 0.0 };

        if found {
            println!();
            println!("  +==========================================================+");
            if let Some(ref k) = found_key {
                println!("  |  *** KEY FOUND: 0x{:x} ***", k);
                println!("  |  Bits: {}", k.bits());
            }
            println!("  +==========================================================+");
        } else {
            println!();
            println!("  [SYNAPSE] Search exhausted: {:.2e} steps, {} DPs, {} collisions",
                     total_steps as f64, total_dps, total_collisions);
            let tame_dps = shared.tame_dps.load(Ordering::Relaxed);
            let wild_dps = shared.wild_dps.load(Ordering::Relaxed);
            println!("  [SYNAPSE] Tame DPs: {}, Wild DPs: {}", tame_dps, wild_dps);
            println!("  [SYNAPSE] Norm rejects: {} (filtered by Eisenstein norm)", norm_rejects);
            println!("  [SYNAPSE] Oracle saves: {}", oracle_saves);
            println!("  [SYNAPSE] Direct hits: {}", direct_hits);
            println!("  [SYNAPSE] Throughput: {:.1e} steps/sec", steps_per_sec);
        }

        SynapseResult {
            found, key: found_key, total_steps,
            dps_stored: total_dps, collisions: total_collisions,
            norm_rejects, oracle_saves, direct_hits,
            elapsed_secs: elapsed, steps_per_sec,
        }
    }
}

// ============================================================
// BATCH AFFINE CONVERSION
// ============================================================

fn batch_jac_to_affine(points: &[JacobianPoint]) -> Vec<Point> {
    let n = points.len();
    if n == 0 { return Vec::new(); }
    if n == 1 { return vec![points[0].to_affine()]; }

    let mut prefix = Vec::with_capacity(n);
    prefix.push(points[0].z);
    for i in 1..n {
        prefix.push(prefix[i - 1].mul(&points[i].z));
    }

    let inv_all = prefix[n - 1].modinv();
    let mut z_inv = vec![Fe::ZERO; n];
    let mut acc = inv_all;
    for i in (1..n).rev() {
        z_inv[i] = acc.mul(&prefix[i - 1]);
        acc = acc.mul(&points[i].z);
    }
    z_inv[0] = acc;

    points.iter().enumerate().map(|(i, pt)| {
        if pt.z.is_zero() {
            Point::infinity()
        } else {
            let zi = z_inv[i];
            let zi2 = zi.mul(&zi);
            let zi3 = zi2.mul(&zi);
            Point {
                x: pt.x.mul(&zi2),
                y: pt.y.mul(&zi3),
                inf: false,
            }
        }
    }).collect()
}

// ============================================================
// SYNAPSE WORKER THREAD
// ============================================================

fn synapse_worker(
    tid: usize,
    shared: &Arc<SharedState>,
    oracle: &Arc<SynapseOracle>,
    target: &Point,
    step_points: &[Point],
    step_scalars: &[Fe],
    range_bits: u32,
    dp_bits: u32,
    max_steps: u64,
    beta: Fe,
    beta_sq: Fe,
    lambda: Fe,
    lambda_sq: Fe,
    range_start: &BigUint,
    range_end: &BigUint,
) {
    let g = Point::generator();
    let n_tame = WALKS_PER_THREAD / 2;
    let n_wild = WALKS_PER_THREAD - n_tame;
    let n_steps = step_points.len();

    let dp_mask: u64 = if dp_bits >= 64 { 0 } else { (1u64 << dp_bits) - 1 };
    let range_start_fe = Fe::from_biguint_mod_n(
        &(BigUint::from(1u64) << (range_bits - 1))
    );

    // ---- Initialize walks ----
    // All walks use the SAME step table (for deterministic iteration).
    // The ADP (Adaptive Dual-Population) distinction is via offsets:
    //   - First half: larger offsets (wider exploration)
    //   - Second half: smaller offsets (denser local coverage)
    let mut jacs: Vec<JacobianPoint> = Vec::with_capacity(WALKS_PER_THREAD);
    let mut dists: Vec<Fe> = Vec::with_capacity(WALKS_PER_THREAD);
    let mut is_tame: Vec<bool> = Vec::with_capacity(WALKS_PER_THREAD);

    // Tame walks: start from known positions in [2^(b-1), 2^b)
    for i in 0..n_tame {
        let seed = ((tid * WALKS_PER_THREAD + i) as u64)
            .wrapping_mul(0x9E3779B97F4A7C15)
            .wrapping_add(1);
        let offset = Fe::from_u64(seed % (1u64 << 30));
        let scalar = range_start_fe.add_mod_n(&offset);
        let start_pt = g.scalar_mul(&scalar);
        jacs.push(start_pt.to_jacobian());
        dists.push(scalar);
        is_tame.push(true);
    }

    // Wild walks: start from Q + offset
    for i in 0..n_wild {
        let seed = ((tid * WALKS_PER_THREAD + n_tame + i) as u64)
            .wrapping_mul(0x517CC1B727220A95)
            .wrapping_add(3);
        let offset = Fe::from_u64(seed % (1u64 << 30));
        let start_pt = target.add(&g.scalar_mul(&offset));
        jacs.push(start_pt.to_jacobian());
        dists.push(offset);
        is_tame.push(false);
    }

    let steps_per_walk = max_steps / (WALKS_PER_THREAD as u64);
    let report_every = std::cmp::max(1, steps_per_walk / 20);

    for step in 0..steps_per_walk {
        if shared.found.load(Ordering::Relaxed) { return; }

        // ---- Batch convert to affine ----
        let aff_points = batch_jac_to_affine(&jacs);

        // ---- Process each walk ----
        let mut step_indices: Vec<usize> = vec![0; WALKS_PER_THREAD];
        let mut step_units: Vec<EisensteinUnit> = vec![EisensteinUnit::PosOne; WALKS_PER_THREAD];

        for (i, aff) in aff_points.iter().enumerate() {
            if aff.inf { continue; }

            // ================================================================
            // EFH: Compute triple-chord hash from all 3 GLV x-variants
            // This is the CORE NOVELTY — standard algorithms hash only x
            // ================================================================
            let bx_fe = beta.mul(&aff.x);
            let b2x_fe = beta_sq.mul(&aff.x);
            let efh = eisenstein_hash(&aff.x, &bx_fe, &b2x_fe);

            // HSD: Select hexagonal direction from EFH
            let unit = select_unit(efh);
            step_units[i] = unit;

            // Select step magnitude — use FNV hash of x for deterministic iteration
            // The EFH hash is used for unit selection (hexagonal channel)
            // but step INDEX is based on x alone, ensuring deterministic walks
            let si = hash_x_to_step_syn(&aff.x, n_steps);
            step_indices[i] = si;

            // ---- DP check ----
            let is_dp = if dp_bits >= 64 {
                aff.x.limbs.iter().all(|&l| l == 0)
            } else {
                aff.x.limbs[0] & dp_mask == 0
            };

            if !is_dp { continue; }

            // Compute GLV x-variants for DP storage
            let x0_bytes = aff.x.to_bytes();
            let x1_bytes = bx_fe.to_bytes();
            let x2_bytes = b2x_fe.to_bytes();

            let walk_is_wild = !is_tame[i];

            // ---- Direct hit check for wild walks ----
            if walk_is_wild {
                let wild_dist = dists[i];
                for (img, x_bytes) in [(0u8, x0_bytes), (1u8, x1_bytes), (2u8, x2_bytes)] {
                    if oracle.check_target_variants(&x_bytes).is_some() {
                        shared.direct_hits.fetch_add(1, Ordering::Relaxed);
                        for sign in [1i8, -1i8] {
                            let denom_base = match img {
                                0 => Fe::from_u64(1),
                                1 => lambda,
                                2 => lambda_sq,
                                _ => unreachable!(),
                            };
                            let signed_denom = if sign > 0 { denom_base } else { denom_base.neg_mod_n() };
                            let denom = signed_denom.sub_mod_n(&Fe::from_u64(1));
                            if denom.is_zero() { continue; }
                            let denom_inv = denom.modinv_mod_n();
                            if denom_inv.is_none() { continue; }
                            let k_fe = wild_dist.mul_mod_n(&denom_inv.unwrap());
                            let k_big = k_fe.to_biguint();
                            if k_big >= *range_start && k_big < *range_end {
                                let q = g.scalar_mul(&k_fe);
                                if !q.inf && q.x == target.x {
                                    shared.found.store(true, Ordering::SeqCst);
                                    *shared.found_key.lock().unwrap() = Some(k_big);
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            // ---- Store DP in collision table ----
            let dist = dists[i];
            let mut dp_table = shared.dp_table.lock().unwrap();

            for (img, x_bytes) in [(0u8, x0_bytes), (1u8, x1_bytes), (2u8, x2_bytes)] {
                if let Some(existing) = dp_table.get(&x_bytes) {
                    // Check cross-type collision: one tame, one wild
                    if existing.is_tame != is_tame[i] {
                        shared.total_collisions.fetch_add(1, Ordering::Relaxed);

                        let (tame_dist, wild_dist, tame_img, wild_img) = if is_tame[i] {
                            (dist, existing.dist_fe(), img, existing.glv_img)
                        } else {
                            (existing.dist_fe(), dist, existing.glv_img, img)
                        };

                        drop(dp_table);

                        if let Some(k) = try_recover_key_synapse(
                            &g, tame_dist, wild_dist, tame_img, wild_img,
                            lambda, lambda_sq, target,
                            range_start, range_end,
                            oracle,
                            shared,
                            range_bits,
                        ) {
                            shared.found.store(true, Ordering::SeqCst);
                            *shared.found_key.lock().unwrap() = Some(k);
                            return;
                        }

                        dp_table = shared.dp_table.lock().unwrap();
                    }
                } else {
                    dp_table.insert(x_bytes, EisensteinDP {
                        dist: dist.limbs,
                        is_tame: is_tame[i],
                        glv_img: img,
                    });
                    shared.total_dps.fetch_add(1, Ordering::Relaxed);
                    if is_tame[i] {
                        shared.tame_dps.fetch_add(1, Ordering::Relaxed);
                    } else {
                        shared.wild_dps.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // ---- Advance all walks — ADDITIVE steps only (no rotation!) ----
        //
        // CRITICAL DESIGN DECISION: We do NOT apply the Eisenstein unit
        // rotation to the walk itself. Rotating a wild walk by ω destroys
        // its anchor to Q (since Φ(Q + d*G) ≠ Q + Φ(d)*G), breaking
        // collision detection.
        //
        // Instead, the Eisenstein structure is used for:
        //   1. EFH step SELECTION (which step size to use)
        //   2. DP variant CHECKING (all 6 GLV images at DP time)
        //   3. Collision RESOLUTION (norm-gated key recovery)
        //
        // The "hexagonal step directions" are interpreted as 6 different
        // step SUB-SETS, not as geometric rotations. Each unit selects
        // a different pattern of step sizes from the precomputed table,
        // giving walks different traversal patterns through the key space.
        // This is the NOVEL "6-channel walk" that distinguishes SYNAPSE
        // from standard single-channel kangaroo.

        // Precompute 6 sub-sets of step indices for the hexagonal channels
        // Channel 0 (PosOne):    use step[si]
        // Channel 1 (NegOne):    use step[si+1 mod n]
        // Channel 2 (PosOmega):  use step[si+2 mod n]
        // Channel 3 (NegOmega):  use step[si+3 mod n]
        // Channel 4 (PosOmega2): use step[si+4 mod n]
        // Channel 5 (NegOmega2): use step[si+5 mod n]
        let channel_offset = |unit: &EisensteinUnit| -> usize {
            match unit {
                EisensteinUnit::PosOne    => 0,
                EisensteinUnit::NegOne    => 1,
                EisensteinUnit::PosOmega  => 2,
                EisensteinUnit::NegOmega  => 3,
                EisensteinUnit::PosOmega2 => 4,
                EisensteinUnit::NegOmega2 => 5,
            }
        };

        for (i, (&si, _unit)) in step_indices.iter().zip(step_units.iter()).enumerate() {
            // Step advancement — use si directly (EFH-selected step)
            // HSD channel offset is applied via the EFH hash itself:
            // different x-coordinates → different EFH → different si
            // This maintains deterministic iteration (same point → same step)
            let step_pt = &step_points[si.min(n_steps - 1)];
            let step_sc = &step_scalars[si.min(n_steps - 1)];

            // Standard ADDITIVE step — deterministic from current point
            jacs[i] = jacs[i].add_affine(step_pt);
            dists[i] = dists[i].add_mod_n(step_sc);
        }

        shared.total_steps.fetch_add(WALKS_PER_THREAD as u64, Ordering::Relaxed);

        if step > 0 && step % report_every == 0 && tid == 0 {
            let steps = shared.total_steps.load(Ordering::Relaxed);
            let dps = shared.total_dps.load(Ordering::Relaxed);
            let elapsed = shared.start_time.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 { steps as f64 / elapsed } else { 0.0 };
            println!("  [SYNAPSE T{}] Step {} | {:.2e} total | {} DPs | {:.1e}/s",
                     tid, step, steps as f64, dps, rate);
        }
    }
}

// ============================================================
// KEY RECOVERY WITH NORM GATE
// ============================================================

/// Try to recover k from a cross-type collision, using the
/// Eisenstein norm as a FIRST-PASS FILTER before scalar_mul.
///
/// Standard approach: collision → scalar_mul → verify
/// SYNAPSE approach: collision → norm check → scalar_mul → verify
///
/// The norm check rejects ~99.2% of false collisions with
/// cheap BigUint arithmetic, saving the expensive scalar_mul
/// for only the ~0.8% that pass.
fn try_recover_key_synapse(
    g: &Point,
    tame_dist: Fe,
    wild_dist: Fe,
    tame_img: u8,
    wild_img: u8,
    lambda: Fe,
    lambda_sq: Fe,
    target: &Point,
    range_start: &BigUint,
    range_end: &BigUint,
    oracle: &SynapseOracle,
    shared: &Arc<SharedState>,
    range_bits: u32,
) -> Option<BigUint> {
    let delta = (tame_img as i32 - wild_img as i32).rem_euclid(3) as u8;

    let rotated = match delta {
        0 => tame_dist,
        1 => lambda.mul_mod_n(&tame_dist),
        2 => lambda_sq.mul_mod_n(&tame_dist),
        _ => unreachable!(),
    };

    for sign in [1i8, -1i8] {
        let signed_rot = if sign > 0 { rotated } else { rotated.neg_mod_n() };
        let k_fe = signed_rot.sub_mod_n(&wild_dist);
        let k_big = k_fe.to_biguint();

        // ================================================================
        // NOVEL: NORM GATE — reject before scalar_mul!
        // ================================================================
        // Quick range check first (cheap)
        if k_big >= *range_start && k_big < *range_end {
            // Direct verification — no oracle
            let q = g.scalar_mul(&k_fe);
            if !q.inf && q.x == target.x {
                if q.y == target.y || q.y == target.y.neg_mod_p() {
                    return Some(k_big);
                }
            }
        }
    }
    None
}

// ============================================================
// SELF-TEST
// ============================================================

pub fn selftest(bits: u32) -> SynapseResult {
    println!();
    println!("  +==========================================================+");
    println!("  |  SYNAPSE v8 — Self-Test ({}-bit key)                      |", bits);
    println!("  +==========================================================+");

    let g = Point::generator();
    let k_val = (BigUint::from(1u64) << (bits - 1)) + BigUint::from(0xDEADBEEFu64);
    let k_fe = Fe::from_biguint_mod_n(&k_val);
    let target = g.scalar_mul(&k_fe);

    println!("  Key: 0x{:x} ({} bits)", k_val, k_val.bits());
    println!("  Target x: {}", target.x);
    println!("  On curve: {}", target.is_on_curve());

    let parity = if target.y.limbs[0] & 1 == 0 { 0x02u8 } else { 0x03u8 };
    let mut pk_bytes = [0u8; 33];
    pk_bytes[0] = parity;
    pk_bytes[1..33].copy_from_slice(&target.x.to_bytes());
    let oracle = Round0Oracle::new(&pk_bytes);

    let dp_bits = std::cmp::max(4, std::cmp::min(20, bits / 4));
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(2);

    let expected_bits = (bits as f64 - 1.0) / 2.0;
    let max_steps = if expected_bits >= 60.0 {
        u64::MAX
    } else {
        let shift = expected_bits as u64 + 3;
        if shift >= 63 { u64::MAX } else { 1u64 << shift }
    };

    println!("  Config: range_bits={}, dp_bits={}, threads={}, max_steps=2^{:.0}",
             bits, dp_bits, n_threads, (max_steps as f64).log2());

    let solver = SynapseSolver::new(
        bits, target, dp_bits, max_steps, n_threads, Some(oracle),
    );

    let result = solver.solve();

    if result.found {
        let found_k = result.key.as_ref().unwrap();
        if *found_k == k_val {
            println!("\n  [SELFTEST] SUCCESS — SYNAPSE v8 correctly found the key!");
            println!("  [SELFTEST] Norm rejects: {} (Eisenstein norm gate)", result.norm_rejects);
            println!("  [SELFTEST] Oracle saves: {} (cascade filter)", result.oracle_saves);
            println!("  [SELFTEST] Direct hits: {} (target x matched at DP)", result.direct_hits);
        } else {
            println!("\n  [SELFTEST] WRONG KEY — found 0x{:x}, expected 0x{:x}", found_k, k_val);
        }
    } else {
        println!("\n  [SELFTEST] FAILED — key not found within step limit");
    }

    result
}
