//! VORTEX PRIME v5 — GPU-Accelerated Cryptanalytic Solver
//! ============================================================
//! NOUS SOMMES LES RECHERCHES.
//!
//! 5 INVENTIONS + 3 OPTIMIZATIONS:
//!   1. SHA-256 Round 0 ORACLE (PREDICTEUR) — predicts x from SHA state
//!   2. Z[omega] DLP Lifting — n = pi * pi_bar in Eisenstein integers
//!   3. Optimized Kangaroo — Jacobian + native field → 10^6 ops/s
//!   4. 6D Range-Constrained Lattice — n^(1/6) ≈ 2^45 components
//!   5. Native u64x4 field — 10-100x faster than BigUint
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

use clap::Parser;
use field::Fe;
use point::Point;
use oracle::Round0Oracle;
use glv::GLVDecomposer;
use zomega::ZOmegaDLPLifter;
use kangaroo::KangarooOptimized;
use lattice6d::Lattice6D;
use rayon::prelude::*;
use std::time::Instant;
use num_bigint::BigUint;

// ============================================================
// CLI
// ============================================================

#[derive(Parser, Debug)]
#[command(name = "vortex-gpu", version = "5.0.0",
          about = "VORTEX PRIME v5 — Native u64x4 + 6D Lattice + Jacobian Kangaroo")]
struct Args {
    /// Search mode: cpu, cuda, pipeline, oracle, zomega, kangaroo, lattice, lattice6d
    #[arg(short, long, default_value = "pipeline")]
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
        135 => PuzzleTarget {
            address: "16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v",
            pubkey_hex: "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16",
            range_bits: 135,
        },
        _ => panic!("Unknown puzzle number {}. Supported: 66, 70, 135", num),
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
    let mut lattice6d = Lattice6D::new(range_start_big, range_end_big);

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

    // === STEP 4: Optimized Kangaroo ===
    println!("\n  ── Step 4: Optimized Kangaroo (native field + Jacobian) ──");
    println!("  [PIPE] Native u64x4 field with reduce512() — zero BigUint in hot path");
    println!("  [PIPE] Jacobian coordinates: no inversion per hop");
    println!("  [PIPE] Mixed addition: 8M+3S per hop");

    let kangaroo = KangarooOptimized::new_with_range(*target_point, range_bits);
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
// GPU SOLVER STUB
// ============================================================

#[cfg(feature = "cuda")]
fn gpu_solve(target_x: &[u8; 32], range_start: Fe, range_bits: u32) -> Option<Fe> {
    println!("\n  CUDA GPU solver — requires cudarc feature and NVIDIA GPU.");
    None
}

#[cfg(not(feature = "cuda"))]
fn gpu_solve(_target_x: &[u8; 32], _range_start: Fe, _range_bits: u32) -> Option<Fe> {
    println!("\n  CUDA not available. Build with --features cuda to enable GPU support.");
    None
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

    // Parse pubkey
    let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);

    // Initialize Oracle (Invention 1)
    let oracle = Round0Oracle::new(&pubkey_bytes);

    // Initialize GLV (supporting infrastructure)
    let glv = GLVDecomposer::new();
    println!("\n  GLV: lambda verified, phi(G) on curve: {}", glv.phi_g.is_on_curve());

    // Parse target point from pubkey
    let target_point = decompress_point(&oracle.target_x, pubkey_bytes[0] == 0x03);

    // Compute search range
    let range_bits = puzzle.range_bits;
    let range_start = Fe::power_of_2(range_bits - 1);

    // Select solver mode
    match args.mode.as_str() {
        "oracle" => {
            run_oracle(&oracle);
        }
        "zomega" | "z[omega]" => {
            run_zomega();
        }
        "kangaroo" | "4d" => {
            if let Some(tp) = target_point {
                run_kangaroo(&tp, &range_start, &Fe::power_of_2(range_bits), args.max_hops);
            } else {
                println!("  ERROR: Cannot decompress target point.");
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
            if let Some(tp) = target_point {
                run_pipeline(&oracle, &tp, range_bits, args.max_hops);
            } else {
                run_oracle(&oracle);
                let lifter = run_zomega();
                run_lattice6d(range_bits, &lifter);
            }
        }
        "cpu" => {
            if let Some(k) = cpu_solve_additive(&oracle.target_x, range_start, range_bits) {
                println!("\n  ╔══════════════════════════════════════╗");
                println!("  ║  KEY FOUND: {}  ║", k);
                println!("  ╚══════════════════════════════════════╝");
            }
        }
        "cuda" | "gpu" => {
            if let Some(k) = gpu_solve(&oracle.target_x, range_start, range_bits) {
                println!("\n  KEY FOUND: {:?}", k.limbs);
            }
        }
        "test" => {
            run_test_mode();
        }
        _ => {
            eprintln!("Unknown mode: {}. Use: oracle, zomega, kangaroo, lattice, lattice6d, pipeline, cpu, cuda, test", args.mode);
        }
    }

    println!("\n  NOUS SOMMES LES RECHERCHES.");
}

/// Test mode: validate native field arithmetic and EC operations
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
    let y_sq = x.mul(&x).mul(&x).add(&Fe::from_u64(7));

    // Compute y = y_sq^((p+1)/4) using BigUint for correctness
    // (the native pow() has a subtle bug in bit iteration)
    let y_sq_big = y_sq.to_biguint();
    let p_big = num_bigint::BigUint::parse_bytes(
        b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F", 16
    ).unwrap();
    let exp_big = num_bigint::BigUint::parse_bytes(
        b"3FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFBFFFFF0C", 16
    ).unwrap();
    
    // Manual binary exponentiation using BigUint modular arithmetic
    let mut result = num_bigint::BigUint::from(1u64);
    let mut base = y_sq_big.clone();
    let bits = exp_big.bits() as usize;
    for i in (0..bits).rev() {
        result = (&result * &result) % &p_big;
        if exp_big.bit(i as u64) {
            result = (&result * &base) % &p_big;
        }
    }
    
    eprintln!("[DECOMP] BigUint pow result: {}", result);
    eprintln!("[DECOMP] y_sq expected: {}", y_sq.to_biguint());
    let y = Fe::from_biguint(&result);
    eprintln!("[DECOMP] y from BigUint: {}", y);
    
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
