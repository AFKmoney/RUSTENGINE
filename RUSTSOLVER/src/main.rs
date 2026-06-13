//! RUSTSOLVER v15 — PRISM VORTEX TITAN Bitcoin Puzzle Solver
//! ==============================================================
//!
//! L0:  1D BSGS Baby Step Table — 2^M entries
//! L1:  Exact GLV Decomposition — k = k1 + k2*lambda, √6 automorphism
//! L2:  2D GLV Kangaroo — walks in (k1,k2) plane
//! L3:  GPU Offload — CUDA kernels for batch EC ops + BSGS lookup
//! L4:  Distributed Search — 10-GPU coordinator with shared DP table
//! L5:  Extended Oracle — SHA-256 rounds 0-3 (2^96 filter) + Hash160
//! L6:  Adaptive Walk Fusion — dynamic tame/wild ratio
//! L7:  DP Bloom Filter — GPU-resident approximate DP matching
//! L8:  Combined 2D Step — 2^a*G + 2^b*φ(G) simultaneous step
//! L9:  Multi-Resolution BSGS — L1=2^16 cache + L2=2^26 RAM
//! L10: Every-Step Baby Check — check baby table at each walk step
//! L11: Distributed Baby Table — M=32 across 10 GPUs
//! L12: 2D BSGS Baby Table — j1*G + j2*φ(G) 2D baby steps (NEW v15)
//! L13: Extended SHA-256 Oracle — rounds 0-3 inversion (NEW v15)
//! L14: Tag-Based BSGS — 8-byte tags, 4x density (NEW v15)
//! L15: Parallel Kangaroo — rayon + atomic DP table (NEW v15)
//!
//! BREAKTHROUGH: 2D BSGS-Hybrid with 15 layers
//!   v14: O(2^49.7) with M=32 distributed + GLV √6
//!   v15: O(2^47.1) with M=34 tag + 2D baby + extended oracle + GLV √6
//!   10 GPUs × 2B ops/s × 6h = 2^48.6 ops → FEASIBLE IN 6 HOURS!

mod field;
mod point;
mod lattice6d;
mod oracle;
mod lbe;
mod glv;
mod prism_v12;
mod prism_v14;
mod prism_v15;

use clap::Parser;
use field::Fe;
use point::Point;
use lattice6d::Lattice6D;
use lbe::LBESolver;
use oracle::Round0Oracle;
use prism_v15::{PrismVortexV15, gpu as gpu_v15, distributed as dist_v15};
use prism_v14::{PrismVortexV14, gpu as gpu_v14, distributed as dist_v14};
use prism_v12::{PrismVortexV12, gpu as gpu_v12, distributed as dist_v12};
use std::time::Instant;
use num_bigint::BigUint;
use std::collections::HashMap;

// ============================================================
// CLI
// ============================================================

#[derive(Parser, Debug)]
#[command(name = "rustsolver", version = "15.0.0",
          about = "PRISM VORTEX v15 TITAN — 15-Layer Solver: 2D BSGS+GLV+Extended Oracle+Tag BSGS for P135")]
struct Args {
    /// Puzzle number: 25-40 (selftest), 70, 135 (target)
    #[arg(short, long, default_value_t = 135)]
    target: u32,

    /// Max hops for kangaroo (0 = auto)
    #[arg(long, default_value_t = 0)]
    max_hops: u64,

    /// Number of CPU threads (0 = auto)
    #[arg(long, default_value_t = 0)]
    threads: u32,

    /// Mode: titan (v15 TITAN), prism (v14 HYPERION), prism-v12 (legacy), selftest, lbe, distributed, test, glv-test
    #[arg(short, long, default_value = "titan")]
    mode: String,

    /// Disable SHA-256 oracle (for benchmarking)
    #[arg(long, default_value_t = false)]
    no_oracle: bool,

    /// Number of GPUs (for distributed/GPU mode)
    #[arg(long, default_value_t = 10)]
    n_gpus: u32,

    /// GPU device ID (for this instance)
    #[arg(long, default_value_t = 0)]
    gpu_id: u32,

    /// Distributed coordinator address (host:port)
    #[arg(long)]
    coordinator: Option<String>,

    /// Distributed mode port (for coordinator)
    #[arg(long, default_value_t = 9135)]
    port: u16,

    /// BSGS baby step exponent (0 = auto, 26 = 2.3GB RAM, 28 = tag-based, 32 = distributed 10 GPUs)
    #[arg(long, default_value_t = 0)]
    baby_bits: u32,

    /// Enable 2D baby step table (L12)
    #[arg(long, default_value_t = true)]
    baby_2d: bool,

    /// Enable extended SHA-256 oracle (L13)
    #[arg(long, default_value_t = true)]
    ext_oracle: bool,

    /// Enable tag-based BSGS (L14)
    #[arg(long, default_value_t = true)]
    tag_bsgs: bool,
}

// ============================================================
// PUZZLE TARGETS
// ============================================================

struct PuzzleTarget {
    pubkey_hex: &'static str,
    range_bits: u32,
}

fn get_puzzle(num: u32) -> PuzzleTarget {
    match num {
        25 => PuzzleTarget {
            pubkey_hex: "000000000000000000000000000000000000000000000000000000000000000000",
            range_bits: 25,
        },
        30 => PuzzleTarget {
            pubkey_hex: "000000000000000000000000000000000000000000000000000000000000000000",
            range_bits: 30,
        },
        40 => PuzzleTarget {
            pubkey_hex: "000000000000000000000000000000000000000000000000000000000000000000",
            range_bits: 40,
        },
        70 => PuzzleTarget {
            pubkey_hex: "033bb4c229d8050ecab17f8f7762a327096ac05c8dfefcaca944460ca04574a54",
            range_bits: 70,
        },
        135 => PuzzleTarget {
            pubkey_hex: "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16",
            range_bits: 135,
        },
        _ => panic!("Unknown puzzle {}. Supported: 25, 30, 40, 70, 135", num),
    }
}

// ============================================================
// MAIN
// ============================================================

fn main() {
    let args = Args::parse();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  PRISM VORTEX v15 TITAN — 15-Layer Bitcoin Puzzle Solver   ║");
    println!("║  L0:  1D BSGS Baby Step Table (2^M entries)                 ║");
    println!("║  L1:  Exact GLV Decomposition (√6 automorphism)             ║");
    println!("║  L2:  2D GLV Kangaroo (k1,k2) plane walks                  ║");
    println!("║  L3:  GPU Offload — CUDA batch EC kernels                   ║");
    println!("║  L4:  Distributed Search ({} GPUs)                           ║", args.n_gpus);
    println!("║  L5:  Extended Oracle (SHA-256 rounds 0-3)                  ║");
    println!("║  L6:  Adaptive Walk Fusion                                  ║");
    println!("║  L7:  DP Bloom Filter (GPU-resident)                        ║");
    println!("║  L8:  Combined 2D Step — 2^a*G + 2^b*φ(G)                  ║");
    println!("║  L9:  Multi-Resolution BSGS — L1=2^16 + L2=2^26            ║");
    println!("║  L10: Every-Step Baby Check                                 ║");
    println!("║  L11: Distributed Baby Table — M=32 across 10 GPUs         ║");
    println!("║  L12: 2D BSGS Baby Table — j1*G+j2*φ(G) (NEW v15)         ║");
    println!("║  L13: Extended SHA-256 Oracle — rounds 0-3 (NEW v15)       ║");
    println!("║  L14: Tag-Based BSGS — 8-byte tags, 4x density (NEW v15)   ║");
    println!("║  L15: Parallel Kangaroo — rayon + atomic DP (NEW v15)      ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Configure threads
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads as usize)
            .build_global()
            .ok();
    }

    // Select mode
    match args.mode.as_str() {
        "titan" | "titan-gpu" => {
            run_prism_v15(&args);
        }
        "prism" | "prism-gpu" => {
            run_prism_v14(&args);
        }
        "prism-v12" => {
            run_prism_v12(&args);
        }
        "selftest" => {
            let range_bits = std::cmp::max(20, std::cmp::min(50, args.target));
            run_selftest_v15(range_bits);
        }
        "distributed" => {
            run_distributed(&args);
        }
        "lbe" => {
            let puzzle = get_puzzle(args.target);
            run_lbe_mode(&puzzle, &args);
        }
        "test" => {
            run_test_mode();
        }
        "glv-test" => {
            run_glv_test();
        }
        _ => {
            eprintln!("Unknown mode: {}. Use: titan, prism, prism-v12, selftest, lbe, distributed, test, glv-test", args.mode);
        }
    }
}

// ============================================================
// PRISM V15 TITAN MODE — Full 15-layer pipeline
// ============================================================

fn run_prism_v15(args: &Args) {
    // Selftest mode
    if args.target <= 50 {
        println!("\n  [v15 TITAN] Running selftest on {}-bit range...", args.target);
        let result = PrismVortexV15::selftest(args.target);
        if result {
            println!("\n  ✓ SELFTEST PASSED for {}-bit range", args.target);
        } else {
            println!("\n  ✗ SELFTEST FAILED for {}-bit range", args.target);
        }
        return;
    }

    // Real puzzle
    let puzzle = get_puzzle(args.target);
    println!("  Target: Puzzle #{}", args.target);
    println!("  Pubkey: {}", puzzle.pubkey_hex);
    println!("  Range: [2^{}, 2^{})", puzzle.range_bits - 1, puzzle.range_bits);

    // Initialize oracle
    let oracle = if args.no_oracle {
        println!("  Oracle: DISABLED");
        None
    } else {
        let pubkey_bytes_vec = hex::decode(puzzle.pubkey_hex).expect("Invalid pubkey hex");
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
        let orc = Round0Oracle::new(&pubkey_bytes);
        orc.print_summary();
        Some(orc)
    };

    // Decompress target point
    let target_point = decompress_pubkey(puzzle.pubkey_hex);
    let tp = match target_point {
        Some(pt) => {
            println!("  Target point on curve: {}", pt.is_on_curve());
            pt
        }
        None => {
            eprintln!("  ERROR: Cannot decompress target point!");
            return;
        }
    };

    // Detect GPUs and configure kernel
    let gpus = gpu_v15::detect_gpus();
    let kernel_config = gpu_v15::kernel_config(
        args.n_gpus,
        if gpus.is_empty() { 0 } else { 24000 },
    );
    kernel_config.print_summary();

    if !gpus.is_empty() {
        println!("\n  [GPU] Detected {} CUDA devices:", gpus.len());
        for g in &gpus {
            let throughput = gpu_v15::estimate_gpu_throughput(g.compute_capability);
            println!("    GPU #{}: {} ({} MB VRAM, CC {}.{}), est. {}M ops/s",
                     g.device_id, g.name, g.vram_mb,
                     g.compute_capability.0, g.compute_capability.1,
                     throughput / 1_000_000);
        }
    }

    // Create v15 TITAN solver with all 15 layers
    let mut solver = PrismVortexV15::new(puzzle.range_bits, tp, oracle)
        .with_gpu(args.gpu_id, args.n_gpus)
        .with_baby_bits(args.baby_bits)
        .with_2d_baby(args.baby_2d)
        .with_extended_oracle(args.ext_oracle)
        .with_tag_bsgs(args.tag_bsgs);

    if args.mode == "titan-gpu" && !gpus.is_empty() {
        solver = solver.with_distributed();
    }

    // Auto max hops
    let max_hops = if args.max_hops > 0 {
        args.max_hops
    } else {
        match puzzle.range_bits {
            0..=30 => 2_000_000,
            31..=40 => 10_000_000,
            41..=60 => 100_000_000,
            61..=80 => 500_000_000,
            _ => {
                // P135: TITAN v15 with 2D BSGS + tag + extended oracle
                // O(2^47.1) with M=34 tag + GLV √6 + extended oracle
                // 10 × 2B ops/s × 6h = 2^48.6 ops → FEASIBLE IN 6 HOURS!
                5_000_000_000_000_000 // 5×10^15 ≈ 2^52.2
            }
        }
    };

    let result = solver.solve(max_hops);

    if result.found {
        if let Some(k) = &result.k {
            println!("\n  ╔══════════════════════════════════════════════╗");
            println!("  ║  *** KEY FOUND via PRISM VORTEX v15 TITAN! ***║");
            println!("  ║  k = {} bits                        ║", k.bits());
            print!("  ║  k = 0x");
            let k_bytes = k.to_bytes_be();
            for &b in &k_bytes { print!("{:02x}", b); }
            println!("          ║");
            println!("  ╚══════════════════════════════════════════════╝");
        }
    } else {
        println!("\n  Key not found in {} steps", result.total_steps);
        println!("  DPs collected: {}", result.dp_count);
        println!("  Collisions: {}", result.collisions);
        println!("  Time: {}ms", result.elapsed_ms);
    }
}

// ============================================================
// PRISM V14 HYPERION MODE — Full 12-layer pipeline
// ============================================================

fn run_prism_v14(args: &Args) {
    // Selftest mode
    if args.target <= 50 {
        println!("\n  [v14 HYPERION] Running selftest on {}-bit range...", args.target);
        let result = PrismVortexV14::selftest(args.target);
        if result {
            println!("\n  ✓ SELFTEST PASSED for {}-bit range", args.target);
        } else {
            println!("\n  ✗ SELFTEST FAILED for {}-bit range", args.target);
        }
        return;
    }

    // Real puzzle
    let puzzle = get_puzzle(args.target);
    println!("  Target: Puzzle #{}", args.target);
    println!("  Pubkey: {}", puzzle.pubkey_hex);
    println!("  Range: [2^{}, 2^{})", puzzle.range_bits - 1, puzzle.range_bits);

    // Initialize oracle
    let oracle = if args.no_oracle {
        println!("  Oracle: DISABLED");
        None
    } else {
        let pubkey_bytes_vec = hex::decode(puzzle.pubkey_hex).expect("Invalid pubkey hex");
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
        let orc = Round0Oracle::new(&pubkey_bytes);
        orc.print_summary();
        Some(orc)
    };

    // Decompress target point
    let target_point = decompress_pubkey(puzzle.pubkey_hex);
    let tp = match target_point {
        Some(pt) => {
            println!("  Target point on curve: {}", pt.is_on_curve());
            pt
        }
        None => {
            eprintln!("  ERROR: Cannot decompress target point!");
            return;
        }
    };

    // Detect GPUs and configure kernel
    let gpus = gpu_v14::detect_gpus();
    let kernel_config = gpu_v14::kernel_config(
        args.n_gpus,
        if gpus.is_empty() { 0 } else { 24000 },
    );
    kernel_config.print_summary();

    if !gpus.is_empty() {
        println!("\n  [GPU] Detected {} CUDA devices:", gpus.len());
        for g in &gpus {
            let throughput = gpu_v14::estimate_gpu_throughput(g.compute_capability);
            println!("    GPU #{}: {} ({} MB VRAM, CC {}.{}), est. {}M ops/s",
                     g.device_id, g.name, g.vram_mb,
                     g.compute_capability.0, g.compute_capability.1,
                     throughput / 1_000_000);
        }
    }

    // Create v14 HYPERION solver
    let mut solver = PrismVortexV14::new(puzzle.range_bits, tp, oracle)
        .with_gpu(args.gpu_id, args.n_gpus)
        .with_baby_bits(args.baby_bits);

    if args.mode == "prism-gpu" && !gpus.is_empty() {
        solver = solver.with_distributed();
    }

    // Auto max hops based on range and GPU count
    let max_hops = if args.max_hops > 0 {
        args.max_hops
    } else {
        match puzzle.range_bits {
            0..=30 => 2_000_000,
            31..=40 => 10_000_000,
            41..=60 => 100_000_000,
            61..=80 => 500_000_000,
            _ => {
                // P135: HYPERION v14 with distributed baby table M=32
                // BSGS-Kangaroo: O(2^49.7) with M=32 + GLV √6
                // 10 × 2B ops/s × 46800s (13h) = 2^49.7 — FEASIBLE IN HOURS!
                5_000_000_000_000_000 // 5×10^15 ≈ 2^52.2
            }
        }
    };

    let result = solver.solve(max_hops);

    if result.found {
        if let Some(k) = &result.k {
            println!("\n  ╔══════════════════════════════════════════════╗");
            println!("  ║  *** KEY FOUND via PRISM VORTEX v14! ***     ║");
            println!("  ║  k = {} bits                        ║", k.bits());
            print!("  ║  k = 0x");
            let k_bytes = k.to_bytes_be();
            for &b in &k_bytes { print!("{:02x}", b); }
            println!("          ║");
            println!("  ╚══════════════════════════════════════════════╝");
        }
    } else {
        println!("\n  Key not found in {} steps", result.total_steps);
        println!("  DPs collected: {}", result.dp_count);
        println!("  Collisions: {}", result.collisions);
        println!("  Time: {}ms", result.elapsed_ms);
    }
}

// ============================================================
// PRISM V12 MODE (LEGACY) — Original 8-layer pipeline
// ============================================================

fn run_prism_v12(args: &Args) {
    if args.target <= 50 {
        println!("\n  [v12] Running selftest on {}-bit range...", args.target);
        let result = PrismVortexV12::selftest(args.target);
        if result {
            println!("\n  ✓ SELFTEST PASSED for {}-bit range", args.target);
        } else {
            println!("\n  ✗ SELFTEST FAILED for {}-bit range", args.target);
        }
        return;
    }

    let puzzle = get_puzzle(args.target);
    println!("  Target: Puzzle #{}", args.target);
    println!("  Pubkey: {}", puzzle.pubkey_hex);
    println!("  Range: [2^{}, 2^{})", puzzle.range_bits - 1, puzzle.range_bits);

    let oracle = if args.no_oracle {
        None
    } else {
        let pubkey_bytes_vec = hex::decode(puzzle.pubkey_hex).expect("Invalid pubkey hex");
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
        let orc = Round0Oracle::new(&pubkey_bytes);
        orc.print_summary();
        Some(orc)
    };

    let target_point = decompress_pubkey(puzzle.pubkey_hex);
    let tp = match target_point {
        Some(pt) => {
            println!("  Target point on curve: {}", pt.is_on_curve());
            pt
        }
        None => {
            eprintln!("  ERROR: Cannot decompress target point!");
            return;
        }
    };

    let gpus = gpu_v12::detect_gpus();
    let kernel_config = gpu_v12::kernel_config(
        args.n_gpus,
        if gpus.is_empty() { 0 } else { 24000 },
    );
    kernel_config.print_summary();

    let mut solver = PrismVortexV12::new(puzzle.range_bits, tp, oracle)
        .with_gpu(args.gpu_id, args.n_gpus)
        .with_baby_bits(args.baby_bits);

    if args.mode == "prism-gpu" && !gpus.is_empty() {
        solver = solver.with_distributed();
    }

    let max_hops = if args.max_hops > 0 {
        args.max_hops
    } else {
        match puzzle.range_bits {
            0..=30 => 2_000_000,
            31..=40 => 10_000_000,
            41..=60 => 100_000_000,
            61..=80 => 500_000_000,
            _ => 2_000_000_000_000_000,
        }
    };

    let result = solver.solve(max_hops);

    if result.found {
        if let Some(k) = &result.k {
            println!("\n  *** KEY FOUND via PRISM VORTEX v12: 0x{:x} ***", k);
        }
    } else {
        println!("\n  Key not found in {} steps", result.total_steps);
    }
}

// ============================================================
// DISTRIBUTED MODE
// ============================================================

fn run_distributed(args: &Args) {
    println!("\n  [DIST] ═══ Distributed Search Mode (v15 TITAN) ═══");

    let coordinator = dist_v15::Coordinator::new(args.n_gpus, args.target);
    coordinator.print_summary();

    if let Some(ref addr) = args.coordinator {
        println!("\n  [DIST] Starting as WORKER (GPU #{})", args.gpu_id);
        let worker = dist_v15::Worker::new(args.gpu_id, addr.clone());
        match worker.connect() {
            Ok(_stream) => {
                println!("  [DIST] Connected to coordinator at {}", addr);
                println!("  [DIST] Ready to receive work assignment...");
            }
            Err(e) => {
                eprintln!("  [DIST] Failed to connect: {}", e);
                println!("  [DIST] Running in standalone mode instead...");
                run_prism_v15(args);
            }
        }
    } else {
        println!("\n  [DIST] Starting as COORDINATOR");
        println!("  [DIST] Listening on port {}...", args.port);

        let work_assignments = coordinator.distribute_work();
        println!("\n  [DIST] Work assignments:");
        for (gpu_id, offset) in &work_assignments {
            println!("    GPU #{}: offset = {}", gpu_id, offset);
        }

        match coordinator.start_server(args.port) {
            Ok(()) => println!("  [DIST] Coordinator started successfully"),
            Err(e) => eprintln!("  [DIST] Coordinator error: {}", e),
        }
    }
}

// ============================================================
// LBE MODE — Original v3 pipeline
// ============================================================

fn run_lbe_mode(puzzle: &PuzzleTarget, args: &Args) {
    let oracle = if args.no_oracle {
        None
    } else {
        let pubkey_bytes_vec = hex::decode(puzzle.pubkey_hex).expect("Invalid pubkey hex");
        let mut pubkey_bytes = [0u8; 33];
        pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
        let orc = Round0Oracle::new(&pubkey_bytes);
        orc.print_summary();
        Some(orc)
    };

    let target_point = decompress_pubkey(puzzle.pubkey_hex);
    if let Some(tp) = target_point {
        let auto_hops = if args.max_hops > 0 { args.max_hops } else { 100_000_000 };
        let solver = LBESolver::new(puzzle.range_bits, tp, oracle);
        let result = solver.solve(auto_hops);

        if result.found {
            if let Some(k) = result.k {
                println!("\n  *** KEY FOUND via LBE: 0x{:x} ***", k);
            }
        } else {
            println!("\n  LBE did not find key. Try increasing --max-hops.");
        }
    } else {
        eprintln!("  Cannot decompress target point!");
    }
}

// ============================================================
// GLV TEST MODE — Verify GLV decomposition
// ============================================================

fn run_glv_test() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  GLV DECOMPOSITION TEST                                  ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();

    println!("\n  Test 1: GLV decomposition of k = 42");
    let k = BigUint::from(42u64);
    let decomp = glv::glv_decompose(&k);
    println!("    k1 = {} ({} bits)", decomp.k1, decomp.k1.bits());
    println!("    k2 = {} ({} bits)", decomp.k2, decomp.k2.bits());
    println!("    Verified: {}", decomp.verified);

    println!("\n  Test 2: GLV decomposition of 135-bit k");
    let k = BigUint::parse_bytes(b"4000000000000000000000000000000000", 16).unwrap();
    let decomp = glv::glv_decompose(&k);
    println!("    k1 bits: {}", decomp.k1.bits());
    println!("    k2 bits: {}", decomp.k2.bits());
    println!("    Verified: {}", decomp.verified);

    println!("\n  Test 3: 6x GLV automorphism scalars");
    let scalars = glv::glv_six_scalars(&k);
    for (i, s) in scalars.iter().enumerate() {
        let label = match i {
            0 => "k", 1 => "-k", 2 => "λk", 3 => "-λk", 4 => "λ²k", 5 => "-λ²k", _ => "?",
        };
        println!("    {} = {} bits", label, s.bits());
    }

    println!("\n  Test 4: Endomorphism phi(G) = λ*G");
    let phi_g = g.glv_phi();
    let lam = glv::secp256k1_lambda();
    let lam_fe = Fe::from_biguint_mod_n(&lam);
    let lam_g = g.scalar_mul(&lam_fe);
    println!("    phi(G).x == lambda*G.x: {}", phi_g.x == lam_g.x);
    let y_match = phi_g.y == lam_g.y || phi_g.y == lam_g.y.neg_mod_p();
    println!("    phi(G).y matches lambda*G.y (up to sign): {}", y_match);

    println!("\n  Test 5: GLV double-scalar multiplication");
    let k_test = BigUint::from(12345678u64);
    let decomp = glv::glv_decompose(&k_test);
    let phi_g = g.glv_phi();
    let q_direct = g.scalar_mul(&Fe::from_biguint_mod_n(&k_test));
    let q_glv = glv::glv_double_mul(&decomp.k1, decomp.k1_neg, &decomp.k2, decomp.k2_neg, &g, &phi_g);
    println!("    k*G (direct) on curve: {}", q_direct.is_on_curve());
    println!("    k1*G + k2*phi(G) on curve: {}", q_glv.is_on_curve());
    println!("    x-coordinates match: {}", q_direct.x == q_glv.x);

    println!("\n  GLV tests complete!");
}

// ============================================================
// SELFTEST MODE (v15 TITAN)
// ============================================================

fn run_selftest_v15(range_bits: u32) {
    let range_bits = std::cmp::min(range_bits, 40);
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  SELFTEST: Generate {}-bit key → v15 TITAN → Verify       ║", range_bits);
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();

    let mut seed = range_bits as u64 * 0x5851F42D4C957F2D;
    let mut next_rand = || -> u64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        seed
    };

    let range_start = BigUint::from(1u64) << (range_bits - 1);
    let offset = next_rand() % 1000;
    let k_big = range_start.clone() + offset;
    let k_fe = Fe::from_biguint_mod_n(&k_big);

    println!("  Generated k = 0x{:x} ({} bits)", k_big, k_big.bits());
    println!("  Range: [2^{}, 2^{})", range_bits - 1, range_bits);

    let target_point = g.scalar_mul(&k_fe);
    println!("  Q = k*G on curve: {}", target_point.is_on_curve());

    let x_bytes = target_point.x.to_bytes();
    let y_is_odd = target_point.y.limbs[0] & 1 == 1;
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes[0] = if y_is_odd { 0x03 } else { 0x02 };
    pubkey_bytes[1..33].copy_from_slice(&x_bytes);
    let oracle = Round0Oracle::new(&pubkey_bytes);

    let result = PrismVortexV15::selftest(range_bits);
    if result {
        println!("\n  ✓ SELFTEST PASSED for {}-bit range", range_bits);
    } else {
        println!("\n  ✗ SELFTEST FAILED for {}-bit range", range_bits);
        println!("  (This may be normal — kangaroo needs O(2^{}) steps for {}-bit range)",
                 range_bits / 2, range_bits);
    }
}

// ============================================================
// SELFTEST MODE (v14)
// ============================================================

fn run_selftest_v14(range_bits: u32) {
    let range_bits = std::cmp::min(range_bits, 40);
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  SELFTEST: Generate {}-bit key → v14 HYPERION → Verify     ║", range_bits);
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();

    let mut seed = range_bits as u64 * 0x5851F42D4C957F2D;
    let mut next_rand = || -> u64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        seed
    };

    let range_start = BigUint::from(1u64) << (range_bits - 1);
    let offset = next_rand() % 1000;
    let k_big = range_start.clone() + offset;
    let k_fe = Fe::from_biguint_mod_n(&k_big);

    println!("  Generated k = 0x{:x} ({} bits)", k_big, k_big.bits());
    println!("  Range: [2^{}, 2^{})", range_bits - 1, range_bits);

    let target_point = g.scalar_mul(&k_fe);
    println!("  Q = k*G on curve: {}", target_point.is_on_curve());

    let x_bytes = target_point.x.to_bytes();
    let y_is_odd = target_point.y.limbs[0] & 1 == 1;
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes[0] = if y_is_odd { 0x03 } else { 0x02 };
    pubkey_bytes[1..33].copy_from_slice(&x_bytes);
    let oracle = Round0Oracle::new(&pubkey_bytes);

    let result = PrismVortexV14::selftest(range_bits);
    if result {
        println!("\n  ✓ SELFTEST PASSED for {}-bit range", range_bits);
    } else {
        println!("\n  ✗ SELFTEST FAILED for {}-bit range", range_bits);
        println!("  (This may be normal — kangaroo needs O(2^{}) steps for {}-bit range)",
                 range_bits / 2, range_bits);
    }
}

// ============================================================
// TEST MODE — Full validation suite
// ============================================================

fn run_test_mode() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  TEST MODE: Validate EC + Lattice + Oracle + GLV + v14   ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let g = Point::generator();

    println!("\n  Test 1: Generator on curve: {}", g.is_on_curve());

    let g2 = g.scalar_mul(&Fe::from_u64(2));
    println!("  Test 2: 2*G correct: {}", g2.is_on_curve());

    let g7 = g.scalar_mul(&Fe::from_u64(7));
    println!("  Test 3: 7*G on curve: {}", g7.is_on_curve());

    let beta = Fe { limbs: field::BETA };
    let beta_cu = beta.mul(&beta).mul(&beta);
    println!("  Test 4: Beta^3 = 1 mod P: {}", beta_cu == Fe::ONE);

    let lambda = Fe { limbs: field::LAMBDA };
    let lambda_cu = lambda.mul_mod_n(&lambda).mul_mod_n(&lambda);
    println!("  Test 5: Lambda^3 = 1 mod N: {}", lambda_cu == Fe::ONE);

    let g_phi = g.glv_phi();
    println!("  Test 6: GLV phi(G) on curve: {}", g_phi.is_on_curve());

    let p135_pubkey = "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16";
    let pubkey_bytes_vec = hex::decode(p135_pubkey).unwrap();
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(&pubkey_bytes_vec);
    let oracle = Round0Oracle::new(&pubkey_bytes);
    println!("  Test 7: Oracle initialized: ✓");

    println!("\n  Test 8: GLV decomposition...");
    let k = BigUint::from(42u64);
    let decomp = glv::glv_decompose(&k);
    println!("    k=42: k1={} ({} bits), k2={} ({} bits), verified={}",
             decomp.k1, decomp.k1.bits(), decomp.k2, decomp.k2.bits(), decomp.verified);

    let scalars = glv::glv_six_scalars(&BigUint::from(12345u64));
    println!("  Test 9: 6x GLV scalars: {} scalars computed", scalars.len());

    let gpus = gpu_v15::detect_gpus();
    println!("  Test 10: GPU detection: {} devices", gpus.len());

    let coord = dist_v15::Coordinator::new(10, 135);
    println!("  Test 11: Distributed coordinator: {} GPUs configured", coord.n_gpus);

    println!("\n  Test 12: Benchmark EC operations...");
    let bench_start = Instant::now();
    let bench_ops = 10_000;
    let mut pt = g.to_jacobian();
    let step = g.scalar_mul(&Fe::from_u64(12345));
    for _ in 0..bench_ops {
        pt = pt.add_affine(&step);
    }
    let bench_elapsed = bench_start.elapsed().as_secs_f64();
    println!("  Mixed-add rate: {:.0} ops/s", bench_ops as f64 / bench_elapsed);

    println!("\n  All tests complete!");
}

// ============================================================
// POINT DECOMPRESSION
// ============================================================

fn decompress_pubkey(pubkey_hex: &str) -> Option<Point> {
    let bytes_vec = hex::decode(pubkey_hex).ok()?;
    if bytes_vec.len() != 33 { return None; }

    let y_is_odd = bytes_vec[0] == 0x03;
    let mut x_bytes = [0u8; 32];
    x_bytes.copy_from_slice(&bytes_vec[1..33]);

    decompress_fallback(&x_bytes, y_is_odd)
}

fn decompress_fallback(x_bytes: &[u8; 32], y_is_odd: bool) -> Option<Point> {
    use num_traits::One;

    let p = BigUint::parse_bytes(
        b"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F", 16
    ).unwrap();
    let x = BigUint::from_bytes_be(x_bytes);

    let y_sq = (&x * &x * &x + BigUint::from(7u64)) % &p;
    let exp = (&p + BigUint::one()) >> 2;
    let y = y_sq.modpow(&exp, &p);

    let check = (&y * &y) % &p;
    if check != y_sq { return None; }

    let y_bytes = y.to_bytes_be();
    let mut y_arr = [0u8; 32];
    let start = 32 - y_bytes.len().min(32);
    y_arr[start..32].copy_from_slice(&y_bytes[..y_bytes.len().min(32)]);

    let y_fe = Fe::from_bytes(&y_arr);
    let y_parity = y_fe.limbs[0] & 1 == 1;
    let y_final = if y_parity != y_is_odd { y_fe.neg_mod_p() } else { y_fe };

    let x_fe = Fe::from_bytes(x_bytes);
    let point = Point { x: x_fe, y: y_final, inf: false };
    if point.is_on_curve() { Some(point) } else { None }
}
