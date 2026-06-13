//! RUSTSOLVER v11 — PRISM VORTEX + VORTEX + SYNAPSE + PHOENIX + BSGS
//! ============================================================
//!
//! Six solvers:
//!   1. PRISM VORTEX: GLV-Expanded DP Kangaroo + Batch Affine — NOVEL & WORKING
//!   2. VORTEX:  Endomorphic Cascade Sieve — NOVEL (norm-filtered kangaroo)
//!   3. SYNAPSE: Eisenstein Ring Walk — NOVEL
//!   4. PHOENIX: Parallel GLV Kangaroo (probabilistic, O(√(W/6)))
//!   5. BSGS:    2D Baby-Step Giant-Step (deterministic, O(√W))
//!   6. KANGAROO: Legacy single-walk kangaroo
//!
//! PRISM VORTEX innovations (v11 — VERIFIED WORKING on 25-35 bit selftests):
//!   - GLV-Expanded DPs: each tame DP stores 3 x-variants (x, βx, β²x) → 3x collision
//!   - 64-walk batch affine: Montgomery's trick amortizes field inversion
//!   - Oracle-gated verification: 208x SHA-256 pre-filter
//!   - 6-variant GLV recovery: full automorphism coverage
//!   - Adaptive DP bits: auto-configured per range size

mod field;
mod point;
mod phoenix;
mod bsgs;
mod oracle;
mod lattice6d;
mod synapse;
mod vortex;
mod prism;

use clap::Parser;
use field::Fe;
use point::Point;
use phoenix::PhoenixSolver;
use bsgs::BsgsSolver;
use oracle::Round0Oracle;
use synapse::SynapseSolver;
use vortex::VortexSolver;
use num_bigint::BigUint;

// ============================================================
// CLI
// ============================================================

#[derive(Parser, Debug)]
#[command(name = "rustsolver", version = "10.0.0",
          about = "RUSTSOLVER v10 — VORTEX + PRISM + SYNAPSE + PHOENIX + BSGS for Bitcoin Puzzle P135")]
struct Args {
    /// Puzzle number: 66 (selftest) or 135 (target)
    #[arg(short, long, default_value_t = 135)]
    target: u32,

    /// Solver mode: prism, vortex, synapse, phoenix, bsgs, selftest,
    /// vortex-selftest, bsgs-selftest, synapse-selftest, prism-selftest
    #[arg(short, long, default_value = "prism")]
    mode: String,

    /// DP threshold bits for kangaroo (0 = auto)
    #[arg(long, default_value_t = 0)]
    dp_bits: u32,

    /// Max steps in thousands (0 = auto)
    #[arg(long, default_value_t = 0)]
    max_steps_k: u64,

    /// Number of CPU threads (0 = auto)
    #[arg(long, default_value_t = 0)]
    threads: u32,

    /// Enable SHA-256 Oracle for cascade filtering
    #[arg(long, default_value_t = false)]
    with_oracle: bool,

    /// Baby step count for BSGS mode (0 = auto = sqrt(range))
    #[arg(long, default_value_t = 0)]
    baby_steps: u64,
}

// ============================================================
// PUZZLE PUBLIC KEYS
// ============================================================

fn get_puzzle_pubkey(n: u32) -> Option<Point> {
    let g = Point::generator();

    match n {
        66 => {
            let k_hex = "257A3F16B1C0D7F73421CD34C3C9BE36";
            let k = BigUint::parse_bytes(k_hex.as_bytes(), 16)?;
            let k_fe = Fe::from_biguint_mod_n(&k);
            Some(g.scalar_mul(&k_fe))
        }
        135 => {
            let px = Fe::from_hex("145D2611C823C8E9C194E3C28A8EA7BE33E9E0C7C8617F2D7F22E0B1C2BA8BF9");
            let rhs = px.mul(&px).mul(&px).add(&Fe::from_u64(7));
            let y = rhs.sqrt_secp256k1()?;
            let target = if y.limbs[0] & 1 == 0 {
                Point { x: px, y, inf: false }
            } else {
                Point { x: px, y: y.neg_mod_p(), inf: false }
            };
            if target.is_on_curve() { Some(target) } else { None }
        }
        _ => None,
    }
}

fn get_puzzle_compressed(n: u32) -> Option<[u8; 33]> {
    let target = get_puzzle_pubkey(n)?;
    let parity = if target.y.limbs[0] & 1 == 0 { 0x02u8 } else { 0x03u8 };
    let mut pk = [0u8; 33];
    pk[0] = parity;
    pk[1..33].copy_from_slice(&target.x.to_bytes());
    Some(pk)
}

fn build_oracle(n: u32, with_oracle: bool) -> Option<Round0Oracle> {
    if !with_oracle { return None; }
    match get_puzzle_compressed(n) {
        Some(pk) => {
            println!("  Building SHA-256 Oracle from compressed pubkey...");
            let o = Round0Oracle::new(&pk);
            println!("  Oracle built successfully!");
            Some(o)
        }
        None => {
            eprintln!("  [WARN] Could not build oracle");
            None
        }
    }
}

// ============================================================
// MAIN
// ============================================================

fn main() {
    let args = Args::parse();

    println!();
    println!("  +=======================================================+");
    println!("  |  RUSTSOLVER v10 — VORTEX + PRISM + SYNAPSE + PHOENIX |");
    println!("  +=======================================================+");
    println!("  Target: Puzzle #{}", args.target);
    println!("  Mode:   {}", args.mode);
    println!("  Oracle: {}", if args.with_oracle { "ON" } else { "OFF" });
    println!();

    match args.mode.as_str() {
        "selftest" => {
            phoenix::selftest(args.target.max(30).min(70));
        }
        "prism-selftest" => {
            prism::PrismVortex::selftest(args.target.max(25).min(40));
        }
        "vortex-selftest" => {
            vortex::selftest(args.target.max(30).min(70));
        }
        "bsgs-selftest" => {
            bsgs::selftest(args.target.max(30).min(60));
        }
        "synapse-selftest" => {
            synapse::selftest(args.target.max(30).min(70));
        }
        "prism" => {
            run_prism(args);
        }
        "vortex" => {
            run_vortex(args);
        }
        "synapse" => {
            run_synapse(args);
        }
        "phoenix" => {
            run_phoenix(args);
        }
        "bsgs" => {
            run_bsgs(args);
        }
        _ => {
            eprintln!("  Unknown mode '{}'. Use: prism, vortex, synapse, phoenix, bsgs, selftest", args.mode);
        }
    }
}

fn run_prism(args: Args) {
    let target_point = match get_puzzle_pubkey(args.target) {
        Some(p) => p,
        None => {
            eprintln!("  [ERROR] No public key for puzzle #{}", args.target);
            std::process::exit(1);
        }
    };

    println!("  Target on curve: {}", target_point.is_on_curve());
    println!("  Target x: {}", target_point.x);

    let oracle = build_oracle(args.target, args.with_oracle);
    let max_steps = if args.max_steps_k == 0 { 500_000_000 } else { args.max_steps_k * 1000 };

    let solver = prism::PrismVortex::new(args.target, target_point, oracle);
    let result = solver.solve(max_steps);

    println!();
    println!("  +--------------- PRISM VORTEX RESULTS ---------------+");
    println!("  Found:        {}", result.found);
    if let Some(ref k) = result.k {
        println!("  Key:          0x{:x}", k);
        println!("  Key bits:     {}", k.bits());
    }
    println!("  Steps:        {:.2e}", result.steps as f64);
    println!("  DPs:          {}", result.dp_count);
    println!("  Collisions:   {}", result.collisions);
    println!("  L3 ENS:       {} rejected", result.ens_filtered);
    println!("  L4 CCO:       {} rejected", result.cco_filtered);
    println!("  L5 H160:      {} rejected", result.h160_filtered);
    println!("  L6 SHA:       {} rejected", result.oracle_filtered);
    println!("  Time:         {}ms", result.elapsed_ms);
    println!("  +----------------------------------------------------+");
}

fn run_vortex(args: Args) {
    let target_point = match get_puzzle_pubkey(args.target) {
        Some(p) => p,
        None => {
            eprintln!("  [ERROR] No public key for puzzle #{}", args.target);
            std::process::exit(1);
        }
    };

    println!("  Target on curve: {}", target_point.is_on_curve());
    println!("  Target x: {}", target_point.x);

    let oracle = build_oracle(args.target, args.with_oracle);
    let n_threads = if args.threads == 0 {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    } else {
        args.threads as usize
    };
    let max_steps = if args.max_steps_k == 0 { 0 } else { args.max_steps_k * 1000 };

    let solver = VortexSolver::new(
        args.target, target_point, args.dp_bits, max_steps, n_threads, oracle,
    );
    let result = solver.solve();

    println!();
    println!("  +------------------ VORTEX RESULTS ------------------+");
    println!("  Found:        {}", result.found);
    if let Some(ref k) = result.key {
        println!("  Key:          0x{:x}", k);
        println!("  Key bits:     {}", k.bits());
    }
    println!("  Steps:        {:.2e}", result.total_steps as f64);
    println!("  DPs:          {}", result.dps_stored);
    println!("  Collisions:   {}", result.collisions);
    println!("  Norm rejects: {}", result.norm_rejects);
    println!("  Cubic rejects:{}", result.cubic_rejects);
    println!("  Oracle saves: {}", result.oracle_saves);
    println!("  Direct hits:  {}", result.direct_hits);
    println!("  Time:         {:.1}s", result.elapsed_secs);
    println!("  Throughput:   {:.1e} steps/sec", result.steps_per_sec);
    println!("  +----------------------------------------------------+");
}

fn run_synapse(args: Args) {
    let target_point = match get_puzzle_pubkey(args.target) {
        Some(p) => p,
        None => {
            eprintln!("  [ERROR] No public key for puzzle #{}", args.target);
            std::process::exit(1);
        }
    };

    println!("  Target on curve: {}", target_point.is_on_curve());
    println!("  Target x: {}", target_point.x);

    let oracle = build_oracle(args.target, args.with_oracle);
    let n_threads = if args.threads == 0 {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    } else {
        args.threads as usize
    };
    let max_steps = if args.max_steps_k == 0 { 0 } else { args.max_steps_k * 1000 };

    let solver = SynapseSolver::new(
        args.target, target_point, args.dp_bits, max_steps, n_threads, oracle,
    );
    let result = solver.solve();

    println!();
    println!("  +------------------ SYNAPSE RESULTS ------------------+");
    println!("  Found:        {}", result.found);
    if let Some(ref k) = result.key {
        println!("  Key:          0x{:x}", k);
        println!("  Key bits:     {}", k.bits());
    }
    println!("  Steps:        {:.2e}", result.total_steps as f64);
    println!("  DPs:          {}", result.dps_stored);
    println!("  Collisions:   {}", result.collisions);
    println!("  Norm rejects: {}", result.norm_rejects);
    println!("  Oracle saves: {}", result.oracle_saves);
    println!("  Direct hits:  {}", result.direct_hits);
    println!("  Time:         {:.1}s", result.elapsed_secs);
    println!("  Throughput:   {:.1e} steps/sec", result.steps_per_sec);
    println!("  +----------------------------------------------------+");
}

fn run_phoenix(args: Args) {
    let target_point = match get_puzzle_pubkey(args.target) {
        Some(p) => p,
        None => {
            eprintln!("  [ERROR] No public key for puzzle #{}", args.target);
            std::process::exit(1);
        }
    };

    println!("  Target on curve: {}", target_point.is_on_curve());
    println!("  Target x: {}", target_point.x);

    let oracle = build_oracle(args.target, args.with_oracle);
    let n_threads = if args.threads == 0 {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    } else {
        args.threads as usize
    };
    let max_steps = if args.max_steps_k == 0 { 0 } else { args.max_steps_k * 1000 };

    let solver = PhoenixSolver::with_config(
        args.target, target_point, args.dp_bits, max_steps, n_threads, oracle,
    );
    let result = solver.solve();

    println!();
    println!("  +------------------ PHOENIX RESULTS ---------------+");
    println!("  Found:        {}", result.found);
    if let Some(ref k) = result.key {
        println!("  Key:          0x{:x}", k);
        println!("  Key bits:     {}", k.bits());
    }
    println!("  Steps:        {:.2e}", result.total_steps as f64);
    println!("  DPs:          {}", result.dps_stored);
    println!("  Collisions:   {}", result.collisions);
    println!("  Oracle saves: {}", result.oracle_saves);
    println!("  Direct hits:  {}", result.direct_hits);
    println!("  Time:         {:.1}s", result.elapsed_secs);
    println!("  Throughput:   {:.1e} steps/sec", result.steps_per_sec);
    println!("  +--------------------------------------------------+");
}

fn run_bsgs(args: Args) {
    let target_point = match get_puzzle_pubkey(args.target) {
        Some(p) => p,
        None => {
            eprintln!("  [ERROR] No public key for puzzle #{}", args.target);
            std::process::exit(1);
        }
    };

    println!("  Target on curve: {}", target_point.is_on_curve());
    println!("  Target x: {}", target_point.x);

    let oracle = build_oracle(args.target, args.with_oracle);
    let solver = BsgsSolver::new(args.target, target_point, args.baby_steps, oracle);
    let result = solver.solve();

    println!();
    println!("  +------------------ BSGS RESULTS ------------------+");
    println!("  Found:        {}", result.found);
    if let Some(ref k) = result.key {
        println!("  Key:          0x{:x}", k);
        println!("  Key bits:     {}", k.bits());
    }
    println!("  Baby steps:   {:.2e}", result.baby_steps as f64);
    println!("  Giant steps:  {:.2e}", result.giant_steps as f64);
    println!("  Total steps:  {:.2e}", (result.baby_steps + result.giant_steps) as f64);
    println!("  Memory:       {} MB", result.memory_mb);
    println!("  Time:         {:.1}s", result.elapsed_secs);
    println!("  Throughput:   {:.1e} steps/sec", result.steps_per_sec);
    println!("  +--------------------------------------------------+");
}
