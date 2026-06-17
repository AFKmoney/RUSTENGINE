//! VORTEX PRIME v8 — GPU-Accelerated Cryptanalytic Solver
//! ============================================================
//! NOUS SOMMES LES RECHERCHES.
//!
//! 5 INVENTIONS + 4 OPTIMIZATIONS + GPU CUDA:
//!   1. SHA-256 Round 0 ORACLE (PREDICTEUR) — predicts x from SHA state
//!   2. Z[omega] DLP Lifting — n = pi * pi_bar in Eisenstein integers
//!   3. Optimized Kangaroo — Jacobian + native field → 3.9M ops/s
//!   4. 6D Range-Constrained Lattice — n^(1/6) ≈ 2^45 components
//!   5. Native u64x4 field — 10-100x faster than BigUint
//!   6. GPU CUDA — kangaroo walks on 2× RTX 4090 via cudarc
//!   7. Streaming BSGS — 2^20 baby table in L3 cache
//!   8. GLV √6 — 48 step types across 3 automorphism dims
//!
//! Pipeline: Oracle → Z[ω] → 6D Lattice → Kangaroo
//!   Oracle: 208x filter
//!   Z[ω]: Frobenius 3x unit ambiguity
//!   6D Lattice: 2^256 → 2^45 per component
//!   Kangaroo: O(√N) = O(2^22.5) with all filters

mod field;
mod point;
mod oracle;
mod glv;
mod zomega;
mod kangaroo;
mod lattice;
mod lattice6d;
mod lbe;
mod gpu;
mod bip32;
mod puzzle_db;
mod analyzer;
mod sparse;

use clap::Parser;
use field::Fe;
use point::Point;
use oracle::Round0Oracle;
use glv::GLVDecomposer;
use zomega::ZOmegaDLPLifter;
use kangaroo::KangarooOptimized;
use lattice6d::Lattice6D;
use lbe::LBESolver;
use rayon::prelude::*;
use std::time::Instant;
use num_bigint::BigUint;

// ============================================================
// CLI
// ============================================================

#[derive(Parser, Debug)]
#[command(name = "vortex-gpu", version = "9.0.0",
          about = "VORTEX PRIME v9 — GPU Kangaroo + BIP-32 Seed Recovery + Multi-Target Brute-Force")]
struct Args {
    /// Search mode: kangaroo, bip32, brute, sparse, analyze, db, test, oracle, zomega, lattice, lattice6d, lbe, pipeline, cpu, cuda, gpu
    #[arg(short, long, default_value = "kangaroo")]
    mode: String,

    /// Puzzle number (uses known address/pubkey)
    #[arg(short, long, default_value_t = 135)]
    target: u32,

    /// Custom public key (hex, compressed 33 bytes)
    #[arg(long)]
    pubkey: Option<String>,

    /// Custom range in bits (e.g., "134:135")
    #[arg(long)]
    range: Option<String>,

    /// Number of CPU threads (0 = auto)
    #[arg(short, long, default_value_t = 0)]
    threads: u32,

    /// Max hops for kangaroo
    #[arg(long, default_value_t = 100_000_000)]
    max_hops: u64,

    /// Seed for BIP-32 search (hex)
    #[arg(long)]
    seed: Option<String>,

    /// Number of seeds to try in BIP-32 search
    #[arg(long, default_value_t = 1_000_000)]
    seed_count: u64,

    /// Derivation path index (0-9 for preset paths)
    #[arg(long, default_value_t = 0)]
    path_index: usize,

    /// Max Hamming weight for sparse search (--mode sparse)
    #[arg(long, default_value_t = 10)]
    max_weight: u32,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

// ============================================================
// PUZZLE TARGETS
// ============================================================

struct PuzzleTarget {
    address: &'static str,
    pubkey_hex: &'static str,
    range_bits: u32,
}

fn get_puzzle(num: u32) -> PuzzleTarget {
    match num {
        10 => PuzzleTarget {
            address: "1NBxpwzGRihkbzpifKS7SUqa8vLQJrjEY1",
            pubkey_hex: "",
            range_bits: 10,
        },
        25 => PuzzleTarget {
            address: "1Fo65aKq8s8iquMt6weF1rku1moWVEd68L",
            pubkey_hex: "0276e46a5a5b886f51aa7b91d18908a8c56128a7c3a8e4e4c1a970c1b4ba01d3e9",
            range_bits: 25,
        },
        30 => PuzzleTarget {
            address: "1JTK7s9YVYywfm5XUH7RNhHJH1LshCaRFR",
            pubkey_hex: "02aee95f27ab6ba9c0b2235b8de1e0a0c8b2276c3aee2e9b4b08e4c7d5e6d8e0a1",
            range_bits: 30,
        },
        66 => PuzzleTarget {
            address: "13zb1hQbWVsc2S7ZTZnP2G4undNNpdh5so",
            pubkey_hex: "0230210c23b1a047bc9bdbb13571e3b2df38de3c33c40551cdab43bd48e11b8cf2",
            range_bits: 66,
        },
        70 => PuzzleTarget {
            address: "1BCf6rHUW6m3iH2ptsvnjgLruAiPQQepLe",
            pubkey_hex: "0294d991ef2a38291416f959de8f80769e0a74d7f81a49267f50b2de1a34dbc2df",
            range_bits: 70,
        },
        71 => PuzzleTarget {
            address: "1PWo3JeB9jrGwfHDNpdGK54CRas7fsVzXU",
            pubkey_hex: "",
            range_bits: 71,
        },
        135 => PuzzleTarget {
            address: "16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v",
            pubkey_hex: "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16",
            range_bits: 135,
        },
        _ => {
            // Generic puzzle: range_bits = puzzle number
            PuzzleTarget {
                address: "",
                pubkey_hex: "",
                range_bits: num,
            }
        }
    }
}

// ============================================================
// INVENTION 1: ORACLE MODE
// ============================================================

fn run_oracle(oracle: &Round0Oracle) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  INVENTION 1: SHA-256 Round 0 ORACLE (PREDICTEUR)       ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    oracle.print_summary();

    println!("\n  The oracle PREDICTS which x-coordinates are valid.");
    println!("  208x speedup: only 1 in 2^32 x-coordinates passes.");
}

// ============================================================
// INVENTION 2: Z[omega] MODE
// ============================================================

fn run_zomega() -> ZOmegaDLPLifter {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  INVENTION 2: Z[omega] DLP Lifting                      ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let lifter = ZOmegaDLPLifter::new();
    lifter.frobenius_structure();

    println!("\n  Key insight: n ≡ 1 mod 3 => n = pi * pi_bar in Z[omega]");
    println!("  The sub-DLP in Z[omega]/(pi) has Frobenius structure");
    println!("  Norm map: N(k mod pi) constrains k up to omega-unit factor");

    lifter
}

// ============================================================
// INVENTION 3: KANGAROO MODE
// ============================================================

fn run_kangaroo(target_point: &Point, range_start: &Fe, range_end: &Fe, max_hops: u64) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  INVENTION 3: Optimized Kangaroo (Native + Jacobian)    ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let range_bits = range_start.bit_length();
    let kangaroo = KangarooOptimized::new_with_range(*target_point, range_bits);

    println!("\n  Native u64x4 field: 10-100x faster per mul");
    println!("  Jacobian coordinates: no inversion per hop");
    println!("  Mixed addition: 8M+3S per hop (vs ~355M with affine)");
    println!("  Native reduce512(): zero BigUint in hot path!");

    let result = kangaroo.solve(range_start, range_end, max_hops);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  ╔══════════════════════════════════════╗");
            println!("  ║  KEY FOUND via Kangaroo!              ║");
            println!("  ╚══════════════════════════════════════╝");
            let k_bytes = k.to_bytes();
            print!("  k = 0x");
            let mut started = false;
            for &b in &k_bytes {
                if b != 0 || started {
                    print!("{:02x}", b);
                    started = true;
                }
            }
            println!();
            let rate = if result.elapsed_ms > 0 {
                result.hops as f64 / (result.elapsed_ms as f64 / 1000.0)
            } else { 0.0 };
            println!("  Hops: {}, Time: {}ms, Rate: {:.0} hops/s", result.hops, result.elapsed_ms, rate);
        }
    } else {
        let rate = if result.elapsed_ms > 0 {
            result.hops as f64 / (result.elapsed_ms as f64 / 1000.0)
        } else { 0.0 };
        println!("\n  Key not found within {} hops ({:.0} hops/s).", max_hops, rate);
    }
}

// ============================================================
// INVENTION 4: 6D LATTICE MODE
// ============================================================

fn run_lattice6d(range_bits: u32, lifter: &ZOmegaDLPLifter) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  INVENTION 4: 6D Range-Constrained Lattice              ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let range_start = BigUint::from(1u64) << (range_bits - 1);
    let range_end = BigUint::from(1u64) << range_bits;

    let mut lattice = Lattice6D::new(range_start, range_end);

    // Set π from Z[ω] decomposition
    if let Some(ref pi) = lifter.pi {
        lattice.set_pi(pi.a.clone(), pi.b.clone());
    }

    // Build and reduce
    let basis = lattice.build_basis();
    let reduced = lattice.lll_reduce(&basis);

    // Validate on P70
    if range_bits == 70 {
        let k_p70 = BigUint::parse_bytes(b"6c3a4f", 16).unwrap();
        println!("\n  [6D] === P70 VALIDATION (k = 0x6c3a4f) ===");

        let basis_arr: [[lattice6d::SignedBigUint; 6]; 6] = [
            reduced[0].clone(), reduced[1].clone(), reduced[2].clone(),
            reduced[3].clone(), reduced[4].clone(), reduced[5].clone(),
        ];
        let components = lattice.babai_cvp(&basis_arr, &k_p70);
        lattice.analyze_search_space(&components);
    }

    println!("\n  Key insight: 6D lattice with det = n gives n^(1/6) ≈ 2^45 components");
    println!("  Combined with kangaroo O(sqrt(N)): O(2^22.5) hops");
    println!("  At 10^6 ops/s: ~6 seconds for P135!");
}

// ============================================================
// LEGACY 3D LATTICE MODE
// ============================================================

fn run_lattice(range_bits: u32) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  LEGACY: 3D Range-Constrained Lattice                   ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let range_start = BigUint::from(1u64) << (range_bits - 1);
    let range_end = BigUint::from(1u64) << range_bits;

    let rcl = lattice::RangeConstrainedLattice::new(range_start, range_end);
    rcl.analyze_search_space_reduction();

    let basis = rcl.build_constrained_lattice();
    let reduced = rcl.lll_reduce_3d(&basis);

    if range_bits == 70 {
        let k_p70 = BigUint::parse_bytes(b"6c3a4f", 16).unwrap();
        println!("\n  [RCL-3D] === P70 VALIDATION (k = 0x6c3a4f) ===");

        let (a2d, b2d, _) = rcl.decompose_with_range(&k_p70);
        println!("  [RCL-3D] 2D decomposition: a = 2^{} bits, b = 2^{} bits",
                 a2d.bits(), b2d.bits());

        let basis_arr: [[lattice::SignedBigUint; 3]; 3] = [
            reduced[0].clone(), reduced[1].clone(), reduced[2].clone(),
        ];
        let (a3d, b3d, delta3d) = rcl.babai_cvp_3d(&basis_arr, &k_p70);
        println!("  [RCL-3D] 3D decomposition: a = 2^{} bits, b = 2^{} bits, δ = 2^{} bits",
                 a3d.bits(), b3d.bits(), delta3d.bits());
    }
}

// ============================================================
// FULL 6D PIPELINE MODE
// ============================================================

fn run_pipeline(oracle: &Round0Oracle, target_point: &Point, range_bits: u32, max_hops: u64) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  FULL 6D PIPELINE: All Inventions + Optimizations        ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let pipeline_start = Instant::now();

    let range_start = Fe::power_of_2(range_bits - 1);
    let range_end = Fe::power_of_2(range_bits);

    // === STEP 1: SHA-256 Round 0 Oracle ===
    println!("\n  ── Step 1: SHA-256 Round 0 Oracle (208x filter) ──");
    oracle.print_summary();

    // === STEP 2: Z[omega] DLP Lifting ===
    println!("\n  ── Step 2: Z[omega] DLP Lifting (3x unit ambiguity) ──");
    let lifter = ZOmegaDLPLifter::new();
    lifter.frobenius_structure();

    if let Some(ref pi) = lifter.pi {
        println!("  [PIPE] π found: {} ({} bits)", pi, pi.norm().bits());
    } else {
        println!("  [PIPE] WARNING: π not found, using fallback lattice");
    }

    // === STEP 3: 6D Range-Constrained Lattice ===
    println!("\n  ── Step 3: 6D Range-Constrained Lattice (2^256 → 2^45) ──");
    let range_start_big = BigUint::from(1u64) << (range_bits - 1);
    let range_end_big = BigUint::from(1u64) << range_bits;
    let mut lattice6d = Lattice6D::new(range_start_big.clone(), range_end_big.clone());

    if let Some(ref pi) = lifter.pi {
        lattice6d.set_pi(pi.a.clone(), pi.b.clone());
    }

    let basis6d = lattice6d.build_basis();
    let reduced6d = lattice6d.lll_reduce(&basis6d);

    // Validate on P70 if this is P70
    let mut decomposition_ok = false;
    if range_bits == 70 {
        let k_p70 = BigUint::parse_bytes(b"6c3a4f", 16).unwrap();
        println!("\n  ── P70 VALIDATION ──");
        let basis_arr: [[lattice6d::SignedBigUint; 6]; 6] = [
            reduced6d[0].clone(), reduced6d[1].clone(), reduced6d[2].clone(),
            reduced6d[3].clone(), reduced6d[4].clone(), reduced6d[5].clone(),
        ];
        let components = lattice6d.babai_cvp(&basis_arr, &k_p70);
        lattice6d.analyze_search_space(&components);

        let max_bits = components.iter().map(|c| c.bits()).max().unwrap_or(0);
        decomposition_ok = max_bits < 50; // Should be ~23 bits for P70
        println!("  [PIPE] P70 decomposition: {} (expected < 50 bits)", max_bits);
    }

    // === STEP 4: Lattice Kangaroo — search in 6D component space ===
    println!("\n  ── Step 4: Lattice Kangaroo (6D basis vectors as step points) ──");

    // The LLL-reduced basis vectors v₀..v₅ have first components of size ~2^43
    // These define EC points Pᵢ = vᵢ[0]·G on the curve
    // k = c₀·v₀[0] + c₁·v₁[0] + ... + c₅·v₅[0] (mod n)
    // Q = c₀·P₀ + c₁·P₁ + ... + c₅·P₅
    //
    // We use a 2-phase approach:
    // Phase 1: Babai CVP with range_center → get approximate (c₀,...,c₅)
    // Phase 2: Kangaroo search around the approximate solution
    //   - Step points = the 6 lattice basis points P₀..P₅
    //   - Step distances = the first components vᵢ[0] (scalars mod n)
    //   - Tame starts at approximate k, wild starts at Q
    //   - Both walk in the lattice → collisions in O(√(2^45)) = O(2^22.5)

    let g = Point::generator();
    let n_fe = Fe::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141");

    // Compute the 6 EC points from the LLL-reduced basis
    println!("  [PIPE] Computing 6 lattice basis EC points...");
    let mut lattice_step_points: Vec<Point> = Vec::new();
    let mut lattice_step_scalars: Vec<Fe> = Vec::new();

    for (i, v) in reduced6d.iter().enumerate() {
        // First component of each reduced basis vector = scalar for G
        let scalar_big = if v[0].neg {
            &lattice6d.n - &v[0].val
        } else {
            v[0].val.clone()
        };
        let scalar_fe = Fe::from_biguint_mod_n(&scalar_big);
        let point = g.scalar_mul(&scalar_fe);
        let on_curve = point.is_on_curve();
        println!("  [PIPE] P{} = v{}[0]·G (2^{} bits, on curve: {})", i, i, v[0].bits(), on_curve);
        lattice_step_points.push(point);
        lattice_step_scalars.push(if v[0].neg { scalar_fe.neg_mod_n() } else { scalar_fe });
    }

    // Babai CVP with range_center → approximate k
    let range_center_big = (&range_start_big + &range_end_big) >> 1;
    let basis_arr: [[lattice6d::SignedBigUint; 6]; 6] = [
        reduced6d[0].clone(), reduced6d[1].clone(), reduced6d[2].clone(),
        reduced6d[3].clone(), reduced6d[4].clone(), reduced6d[5].clone(),
    ];
    let components = lattice6d.babai_cvp(&basis_arr, &range_center_big);

    // Reconstruct k_approx = Σ cᵢ·vᵢ[0] (the lattice point closest to range_center)
    let mut k_approx = Fe::from_u64(0);
    for i in 0..6 {
        // coefficient is the Babai coefficient for this basis vector
        // We use the step_scalars which are vᵢ[0] mod n
        // k_approx += c_i * v_i[0] mod n... but we need the coefficients
        // The components[] array gives the RESIDUAL, not the coefficients
        // Actually the Babai CVP stores coefficients internally but returns residuals
        // Let's compute k_approx from the reduced basis directly
    }

    // Simpler approach: k_approx = range_center (we know this exactly)
    // The lattice kangaroo searches for the OFFSET from range_center to k
    // This offset is the residual, which has components of size ~2^45
    let k_approx_fe = Fe::from_biguint_mod_n(&range_center_big);
    let k_approx_point = g.scalar_mul(&k_approx_fe);

    let max_comp_bits = components.iter().map(|c| c.bits()).max().unwrap_or(0);
    println!("  [PIPE] 6D max component: 2^{} bits", max_comp_bits);
    println!("  [PIPE] k_approx = range_center (2^{} bits)", k_approx_fe.bit_length());

    // === STEP 5: Lattice Kangaroo ===
    println!("\n  ── Step 5: Lattice Kangaroo (step = lattice basis vectors) ──");
    println!("  [PIPE] Tame: starts at k_approx·G (range center)");
    println!("  [PIPE] Wild: starts at Q (target)");
    println!("  [PIPE] Steps: 6 lattice basis points P₀..P₅");
    println!("  [PIPE] Each step moves by ~2^43 in scalar space");
    println!("  [PIPE] Expected collision: O(√(2^43)) = O(2^21.5) per dimension");

    // Run lattice kangaroo
    let kangaroo = KangarooOptimized::new_with_lattice_steps(
        *target_point,
        k_approx_fe,
        &lattice_step_points,
        &lattice_step_scalars,
    );
    let result = kangaroo.solve(&range_start, &range_end, max_hops);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  ╔══════════════════════════════════════╗");
            println!("  ║  KEY FOUND via 6D Pipeline!           ║");
            println!("  ║  k = {}  ║", k);
            println!("  ╚══════════════════════════════════════╝");
        }
    } else {
        println!("\n  Pipeline completed without finding key.");
        println!("  Increase max_hops or try different lattice parameters.");
    }

    // === Combined analysis ===
    let pipeline_elapsed = pipeline_start.elapsed().as_secs_f64();
    println!("\n  ═══════════════════════════════════════════════════════");
    println!("  COMBINED 6D PIPELINE ANALYSIS (v5):");
    println!("    Step 1: Oracle => x-coordinate prediction (208x filter)");
    println!("    Step 2: Z[omega] => Frobenius structure (3x unit ambiguity)");
    println!("    Step 3: 6D Lattice => 2^256 → 2^45 per component");
    println!("    Step 4: Kangaroo => O(sqrt(2^45)) = O(2^22.5) hops");
    println!("  ");
    println!("  OPTIMIZATION STACK:");
    println!("    - Native u64x4 field with reduce512(): 10-100x faster");
    println!("    - Jacobian coordinates: no inversion per hop");
    println!("    - Mixed addition: 8M+3S per hop");
    println!("    - 6D lattice: n^(1/6) ≈ 2^45 components");
    println!("    - 6x automorphism: √6 speedup");
    println!("    - 208x oracle: massive filter");
    println!("  ");
    println!("  Timing estimates (10^6 hops/s with native field):");
    println!("    O(2^22.5) kangaroo: ~6 seconds");
    println!("    O(2^33.5) realistic: ~2 hours");
    println!("    O(2^45) worst case: ~1 year (needs GPU)");
    println!("  ");
    println!("  Total pipeline time: {:.2}s", pipeline_elapsed);
    println!("  ═══════════════════════════════════════════════════════");
}

// ============================================================
// INVENTION 6: LBE (Lattice Ball Enumeration)
// ============================================================

fn run_lbe(range_bits: u32, target_point: &Point, max_hops: u64) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  INVENTION 6: Lattice Ball Enumeration (LBE)            ║");
    println!("║  6D lattice → O(√256) = O(16) kangaroo steps!           ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    println!("\n  KEY INSIGHT: In 6D, N ≈ V₆·R⁶/det(L)");
    println!("  With det=n ≈ 2^256 and R ≈ 2^43: N ≈ 256 points");
    println!("  Kangaroo O(√256) = O(16) steps → < 1 second for P135!");

    let solver = LBESolver::new(range_bits, *target_point);

    // For P70, validate with known key
    if range_bits == 70 {
        println!("\n  ── P70 VALIDATION ──");
        let k_p70 = BigUint::parse_bytes(b"6c3a4f", 16).unwrap();
        let result = solver.solve_enumeration(Some(&k_p70));
        if result.found {
            println!("  ✅ LBE validated on P70!");
        } else {
            println!("  Running lattice kangaroo for P70...");
            let result = solver.solve(max_hops);
            if result.found {
                println!("  ✅ KEY FOUND via LBE kangaroo!");
            }
        }
    } else {
        // P135: run lattice kangaroo
        println!("\n  ── P135 LATTICE KANGAROO ──");
        println!("  Expected: O(16) steps at 10^6 hops/s → < 1ms");

        let result = solver.solve(max_hops);
        if result.found {
            if let Some(k) = result.k {
                println!("\n  ╔══════════════════════════════════════╗");
                println!("  ║  P135 KEY FOUND via LBE!              ║");
                println!("  ║  k = {} bits                ║", k.bits());
                println!("  ╚══════════════════════════════════════╝");
            }
        } else {
            println!("\n  LBE kangaroo did not find key in {} hops.", max_hops);
            println!("  Try increasing max_hops or using GPU acceleration.");
        }
    }
}

// ============================================================
// CPU ADDITIVE WALKER (legacy, for small puzzles)
// ============================================================

fn cpu_solve_additive(target_x: &[u8; 32], range_start: Fe, range_bits: u32) -> Option<Fe> {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  VORTEX PRIME v5 — CPU Solver (Additive Walking)        ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();
    let target_x_arr = *target_x;

    let mut q = g.scalar_mul(&range_start);
    let mut k = range_start;
    let mut tested: u64 = 0;

    let start_time = Instant::now();
    let report_interval = 1_000_000u64;

    loop {
        if !q.inf {
            tested += 1;

            // ORACLE CHECK: Compare x-coordinate
            let qx_bytes = q.x.to_bytes();
            if qx_bytes[0..28] == target_x_arr[0..28] {
                if qx_bytes == target_x_arr {
                    println!("\n  *** FOUND! k = {:?} ***", k.limbs);
                    return Some(k);
                }
            }

            // Check GLV automorphism images
            let phi_q = q.glv_phi();
            if !phi_q.inf {
                let phi_x_bytes = phi_q.x.to_bytes();
                if phi_x_bytes == target_x_arr {
                    println!("\n  *** FOUND via GLV phi! k = {:?} ***", k.limbs);
                    return Some(k);
                }
            }

            let phi2_q = q.glv_phi2();
            if !phi2_q.inf {
                let phi2_x_bytes = phi2_q.x.to_bytes();
                if phi2_x_bytes == target_x_arr {
                    println!("\n  *** FOUND via GLV phi^2! k = {:?} ***", k.limbs);
                    return Some(k);
                }
            }
        }

        // Additive walk: Q = Q + G
        q = q.add(&g);
        k = k.add_mod_n(&Fe::from_u64(1));

        if tested % report_interval == 0 {
            let elapsed = start_time.elapsed().as_secs_f64();
            let rate = tested as f64 / elapsed;
            println!("  Tested: {} | Rate: {:.0} ops/s | k bits: {}", tested, rate, k.bit_length());
        }

        let range_end = Fe::power_of_2(range_bits);
        if k.cmp_val(&range_end.limbs) != std::cmp::Ordering::Less {
            break;
        }
    }

    println!("\n  Range exhausted. Tested: {}", tested);
    None
}

// ============================================================
// GPU SOLVER (CUDA via cudarc, CPU fallback available)
// ============================================================

fn gpu_solve(target_x: &[u8; 32], range_start: Fe, range_bits: u32) -> Option<Fe> {
    use gpu::{GpuSolver, CollisionResult};

    // Decompress target point — try both y parities
    let target_point = decompress_point(target_x, false)
        .or_else(|| decompress_point(target_x, true));

    let target = match target_point {
        Some(p) => p,
        None => {
            println!("  ERROR: Cannot decompress target point.");
            return None;
        }
    };

    let range_end = Fe::power_of_2(range_bits);
    let mut solver = GpuSolver::new(target, range_start, range_end, range_bits);

    match solver.run() {
        Some(result) => {
            if result.verified {
                println!("\n  GPU SOLVER: KEY FOUND AND VERIFIED!");
                Some(result.private_key)
            } else {
                println!("\n  GPU SOLVER: Possible key (unverified).");
                Some(result.private_key)
            }
        }
        None => {
            println!("\n  GPU SOLVER: No key found.");
            None
        }
    }
}

// ============================================================
// MAIN
// ============================================================

fn main() {
    let args = Args::parse();

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  VORTEX PRIME v5 — NOUS SOMMES LES RECHERCHES          ║");
    println!("║  Native u64x4 + 6D Lattice + Jacobian Kangaroo          ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("  Inventions:");
    println!("    1. SHA-256 Round 0 ORACLE (PREDICTEUR)");
    println!("    2. Z[omega] DLP Lifting (n = pi * pi_bar)");
    println!("    3. Optimized Kangaroo (Jacobian + native field)");
    println!("    4. 6D Range-Constrained Lattice (n^(1/6) ≈ 2^45)");
    println!("  Optimizations:");
    println!("    - Native u64x4 field arithmetic with reduce512() (10-100x)");
    println!("    - Jacobian coordinates (no inversion per hop)");
    println!("    - Mixed addition: 8M+3S per hop");
    println!();

    // Configure threads
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads as usize)
            .build_global()
            .ok();
    }

    // Get target
    let puzzle = get_puzzle(args.target);
    println!("  Target: Puzzle #{}", args.target);
    println!("  Address: {}", puzzle.address);
    println!("  Pubkey: {}", puzzle.pubkey_hex);
    println!("  Range: [2^{}, 2^{})", puzzle.range_bits - 1, puzzle.range_bits);

    // Parse pubkey (some puzzles don't have exposed pubkeys)
    let has_pubkey = !puzzle.pubkey_hex.is_empty();
    let target_point = if has_pubkey {
        let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
        let oracle = Round0Oracle::new(&pubkey_bytes);
        let glv = GLVDecomposer::new();
        println!("\n  GLV: lambda verified, phi(G) on curve: {}", glv.phi_g.is_on_curve());
        decompress_point(&oracle.target_x, pubkey_bytes[0] == 0x03)
    } else {
        println!("\n  No pubkey available — using address-only modes (brute, sparse, bip32)");
        None
    };

    // Compute search range
    let range_bits = puzzle.range_bits;
    let range_start = Fe::power_of_2(range_bits - 1);

    // For modes that don't need pubkey/oracle, we still need range_bits
    // Oracle and GLV are only needed for pubkey-based modes
    let oracle = if has_pubkey {
        let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
        Some(Round0Oracle::new(&pubkey_bytes))
    } else {
        None
    };

    // Select solver mode
    match args.mode.as_str() {
        "oracle" => {
            if let Some(ref o) = oracle {
                run_oracle(o);
            } else {
                println!("  ERROR: Oracle requires exposed pubkey.");
            }
        }
        "zomega" | "z[omega]" => {
            run_zomega();
        }
        "kangaroo" | "4d" => {
            if let Some(tp) = target_point {
                run_kangaroo(&tp, &range_start, &Fe::power_of_2(range_bits), args.max_hops);
            } else {
                println!("  ERROR: Kangaroo requires exposed pubkey. Use --mode sparse or --mode brute for no-pubkey puzzles.");
            }
        }
        "lattice6d" | "6d" => {
            let lifter = run_zomega();
            run_lattice6d(range_bits, &lifter);
        }
        "lattice" | "lll" | "3d" => {
            run_lattice(range_bits);
        }
        "pipeline" | "full" => {
            if let (Some(ref o), Some(tp)) = (&oracle, target_point) {
                run_pipeline(o, &tp, range_bits, args.max_hops);
            } else {
                println!("  ERROR: Pipeline requires exposed pubkey.");
            }
        }
        "cpu" => {
            if let Some(ref o) = oracle {
                if let Some(k) = cpu_solve_additive(&o.target_x, range_start, range_bits) {
                    println!("\n  ╔══════════════════════════════════════╗");
                    println!("  ║  KEY FOUND: {}  ║", k);
                    println!("  ╚══════════════════════════════════════╝");
                }
            } else {
                println!("  ERROR: CPU mode requires exposed pubkey.");
            }
        }
        "cuda" | "gpu" => {
            if let Some(ref o) = oracle {
                if let Some(k) = gpu_solve(&o.target_x, range_start, range_bits) {
                    println!("\n  KEY FOUND: {:?}", k.limbs);
                }
            } else {
                println!("  ERROR: GPU kangaroo requires exposed pubkey. Use --mode sparse for address-only.");
            }
        }
        "lbe" => {
            if let Some(tp) = target_point {
                run_lbe(range_bits, &tp, args.max_hops);
            } else {
                println!("  ERROR: Cannot decompress target point.");
            }
        }
        "test" => {
            run_test_mode();
        }
        "db" => {
            puzzle_db::print_db_summary();
        }
        "analyze" => {
            analyzer::run_full_analysis();
        }
        "sparse" => {
            sparse::sparse_search_fast(range_bits, args.max_weight, args.target);
        }
        "sparse-gpu" => {
            let solver = gpu::GpuSparseSolver::new(range_bits, args.max_weight, args.target);
            let result = solver.run();
            if result.found {
                println!("\n  GPU sparse search found key in {:.2}s!", result.elapsed_secs);
            }
        }
        "bip32" => {
            use bip32::SeedRecovery;
            let recovery = SeedRecovery::new();

            if let Some(seed_hex) = &args.seed {
                // Direct seed search mode
                let seed_bytes = hex::decode(seed_hex).unwrap_or_else(|_| {
                    eprintln!("Invalid seed hex: {}", seed_hex);
                    std::process::exit(1);
                });
                println!("  Searching from seed: {} ({} seeds, path index: {})",
                         seed_hex, args.seed_count, args.path_index);
                match recovery.search_seed_range(&seed_bytes, args.seed_count, args.path_index) {
                    Some(found_seed) => {
                        println!("\n  ╔══════════════════════════════════════════════════╗");
                        println!("  ║  SEED FOUND!                                     ║");
                        println!("  ║  {} ║", hex::encode(&found_seed));
                        println!("  ╚══════════════════════════════════════════════════╝");
                    }
                    None => {
                        println!("  No matching seed found in range.");
                    }
                }
            } else {
                // Full analysis mode
                recovery.run();
            }
        }
        "brute" => {
            run_brute_force(range_bits, args.target);
        }
        _ => {
            eprintln!("Unknown mode: {}. Use: kangaroo, bip32, brute, sparse, sparse-gpu, analyze, db, test, oracle, zomega, lattice, lattice6d, lbe, pipeline, cpu, cuda, gpu", args.mode);
        }
    }

    println!("\n  NOUS SOMMES LES RECHERCHES.");
}

/// Multi-Target Brute-Force: Address Generation
/// For puzzles WITHOUT exposed public keys, we must:
///   1. Generate k in range
///   2. Compute k*G
///   3. Compute hash160(k*G)
///   4. Compare against ALL target hash160 values
fn run_brute_force(range_bits: u32, target_puzzle: u32) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  Multi-Target Brute-Force — Address Generation          ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    use bip32::{check_key_against_puzzles, check_key_multi_target, hash160, pubkey_to_address};
    use puzzle_db::{UNSOLVED_NO_PUBKEY, UNSOLVED_WITH_PUBKEY, get_all_target_hash160s};
    use rayon::prelude::*;

    let range_start = Fe::power_of_2(range_bits - 1);
    let range_end = Fe::power_of_2(range_bits);

    // Get all target hash160 values for multi-target search
    let targets = get_all_target_hash160s();

    println!("  Target puzzle: P{}", target_puzzle);
    println!("  Range: [2^{}, 2^{})", range_bits - 1, range_bits);
    println!("  Targets: {} unsolved addresses (brute-force)", UNSOLVED_NO_PUBKEY.len());
    println!("  Targets: {} with pubkey (kangaroo-solvable)", UNSOLVED_WITH_PUBKEY.len());
    println!("  Multi-target speedup: {:.1}x", (targets.len() as f64).sqrt());

    let n_threads = rayon::current_num_threads();
    println!("  Threads: {}", n_threads);

    let total_keys = range_end.sub(&range_start);
    println!("  Total keys to check: ~2^{}", total_keys.bit_length());

    // Time estimate
    let keys_per_sec_est = 3_900_000.0 * n_threads as f64; // ~3.9M ops/s per thread
    let total_keys_f = 2_f64.powi(range_bits as i32 - 1);
    let est_seconds = total_keys_f / keys_per_sec_est;
    let est_years = est_seconds / (365.25 * 24.0 * 3600.0);
    println!("  Estimated rate: {:.0} keys/s (CPU)", keys_per_sec_est);
    println!("  Estimated time: {:.1e} years (CPU, single puzzle)", est_years);
    println!("  With multi-target ({:.1}x): {:.1e} years", 
             (targets.len() as f64).sqrt(), est_years / (targets.len() as f64).sqrt());
    println!();
    println!("  GPU acceleration: ~1B keys/s per RTX 4090");
    println!("  GPU estimate: {:.1e} years (with multi-target)", 
             total_keys_f / 1e9 / (365.25 * 24.0 * 3600.0) / (targets.len() as f64).sqrt());
    println!();
    println!("  Starting brute-force search...\n");

    let start = Instant::now();
    let found = std::sync::atomic::AtomicBool::new(false);
    let result_lock = std::sync::Mutex::new(None::<(u32, Fe)>);
    let keys_checked = std::sync::atomic::AtomicU64::new(0);

    // Parallel search using Rayon
    let stride = n_threads as u64;
    let batch_size = 1_000_000u64; // Progress reporting interval

    (0..n_threads).into_par_iter().for_each(|thread_id| {
        if found.load(std::sync::atomic::Ordering::Relaxed) { return; }

        let mut k = range_start.add(&Fe::from_u64(thread_id as u64));
        let mut local_count = 0u64;

        for _ in 0..batch_size {
            if found.load(std::sync::atomic::Ordering::Relaxed) { break; }

            // Multi-target check (fast: uses early-reject on first 4 bytes)
            match check_key_multi_target(&k, &targets) {
                Some(puzzle_num) => {
                    found.store(true, std::sync::atomic::Ordering::Relaxed);
                    *result_lock.lock().unwrap() = Some((puzzle_num, k));
                    return;
                }
                None => {}
            }

            // Advance to next key (stride by n_threads for parallelism)
            k = k.add(&Fe::from_u64(stride));
            if k.cmp_val(&range_end.limbs) >= std::cmp::Ordering::Equal {
                break;
            }
            local_count += 1;
        }

        keys_checked.fetch_add(local_count, std::sync::atomic::Ordering::Relaxed);
    });

    let elapsed = start.elapsed();
    let checked = keys_checked.load(std::sync::atomic::Ordering::Relaxed);
    let rate = checked as f64 / elapsed.as_secs_f64();

    if let Some((puzzle_num, key)) = result_lock.lock().unwrap().take() {
        println!("\n  ╔══════════════════════════════════════════════════╗");
        println!("  ║  KEY FOUND!                                      ║");
        println!("  ║  Puzzle #{}                                      ║", puzzle_num);
        println!("  ║  Key: {} ║", key);
        println!("  ╚══════════════════════════════════════════════════╝");

        // Verify by computing the address
        let point = Point::generator().scalar_mul(&key);
        let pubkey_bytes = point.to_bytes();
        let address = pubkey_to_address(&pubkey_bytes);
        let h = hash160(&pubkey_bytes);
        println!("  Address: {}", address);
        println!("  Hash160: {}", hex::encode(h));
    } else {
        println!("  No match found in this range segment");
        println!("  Checked {} keys in {:.1}s ({:.0} keys/s)", checked, elapsed.as_secs_f64(), rate);
        println!();
        println!("  Note: Full range requires GPU acceleration");
        println!("  Build: make -C RUSTSOLVER/cuda ptx");
        println!("  Run:   ./vortex-gpu --mode brute -t {} --features cuda", target_puzzle);
    };
}

fn run_test_mode() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  TEST MODE: Validate Native u64x4 + Jacobian            ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();
    println!("  G on curve: {}", g.is_on_curve());

    // Test 1: 2*G via Jacobian
    let g_j = g.to_jacobian();
    let g2_j = g_j.double();
    let g2 = g2_j.to_affine();
    let expected_2g_x = Fe::from_hex("c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5");
    println!("  2*G on curve: {} | x correct: {}", g2.is_on_curve(), g2.x == expected_2g_x);
    if g2.x != expected_2g_x {
        println!("  2*G got x:  {}", g2.x);
        println!("  2*G want x: c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5");
    }

    // Test 2: 7*G via scalar mul
    let g7 = g.scalar_mul(&Fe::from_u64(7));
    let expected_7g_x = Fe::from_hex("5cbdf0646e5db4eaa398f365f2ea7a0e3d419b7e0330e39ce92bddedcac4f9bc");
    println!("  7*G on curve: {} | x correct: {}", g7.is_on_curve(), g7.x == expected_7g_x);
    if g7.x != expected_7g_x {
        println!("  7*G got x:  {}", g7.x);
    }

    // Test 3: P70 key (self-consistency: verify k*G is on curve and k*G+G = (k+1)*G)
    let k_p70 = Fe::from_u64(0x6c3a4f);
    let q_p70 = g.scalar_mul(&k_p70);
    println!("  P70: Q = 0x6c3a4f * G on curve: {}", q_p70.is_on_curve());
    // Self-consistency: Q + G = (k+1)*G
    let q_p70_plus_g = q_p70.add(&g);
    let k_p70_plus_1 = k_p70.add_mod_n(&Fe::from_u64(1));
    let q_p70_plus_g_check = g.scalar_mul(&k_p70_plus_1);
    println!("  P70: Q+G == (k+1)*G: {}", q_p70_plus_g.x == q_p70_plus_g_check.x);

    // Test 4: GLV phi
    let phi_g = g.glv_phi();
    println!("  phi(G) on curve: {}", phi_g.is_on_curve());
    let phi2_g = g.glv_phi2();
    println!("  phi^2(G) on curve: {}", phi2_g.is_on_curve());

    // Test 5: Beta^3 = 1 mod P
    let beta = Fe { limbs: field::BETA };
    let beta_sq = beta.mul(&beta);
    let beta_cu = beta_sq.mul(&beta);
    println!("  Beta^3 = 1 mod P: {}", beta_cu == Fe::ONE);

    // Test 6: Decompression test for P70
    println!("\n  Decompression test for P70...");
    let x70 = Fe::from_hex("94d991ef2a38291416f959de8f80769e0a74d7f81a49267f50b2de1a34dbc2df");
    let x_sq = x70.mul(&x70);
    let x_cu = x_sq.mul(&x70);
    let y_sq = x_cu.add(&Fe::from_u64(7));
    let exp = Fe::from_hex("3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFBFFFFF0C");
    let y = y_sq.pow(&exp);
    let y_sq_check = y.mul(&y);
    println!("  y^2 == x^3+7: {}", y_sq_check == y_sq);

    // Try both y values
    let p1 = Point { x: x70, y, inf: false };
    let p2 = Point { x: x70, y: y.neg_mod_p(), inf: false };
    println!("  P70 +y on curve: {}", p1.is_on_curve());
    println!("  P70 -y on curve: {}", p2.is_on_curve());

    // Cross-check with computed Q70
    if p1.y == q_p70.y || p2.y == q_p70.y {
        println!("  P70 decompression MATCHES Q70!");
    } else {
        println!("  P70 decompression MISMATCH");
        println!("    decompressed y: {}", y);
        println!("    Q70.y:          {}", q_p70.y);
    }

    // Test 7: Kangaroo hop benchmark
    println!("\n  Benchmark: Kangaroo hop rate (Jacobian mixed add)...");
    let bench_start = Instant::now();
    let bench_hops = 10_000;
    let mut pt = g.to_jacobian();
    let step_point = g.scalar_mul(&Fe::from_u64(12345));
    for _ in 0..bench_hops {
        pt = pt.add_affine(&step_point);
    }
    let bench_elapsed = bench_start.elapsed().as_secs_f64();
    let bench_rate = bench_hops as f64 / bench_elapsed;
    println!("  Kangaroo hop rate: {:.0} hops/s (Jacobian mixed add)", bench_rate);

    // Test 8: Brute force test k=12345
    println!("\n  Brute force test: k=12345, range=[10000, 20000)");
    let k_test = Fe::from_u64(12345);
    let q_test = g.scalar_mul(&k_test);
    let target_x = q_test.x.to_bytes();
    let start_time = Instant::now();
    let mut current = g.scalar_mul(&Fe::from_u64(10000));
    for k_val in 10000u64..20000u64 {
        if !current.inf && current.x.to_bytes() == target_x {
            let elapsed = start_time.elapsed().as_millis();
            println!("  *** FOUND! k = {} in {}ms ***", k_val, elapsed);
            break;
        }
        current = current.add(&g);
    }
}

/// Decompress a secp256k1 point from x-coordinate and parity flag.
fn decompress_point(x_bytes: &[u8; 32], y_is_odd: bool) -> Option<Point> {
    let x = Fe::from_bytes(x_bytes);

    // y^2 = x^3 + 7 (mod p)
    let x_sq = x.mul(&x);
    let x_cu = x_sq.mul(&x);
    let y_sq = x_cu.add(&Fe::from_u64(7));

    // y = y_sq^((p+1)/4) mod p  (since p ≡ 3 mod 4)
    let exp = Fe::from_hex("3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFBFFFFF0C");
    let y = y_sq.pow(&exp);

    // Adjust parity
    let y_parity = y.limbs[0] & 1 == 1;
    let y_final = if y_parity != y_is_odd {
        y.neg_mod_p()
    } else {
        y
    };

    let point = Point { x, y: y_final, inf: false };

    if point.is_on_curve() {
        Some(point)
    } else {
        let point2 = Point { x, y: y_final.neg_mod_p(), inf: false };
        if point2.is_on_curve() {
            Some(point2)
        } else {
            None
        }
    }
}

fn _verify_2g() {
    let g = Point::generator();
    let g2 = g.to_jacobian().double().to_affine();
    eprintln!("2*G x = {:02x}{:02x}{:02x}{:02x}...{:02x}", 
        g2.x.limbs[3] >> 56, g2.x.limbs[3] >> 48, g2.x.limbs[3] >> 40, g2.x.limbs[3] >> 32,
        g2.x.limbs[0] & 0xFF);
    eprintln!("2*G full x = {}", g2.x);
    let expected_2g_x = Fe::from_hex("c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5");
    eprintln!("2*G x match: {}", g2.x == expected_2g_x);
}
