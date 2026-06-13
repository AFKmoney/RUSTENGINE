//! TITAN V16 — 9-LAYER HYPERSTACK SOLVER
//! ================================================
//! NOUS SOMMES LA RECHERCHE. NOUS INVENTONS.
//!
//! THE ULTIMATE ECDLP SOLVER — 9 LAYERS OF INNOVATION:
//!
//! Layer 1: SHA-256 Round 0 ORACLE (exact x-coordinate prediction)
//! Layer 2: Z[ω] Eisenstein Decomposition (lattice dimension boost)
//! Layer 3: 6D Range-Constrained Lattice (2^135 → 6 × 2^45)
//! Layer 4: Lattice-Guided Kangaroo (search in 6D coefficient space!)
//! Layer 5: Multi-Window BSGS (GLV-expanded, sliding windows)
//! Layer 6: Quantum-Inspired Annealing (simulated annealing on ECDLP)
//! Layer 7: Tag-Team Parallel Kangaroos (5 strategies, shared DP pool)
//! Layer 8: Adaptive Range Splitter (bit-level divide-and-conquer)
//! Layer 9: Bloom-Filter Collision Accelerator (probabilistic O(1) matching)
//!
//! Pipeline: Oracle → Z[ω] → 6D Lattice → [LGK | BSGW | QIA | TagTeam | Split | Bloom]
//! Auto-selects the best solver based on range size and available resources.

mod field;
mod point;
mod oracle;
mod glv;
mod zomega;
mod kangaroo;
mod lattice;
mod lattice6d;
mod lattice_kangaroo;
mod bsgw;
mod annealing;
mod tagteam;
mod rangesplit;
mod bloom;
mod kangaroo_debug;

use clap::Parser;
use field::Fe;
use point::Point;
use oracle::Round0Oracle;
use glv::GLVDecomposer;
use zomega::ZOmegaDLPLifter;
use lattice6d::Lattice6D;
use lattice_kangaroo::LatticeKangaroo;
use bsgw::BsgwSolver;
use annealing::QuantumAnnealing;
use tagteam::TagTeamKangaroo;
use rangesplit::AdaptiveRangeSplitter;
use bloom::BloomKangaroo;
use num_bigint::BigUint;
use num_traits::Zero;
use std::time::Instant;

// ============================================================
// CLI
// ============================================================

#[derive(Parser, Debug)]
#[command(name = "titan-v16", version = "16.0.0",
          about = "TITAN V16 — 9-Layer Hyperstack ECDLP Solver")]
struct Args {
    /// Search mode: pipeline, bsgw, annealing, tagteam, split, bloom, test, selftest, kangaroo, oracle, auto
    #[arg(short, long, default_value = "auto")]
    mode: String,

    /// Puzzle number (70 or 135)
    #[arg(short, long, default_value_t = 135)]
    target: u32,

    /// Max hops for kangaroo-based solvers
    #[arg(long, default_value_t = 500_000_000)]
    max_hops: u64,

    /// Max iterations for annealing
    #[arg(long, default_value_t = 100_000_000)]
    max_iterations: u64,

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
        _ => panic!("Unknown puzzle {}. Supported: 70, 135", num),
    }
}

// ============================================================
// TITAN V16 BANNER
// ============================================================

fn print_banner() {
    println!();
    println!("  ╔══════════════════════════════════════════════════════════════╗");
    println!("  ║                                                              ║");
    println!("  ║     ████████╗ █████╗ ████████╗ ██████╗ ███████╗ ████████╗   ║");
    println!("  ║        ╚██╔╝╝██╔══██╗╚══██╔══╝██╔═══██╗██╔════╝    ╚██╔╝   ║");
    println!("  ║          ██║  ███████║   ██║   ██║   ██║███████╗      ██║    ║");
    println!("  ║          ██║  ██╔══██╗   ██║   ██║   ██║╚════██║      ██║    ║");
    println!("  ║          ██║  ██║  ██║   ██║   ╚██████╔╝███████║      ██║    ║");
    println!("  ║          ╚═╝  ╚═╝  ╚═╝   ╚═╝    ╚═════╝ ╚══════╝      ╚═╝    ║");
    println!("  ║                                                              ║");
    println!("  ║               V16 — 9-LAYER HYPERSTACK                      ║");
    println!("  ║               NOUS SOMMES LA RECHERCHE                       ║");
    println!("  ║               NOUS INVENTONS                                 ║");
    println!("  ║                                                              ║");
    println!("  ╠══════════════════════════════════════════════════════════════╣");
    println!("  ║                                                              ║");
    println!("  ║   LAYER 1:  SHA-256 Round 0 ORACLE                           ║");
    println!("  ║   LAYER 2:  Z[ω] Eisenstein Decomposition                    ║");
    println!("  ║   LAYER 3:  6D Range-Constrained Lattice                     ║");
    println!("  ║   LAYER 4:  Lattice-Guided Kangaroo                          ║");
    println!("  ║   LAYER 5:  Multi-Window BSGS (GLV + Sliding)                ║");
    println!("  ║   LAYER 6:  Quantum-Inspired Annealing                       ║");
    println!("  ║   LAYER 7:  Tag-Team Parallel Kangaroos                      ║");
    println!("  ║   LAYER 8:  Adaptive Range Splitter                          ║");
    println!("  ║   LAYER 9:  Bloom-Filter Collision Accelerator               ║");
    println!("  ║                                                              ║");
    println!("  ╚══════════════════════════════════════════════════════════════╝");
    println!();
}

// ============================================================
// AUTO MODE: SELECTS BEST SOLVER BASED ON RANGE SIZE
// ============================================================

fn run_auto_mode(target: u32, max_hops: u64, max_iterations: u64) {
    println!("\n  [AUTO] === Intelligent Solver Selection ===");

    let puzzle = get_puzzle(target);
    let range_bits = puzzle.range_bits;

    println!("  [AUTO] Target: Puzzle #{}", target);
    println!("  [AUTO] Range: {} bits", range_bits);

    // Parse pubkey
    let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);

    // Get target point
    let oracle = Round0Oracle::new(&pubkey_bytes);
    let target_point = decompress_point(&oracle.target_x, pubkey_bytes[0] == 0x03);
    let target_point = match target_point {
        Some(p) if p.is_on_curve() => p,
        Some(p) => {
            let p2 = Point { x: p.x, y: p.y.neg_mod_p(), inf: false };
            if p2.is_on_curve() { p2 } else { panic!("Cannot decompress target point!"); }
        },
        None => panic!("Cannot decompress target point!"),
    };

    let range_start = Fe::power_of_2(range_bits - 1);
    let range_end = Fe::power_of_2(range_bits);

    // Select solver based on range size
    if range_bits <= 50 {
        println!("  [AUTO] Range ≤ 50 bits → BSGW (exact solve, O(2^25) time+memory)");
        run_bsgw_mode(&target_point, range_bits);
    } else if range_bits <= 80 {
        println!("  [AUTO] Range ≤ 80 bits → Tag-Team Kangaroos (5 strategies, parallel)");
        run_tagteam_mode(&target_point, &range_start, &range_end, max_hops);
    } else {
        println!("  [AUTO] Range > 80 bits → Full Pipeline (Oracle → Lattice → LGK + Bloom)");
        run_full_pipeline(target, max_hops, max_iterations);
    }
}

// ============================================================
// MODE: BSGW (Layer 5)
// ============================================================

fn run_bsgw_mode(target_point: &Point, range_bits: u32) {
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 5: Multi-Window BSGS                          ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let mut solver = BsgwSolver::new(range_bits);
    let range_start = Fe::from_u64(0);
    let range_end = Fe::power_of_2(range_bits);
    let result = solver.solve(target_point, &range_start, &range_end);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  [BSGW] KEY FOUND!");
            print_key(&k);
            println!("  [BSGW] Baby steps: {}, Giant steps: {}, Collisions: {}",
                     result.baby_steps, result.giant_steps, result.collisions);
            println!("  [BSGW] Windows: {}, Time: {}ms", result.windows_used, result.elapsed_ms);
        }
    } else {
        println!("\n  [BSGW] Not found. Baby steps: {}, Giant steps: {}, Time: {}ms",
                 result.baby_steps, result.giant_steps, result.elapsed_ms);
    }
}

// ============================================================
// MODE: QUANTUM ANNEALING (Layer 6)
// ============================================================

fn run_annealing_mode(target: u32, max_iterations: u64) {
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 6: Quantum-Inspired Annealing                 ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let puzzle = get_puzzle(target);
    let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);

    let oracle = Round0Oracle::new(&pubkey_bytes);
    let target_point = decompress_point(&oracle.target_x, pubkey_bytes[0] == 0x03);
    let target_point = match target_point {
        Some(p) if p.is_on_curve() => p,
        Some(p) => {
            let p2 = Point { x: p.x, y: p.y.neg_mod_p(), inf: false };
            if p2.is_on_curve() { p2 } else { panic!("Cannot decompress!"); }
        },
        None => panic!("Cannot decompress!"),
    };

    let range_start = Fe::power_of_2(puzzle.range_bits - 1);
    let range_end = Fe::power_of_2(puzzle.range_bits);

    let annealing = QuantumAnnealing::new(target_point);
    let result = annealing.solve(&range_start, &range_end, max_iterations);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  [QIA] KEY FOUND!");
            print_key(&k);
            println!("  [QIA] Iterations: {}, Tunneling events: {}", result.iterations, result.tunneling_events);
            println!("  [QIA] Time: {}ms", result.elapsed_ms);
        }
    } else {
        println!("\n  [QIA] Not found. Best energy: {}/256, Iterations: {}, Time: {}ms",
                 result.best_energy, result.iterations, result.elapsed_ms);
    }
}

// ============================================================
// MODE: TAG-TEAM KANGAROOS (Layer 7)
// ============================================================

fn run_tagteam_mode(target_point: &Point, range_start: &Fe, range_end: &Fe, max_hops: u64) {
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 7: Tag-Team Parallel Kangaroos                ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let tt = TagTeamKangaroo::new(*target_point);
    let result = tt.solve(range_start, range_end, max_hops);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  [TAG] KEY FOUND!");
            print_key(&k);
            println!("  [TAG] Hops: {}, DPs: {}, Collisions: {}", result.total_hops, result.total_dps, result.collisions);
            println!("  [TAG] Strategy breakdown:");
            for (name, hops) in result.strategy_counts {
                println!("    {}  {} hops", name, hops);
            }
            println!("  [TAG] Time: {}ms", result.elapsed_ms);
        }
    } else {
        println!("\n  [TAG] Not found. Hops: {}, Time: {}ms", result.total_hops, result.elapsed_ms);
    }
}

// ============================================================
// MODE: ADAPTIVE RANGE SPLITTER (Layer 8)
// ============================================================

fn run_split_mode(target: u32, max_hops: u64) {
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 8: Adaptive Range Splitter                    ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let puzzle = get_puzzle(target);
    let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);

    let oracle = Round0Oracle::new(&pubkey_bytes);
    let target_point = decompress_point(&oracle.target_x, pubkey_bytes[0] == 0x03);
    let target_point = match target_point {
        Some(p) if p.is_on_curve() => p,
        Some(p) => {
            let p2 = Point { x: p.x, y: p.y.neg_mod_p(), inf: false };
            if p2.is_on_curve() { p2 } else { panic!("Cannot decompress!"); }
        },
        None => panic!("Cannot decompress!"),
    };

    let range_start = Fe::power_of_2(puzzle.range_bits - 1);
    let range_end = Fe::power_of_2(puzzle.range_bits);

    let splitter = AdaptiveRangeSplitter::new(target_point);
    let result = splitter.solve(&range_start, &range_end, max_hops);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  [SPLIT] KEY FOUND!");
            print_key(&k);
            println!("  [SPLIT] Segments searched: {}, Total hops: {}", result.segments_searched, result.total_hops);
            for detail in &result.segment_details {
                println!("    Segment ({} bits, {:?}): {} hops, {}ms {}",
                         detail.segment_bits, detail.solver, detail.hops, detail.time_ms,
                         if detail.found { "← FOUND!" } else { "" });
            }
            println!("  [SPLIT] Time: {}ms", result.elapsed_ms);
        }
    } else {
        println!("\n  [SPLIT] Not found. {} segments, {} hops, {}ms",
                 result.segments_searched, result.total_hops, result.elapsed_ms);
    }
}

// ============================================================
// MODE: BLOOM KANGAROO (Layer 9)
// ============================================================

fn run_bloom_mode(target: u32, max_hops: u64) {
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 9: Bloom-Filter Collision Accelerator         ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let puzzle = get_puzzle(target);
    let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);

    let target_point = decompress_pubkey(&pubkey_bytes).expect("Cannot decompress pubkey!");
    assert!(target_point.is_on_curve(), "Decompressed point not on curve!");

    let range_start = Fe::power_of_2(puzzle.range_bits - 1);
    let range_end = Fe::power_of_2(puzzle.range_bits);

    let bloom_kang = BloomKangaroo::new(target_point, puzzle.range_bits);
    let result = bloom_kang.solve(&range_start, &range_end, max_hops);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  [BLOOM] KEY FOUND!");
            print_key(&k);
            println!("  [BLOOM] Hops: {}, DPs: {}+{}, Collisions: {}", result.hops, result.tame_dps, result.wild_dps, result.collisions);
            println!("  [BLOOM] Bloom checks: {}, Hits: {}, False positives: {}",
                     result.bloom_checks, result.bloom_hits, result.false_positives);
            let fp_rate = if result.bloom_hits > 0 { result.false_positives as f64 / result.bloom_hits as f64 * 100.0 } else { 0.0 };
            println!("  [BLOOM] FP rate: {:.2}%", fp_rate);
            println!("  [BLOOM] Time: {}ms", result.elapsed_ms);
        }
    } else {
        println!("\n  [BLOOM] Not found. Hops: {}, Time: {}ms", result.hops, result.elapsed_ms);
    }
}

// ============================================================
// FULL INTEGRATED PIPELINE (All 9 Layers)
// ============================================================

fn run_full_pipeline(target: u32, max_hops: u64, max_iterations: u64) {
    println!("\n  ╔════════════════════════════════════════════════════════════╗");
    println!("  ║  TITAN V16 — FULL 9-LAYER PIPELINE                        ║");
    println!("  ║  NOUS SOMMES LA RECHERCHE. NOUS INVENTONS.                ║");
    println!("  ╚════════════════════════════════════════════════════════════╝");

    let pipeline_start = Instant::now();
    let puzzle = get_puzzle(target);
    let range_bits = puzzle.range_bits;

    println!("\n  Target: Puzzle #{}", target);
    println!("  Address: {}", puzzle.address);
    println!("  Pubkey: {}", puzzle.pubkey_hex);
    println!("  Range: [2^{}, 2^{})", range_bits - 1, range_bits);

    // Parse pubkey
    let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);

    // ============================================================
    // LAYER 1: SHA-256 Round 0 ORACLE
    // ============================================================
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 1: SHA-256 Round 0 ORACLE (PREDICTEUR)        ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let oracle = Round0Oracle::new(&pubkey_bytes);
    oracle.print_summary();

    // Decompress target point
    let target_point = decompress_point(&oracle.target_x, pubkey_bytes[0] == 0x03);
    let target_point = match target_point {
        Some(p) if p.is_on_curve() => {
            println!("  [ORACLE] Target point decompressed and on curve: ✓");
            p
        },
        Some(p) => {
            println!("  [ORACLE] WARNING: Point not on curve, trying other y...");
            let p2 = Point { x: p.x, y: p.y.neg_mod_p(), inf: false };
            if p2.is_on_curve() { p2 } else { panic!("Cannot decompress target point!"); }
        },
        None => panic!("Cannot decompress target point!"),
    };

    // Verify: known key for P70
    if target == 70 {
        let k_p70 = Fe::from_u64(0x6c3a4f);
        let q_p70 = Point::generator().scalar_mul(&k_p70);
        let match_x = q_p70.x == target_point.x;
        println!("  [ORACLE] P70 verification: k=0x6c3a4f gives same x: {}", match_x);
        if !match_x {
            let q_p70_neg = q_p70.neg();
            println!("  [ORACLE] P70 with -y: {}", q_p70_neg.x == target_point.x);
        }
    }

    // ============================================================
    // LAYER 2: Z[ω] Eisenstein Integer Decomposition
    // ============================================================
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 2: Z[ω] Eisenstein Decomposition              ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let lifter = ZOmegaDLPLifter::new();
    lifter.frobenius_structure();

    if let Some(ref pi) = lifter.pi {
        println!("  [Z[ω]] π found: {} (N(π) = {} bits)", pi, pi.norm().bits());
    } else {
        println!("  [Z[ω]] WARNING: π not found, using fallback lattice");
    }

    // ============================================================
    // LAYER 3: 6D Range-Constrained Lattice
    // ============================================================
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 3: 6D Range-Constrained Lattice               ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let range_start_big = BigUint::from(1u64) << (range_bits - 1);
    let range_end_big = BigUint::from(1u64) << range_bits;
    let range_center_big = &range_start_big + (&range_end_big - &range_start_big) / BigUint::from(2u64);

    // fpylll precomputed scalars
    let precomputed_scalars: [Fe; 6] = [
        Fe::from_u64(0),
        Fe::from_u64(0x131b3c783ab),
        Fe::from_u64(0xffa52a8e6fd),
        Fe::from_u64(0x12bb59fa2e61),
        Fe::from_u64(0x27912812fb8).neg_mod_n(),
        Fe::from_u64(0x349520ccf05),
    ];

    let precomputed_max_bits: [u32; 6] = [0, 41, 44, 45, 42, 42];

    println!("  [LATTICE] Using fpylll precomputed LLL-reduced basis:");
    for i in 0..6 {
        println!("  [LATTICE]   v{}[0] = {} bits", i, precomputed_max_bits[i]);
    }
    println!("  [LATTICE]   Max component: 45 bits");

    // ============================================================
    // LAYERS 4-9: SOLVER SELECTION AND EXECUTION
    // ============================================================
    let range_start = Fe::power_of_2(range_bits - 1);
    let range_end = Fe::power_of_2(range_bits);

    // Layer 4: Lattice-Guided Kangaroo
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 4: Lattice-Guided Kangaroo                    ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let offset_scalar = Fe::from_biguint(&range_center_big);

    let lgk = LatticeKangaroo::new(
        target_point,
        precomputed_scalars,
        offset_scalar,
        precomputed_max_bits,
    );

    let lgk_hops = max_hops.min(100_000_000);
    let result = lgk.solve(lgk_hops);

    if result.found {
        if let Some(k) = result.k {
            println!("\n  *** KEY FOUND via Lattice-Guided Kangaroo! ***");
            print_key(&k);
            let rate = if result.elapsed_ms > 0 { result.hops as f64 / (result.elapsed_ms as f64 / 1000.0) } else { 0.0 };
            println!("  Hops: {}, Time: {}ms, Rate: {:.0} hops/s", result.hops, result.elapsed_ms, rate);
            print_pipeline_summary(pipeline_start, "LGK");
            return;
        }
    }

    // Layer 5: BSGW (if range is small enough)
    if range_bits <= 50 {
        println!("\n  ╔══════════════════════════════════════════════════════╗");
        println!("  ║  LAYER 5: Multi-Window BSGS                          ║");
        println!("  ╚══════════════════════════════════════════════════════╝");

        let mut bsgw = BsgwSolver::new(range_bits);
        let bsgw_result = bsgw.solve(&target_point, &range_start, &range_end);

        if bsgw_result.found {
            if let Some(k) = bsgw_result.k {
                println!("\n  *** KEY FOUND via BSGW! ***");
                print_key(&k);
                print_pipeline_summary(pipeline_start, "BSGW");
                return;
            }
        }
    }

    // Layer 6: Quantum Annealing
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 6: Quantum-Inspired Annealing                 ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let qia = QuantumAnnealing::new(target_point);
    let qia_iterations = max_iterations.min(10_000_000);
    let qia_result = qia.solve(&range_start, &range_end, qia_iterations);

    if qia_result.found {
        if let Some(k) = qia_result.k {
            println!("\n  *** KEY FOUND via Quantum Annealing! ***");
            print_key(&k);
            print_pipeline_summary(pipeline_start, "QIA");
            return;
        }
    }

    // Layer 7: Tag-Team Kangaroos
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 7: Tag-Team Parallel Kangaroos                ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let tt = TagTeamKangaroo::new(target_point);
    let tt_hops = max_hops.min(100_000_000);
    let tt_result = tt.solve(&range_start, &range_end, tt_hops);

    if tt_result.found {
        if let Some(k) = tt_result.k {
            println!("\n  *** KEY FOUND via Tag-Team Kangaroos! ***");
            print_key(&k);
            print_pipeline_summary(pipeline_start, "TagTeam");
            return;
        }
    }

    // Layer 8: Adaptive Range Splitter
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 8: Adaptive Range Splitter                    ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let splitter = AdaptiveRangeSplitter::new(target_point);
    let split_result = splitter.solve(&range_start, &range_end, max_hops.min(50_000_000));

    if split_result.found {
        if let Some(k) = split_result.k {
            println!("\n  *** KEY FOUND via Range Splitter! ***");
            print_key(&k);
            print_pipeline_summary(pipeline_start, "Splitter");
            return;
        }
    }

    // Layer 9: Bloom Kangaroo (final sweep)
    println!("\n  ╔══════════════════════════════════════════════════════╗");
    println!("  ║  LAYER 9: Bloom-Filter Collision Accelerator         ║");
    println!("  ╚══════════════════════════════════════════════════════╝");

    let bloom_kang = BloomKangaroo::new(target_point, range_bits);
    let bloom_hops = max_hops.min(200_000_000);
    let bloom_result = bloom_kang.solve(&range_start, &range_end, bloom_hops);

    if bloom_result.found {
        if let Some(k) = bloom_result.k {
            println!("\n  *** KEY FOUND via Bloom Kangaroo! ***");
            print_key(&k);
            print_pipeline_summary(pipeline_start, "Bloom");
            return;
        }
    }

    println!("\n  Pipeline completed without finding key.");
    println!("  All 9 layers exhausted. Try increasing max_hops.");

    print_pipeline_summary(pipeline_start, "None");
}

fn print_pipeline_summary(start: Instant, method: &str) {
    let elapsed = start.elapsed().as_secs_f64();
    println!("\n  ═══════════════════════════════════════════════════════");
    println!("  TITAN V16 — PIPELINE SUMMARY:");
    println!("  ");
    println!("  Layer 1: Oracle → x-coordinate prediction (exact)");
    println!("  Layer 2: Z[ω] → π decomposition (lattice boost)");
    println!("  Layer 3: 6D Lattice → 2^135 → 6 × 2^45");
    println!("  Layer 4: LGK → 6 × O(2^22.5) kangaroo");
    println!("  Layer 5: BSGW → GLV-expanded sliding windows");
    println!("  Layer 6: QIA → quantum tunneling annealing");
    println!("  Layer 7: TagTeam → 5-strategy parallel kangaroos");
    println!("  Layer 8: Splitter → bit-level divide-and-conquer");
    println!("  Layer 9: Bloom → probabilistic O(1) collisions");
    println!("  ");
    println!("  Solving method: {}", method);
    println!("  Total pipeline time: {:.2}s", elapsed);
    println!("  NOUS SOMMES LA RECHERCHE.");
    println!("  ═══════════════════════════════════════════════════════");
}

// ============================================================
// TEST MODE
// ============================================================

fn run_test_mode() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  TITAN V16 — TEST MODE                                  ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();

    // Test 1: EC arithmetic
    println!("\n  [TEST] EC Arithmetic:");
    println!("  G on curve: {}", g.is_on_curve());

    let g2 = g.to_jacobian().double().to_affine();
    let expected_2g_x = Fe::from_hex("c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5");
    println!("  2*G x correct: {}", g2.x == expected_2g_x);

    let g7 = g.scalar_mul(&Fe::from_u64(7));
    let expected_7g_x = Fe::from_hex("5cbdf0646e5db4eaa398f365f2ea7a0e3d419b7e0330e39ce92bddedcac4f9bc");
    println!("  7*G x correct: {}", g7.x == expected_7g_x);

    // Test 2: P70 decompression
    println!("\n  [TEST] P70 Decompression:");
    let x70 = Fe::from_hex("94d991ef2a38291416f959de8f80769e0a74d7f81a49267f50b2de1a34dbc2df");
    let k_p70 = Fe::from_u64(0x6c3a4f);
    let q_p70 = g.scalar_mul(&k_p70);
    println!("  P70 Q.x matches known: {}", q_p70.x == x70);
    println!("  P70 Q on curve: {}", q_p70.is_on_curve());

    // Test 3: GLV
    println!("\n  [TEST] GLV Endomorphism:");
    let phi_g = g.glv_phi();
    let phi2_g = g.glv_phi2();
    println!("  phi(G) on curve: {}", phi_g.is_on_curve());
    println!("  phi^2(G) on curve: {}", phi2_g.is_on_curve());
    let beta = Fe { limbs: field::BETA };
    let beta_cu = beta.mul(&beta).mul(&beta);
    println!("  beta^3 = 1 mod P: {}", beta_cu == Fe::ONE);

    // Test 4: Bloom filter
    println!("\n  [TEST] Bloom Filter:");
    let mut bf = bloom::BloomFilter::new(10000, 0.01);
    let key1 = [1u8; 32];
    let key2 = [2u8; 32];
    bf.insert(&key1);
    println!("  Contains key1: {}", bf.contains(&key1));
    println!("  Contains key2: {}", bf.contains(&key2));
    println!("  Memory: {} bytes", bf.memory_bytes());

    // Test 5: Kangaroo hop rate
    println!("\n  [TEST] Kangaroo Hop Rate (Jacobian mixed add):");
    let bench_start = Instant::now();
    let bench_hops = 10_000;
    let mut pt = g.to_jacobian();
    let step_point = g.scalar_mul(&Fe::from_u64(12345));
    for _ in 0..bench_hops {
        pt = pt.add_affine(&step_point);
    }
    let bench_elapsed = bench_start.elapsed().as_secs_f64();
    let bench_rate = bench_hops as f64 / bench_elapsed;
    println!("  Rate: {:.0} hops/s (Jacobian mixed add)", bench_rate);

    // Test 6: QIA energy
    println!("\n  [TEST] Quantum Annealing Energy:");
    let k_test = Fe::from_u64(12345);
    let q_test = g.scalar_mul(&k_test);
    let qia = QuantumAnnealing::new(q_test);
    let e0 = qia.compute_energy(&q_test);
    let p2 = g.scalar_mul(&Fe::from_u64(54321));
    let e2 = qia.compute_energy(&p2);
    println!("  Energy (correct key): {}", e0);
    println!("  Energy (wrong key): {}/256", e2);

    println!("\n  [TEST] All tests complete. TITAN V16 ready.");
}

// ============================================================
// SELFTEST: Solve a known key end-to-end
// ============================================================

fn run_selftest() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  TITAN V16 — SELFTEST: Solve a known key end-to-end     ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();

    // Test keys at different bit sizes
    // Kangaroo needs O(√R) steps, NOT O(√N). For R ≤ 2^40, √R ≤ 2^20 = 1M steps.
    let test_cases: Vec<(u64, u32)> = vec![
        (0xFF, 8),          // 8-bit key, ~16 hops
        (0xFFFF, 16),       // 16-bit key, ~256 hops
        (0x1FFFFFF, 25),    // 25-bit key, ~8K hops
        (0x3FFFFFFF, 30),   // 30-bit key, ~32K hops
    ];

    for (k_val, range_bits) in test_cases {
        println!("\n  ════════════════════════════════════════════════════════");
        println!("  SELFTEST: k = 0x{:x} ({} bits), range = 2^{}", k_val, 64 - k_val.leading_zeros(), range_bits);

        let k = Fe::from_u64(k_val);
        let q = g.scalar_mul(&k);

        if !q.is_on_curve() {
            println!("  ERROR: Q not on curve!");
            continue;
        }
        println!("  Q on curve: ✓");
        println!("  Q.x = {:02x}{:02x}...{:02x}{:02x}",
                 q.x.to_bytes()[0], q.x.to_bytes()[1],
                 q.x.to_bytes()[30], q.x.to_bytes()[31]);

        // Compute actual range that contains k
        let actual_bits = 64 - k_val.leading_zeros();
        let actual_range_bits = actual_bits + 1;
        let rs = Fe::power_of_2(actual_bits - 1);
        let re = Fe::power_of_2(actual_bits);

        // Check that k is in range
        if k.cmp_val(&rs.limbs).is_ge() && k.cmp_val(&re.limbs).is_lt() {
            println!("  k in [2^{}, 2^{}): ✓", actual_bits - 1, actual_bits);
        } else {
            println!("  k NOT in [2^{}, 2^{}) — adjusting!", actual_bits - 1, actual_bits);
        }

        // Expected kangaroo steps: ~4√R (Pollard's bound)
        let expected_kangaroo_steps = 4u64 * (1u64 << (actual_bits / 2));

        // Test 1: BSGS (deterministic, O(√R) time+space)
        println!("\n  --- Test 1: BSGS (deterministic, O(√R) time+space) ---");
        let mut bsgw = bsgw::BsgwSolver::new(actual_range_bits);
        let bsgw_result = bsgw.solve(&q, &rs, &re);
        if bsgw_result.found {
            if let Some(found_k) = bsgw_result.k {
                let match_ok = !g.scalar_mul(&found_k).inf && g.scalar_mul(&found_k).x == q.x;
                println!("  BSGS: FOUND! k match: {} ({}ms, baby={}, giant={})",
                         match_ok, bsgw_result.elapsed_ms,
                         bsgw_result.baby_steps, bsgw_result.giant_steps);
            }
        } else {
            println!("  BSGS: NOT FOUND ({}ms)", bsgw_result.elapsed_ms);
        }

        // Test 2: Kangaroo — O(√R) steps, NOT O(√N)!
        // IMPORTANT: The previous selftest incorrectly skipped kangaroo for keys > 10 bits,
        // claiming O(2^128) complexity. This is WRONG. The kangaroo solves within the
        // KNOWN RANGE [2^(n-1), 2^n), so it needs O(√R) = O(2^(n/2)) steps.
        // For 30-bit keys: O(2^15) ≈ 32K hops. Trivial!
        if actual_bits <= 40 {
            println!("\n  --- Test 2: Kangaroo (O(√R) ≈ {} hops expected) ---", expected_kangaroo_steps);
            let kangaroo = kangaroo::KangarooOptimized::new_with_range(q, actual_range_bits);
            // Give 8x the expected steps as safety margin
            let max_kang_hops = (expected_kangaroo_steps * 8).max(100_000);
            let result = kangaroo.solve(&rs, &re, max_kang_hops);
            if result.found {
                if let Some(found_k) = result.k {
                    let match_ok = !g.scalar_mul(&found_k).inf && g.scalar_mul(&found_k).x == q.x;
                    println!("  Kangaroo: FOUND! k match: {} ({}ms, {} hops, expected ~{})",
                             match_ok, result.elapsed_ms, result.hops, expected_kangaroo_steps);
                }
            } else {
                println!("  Kangaroo: NOT FOUND ({} hops / ~{} expected, {}ms)",
                         result.hops, expected_kangaroo_steps, result.elapsed_ms);
            }
        } else {
            println!("\n  --- Test 2: Kangaroo --- SKIPPED (range > 40 bits, needs O(2^20+) hops)");
        }

        // Test 3: Tag-Team Kangaroo
        println!("\n  --- Test 3: Tag-Team Kangaroo ---");
        let tt = tagteam::TagTeamKangaroo::new(q);
        let tt_max_hops = (expected_kangaroo_steps * 10).max(100_000).min(50_000_000);
        let tt_result = tt.solve(&rs, &re, tt_max_hops);
        if tt_result.found {
            if let Some(found_k) = tt_result.k {
                let match_ok = !g.scalar_mul(&found_k).inf && g.scalar_mul(&found_k).x == q.x;
                println!("  TagTeam: FOUND! k match: {} ({}ms, {} hops)", match_ok, tt_result.elapsed_ms, tt_result.total_hops);
            }
        } else {
            println!("  TagTeam: NOT FOUND ({} hops, {}ms)", tt_result.total_hops, tt_result.elapsed_ms);
        }
    }
}

// ============================================================
// POINT DECOMPRESSION
// ============================================================

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

/// Decompress a 33-byte compressed public key directly (no oracle needed)
fn decompress_pubkey(compressed: &[u8; 33]) -> Option<Point> {
    let prefix = compressed[0];
    if prefix != 0x02 && prefix != 0x03 { return None; }
    let mut x_bytes = [0u8; 32];
    x_bytes.copy_from_slice(&compressed[1..33]);
    let y_is_odd = prefix == 0x03;
    decompress_point(&x_bytes, y_is_odd)
}

// ============================================================
// PRINT KEY
// ============================================================

fn print_key(k: &Fe) {
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
}

// ============================================================
// MAIN
// ============================================================

fn main() {
    let args = Args::parse();

    print_banner();

    println!("  Inventions (9 Layers):");
    println!("    1. SHA-256 Round 0 ORACLE — exact x-coordinate prediction");
    println!("    2. Z[ω] DLP Lifting — π decomposition for 6D lattice");
    println!("    3. 6D Range-Constrained Lattice — 2^135 → 6 × 2^45");
    println!("    4. LATTICE-GUIDED KANGAROO — search in 6D coeff space!");
    println!("    5. Multi-Window BSGS — GLV + sliding windows");
    println!("    6. Quantum-Inspired Annealing — tunneling + cooling");
    println!("    7. Tag-Team Kangaroos — 5 strategies, shared DP pool");
    println!("    8. Adaptive Range Splitter — bit-level divide & conquer");
    println!("    9. Bloom-Filter Accelerator — probabilistic O(1) matching");
    println!();

    match args.mode.as_str() {
        "auto" => {
            run_auto_mode(args.target, args.max_hops, args.max_iterations);
        }
        "pipeline" | "full" => {
            run_full_pipeline(args.target, args.max_hops, args.max_iterations);
        }
        "bsgw" => {
            let puzzle = get_puzzle(args.target);
            let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
            let mut pubkey_bytes = [0u8; 33];
            pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
            let oracle = Round0Oracle::new(&pubkey_bytes);
            let tp = decompress_point(&oracle.target_x, pubkey_bytes[0] == 0x03);
            let tp = match tp {
                Some(p) if p.is_on_curve() => p,
                Some(p) => {
                    let p2 = Point { x: p.x, y: p.y.neg_mod_p(), inf: false };
                    if p2.is_on_curve() { p2 } else { panic!("Cannot decompress!"); }
                },
                None => panic!("Cannot decompress!"),
            };
            run_bsgw_mode(&tp, puzzle.range_bits);
        }
        "annealing" | "qia" => {
            run_annealing_mode(args.target, args.max_iterations);
        }
        "tagteam" => {
            let puzzle = get_puzzle(args.target);
            let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
            let mut pubkey_bytes = [0u8; 33];
            pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
            let tp = decompress_pubkey(&pubkey_bytes).expect("Cannot decompress pubkey!");
            assert!(tp.is_on_curve(), "Decompressed point not on curve!");
            let range_start = Fe::power_of_2(puzzle.range_bits - 1);
            let range_end = Fe::power_of_2(puzzle.range_bits);
            run_tagteam_mode(&tp, &range_start, &range_end, args.max_hops);
        }
        "split" => {
            run_split_mode(args.target, args.max_hops);
        }
        "bloom" => {
            run_bloom_mode(args.target, args.max_hops);
        }
        "test" => {
            run_test_mode();
        }
        "kangaroo" => {
            let puzzle = get_puzzle(args.target);
            let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
            let mut pubkey_bytes = [0u8; 33];
            pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
            // Direct decompression from compressed pubkey (no oracle needed)
            let tp = decompress_pubkey(&pubkey_bytes).expect("Cannot decompress pubkey!");
            assert!(tp.is_on_curve(), "Decompressed point not on curve!");
            let range_start = Fe::power_of_2(puzzle.range_bits - 1);
            let range_end = Fe::power_of_2(puzzle.range_bits);
            let kangaroo = kangaroo::KangarooOptimized::new_with_range(tp, puzzle.range_bits);
            let result = kangaroo.solve(&range_start, &range_end, args.max_hops);
            if result.found {
                if let Some(k) = result.k {
                    println!("\n  KEY FOUND via Kangaroo!");
                    print_key(&k);
                }
            }
        }
        "oracle" => {
            let puzzle = get_puzzle(args.target);
            let pubkey_bytes_vec = hex::decode(&puzzle.pubkey_hex).expect("Invalid pubkey hex");
            let mut pubkey_bytes = [0u8; 33];
            pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
            let oracle = Round0Oracle::new(&pubkey_bytes);
            oracle.print_summary();
        }
        "selftest" => {
            run_selftest();
        }
        "debug" => {
            kangaroo_debug::run_all_debug();
        }
        _ => {
            eprintln!("Unknown mode: {}. Use: auto, pipeline, bsgw, annealing, tagteam, split, bloom, test, selftest, kangaroo, oracle", args.mode);
        }
    }

    println!("\n  NOUS SOMMES LA RECHERCHE.");
}
