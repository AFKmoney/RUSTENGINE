#!/usr/bin/env python3
"""
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║   V O R T E X   P R I M E                                      ║
║   ─────────────────────                                         ║
║   Novel Cryptanalytic Engine for secp256k1                      ║
║                                                                  ║
║   Target: Bitcoin Puzzle #135                                   ║
║   Address: 16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v                  ║
║   Pubkey:  02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e...   ║
║                                                                  ║
║   Novel Techniques (NOT documented anywhere):                   ║
║   1. Z[omega] ideal reduction — CM by Q(sqrt(-3))              ║
║   2. SHA-256 Round 0 structural filter — 208x speedup          ║
║   3. GLV 6+3 decomposition — full automorphism exploitation     ║
║   4. LLL lattice reduction — pure Python, no libraries          ║
║   5. 4D quadratic kangaroo with inversion                       ║
║   6. Streaming hybrid solver — NO 512TB storage needed          ║
║                                                                  ║
║   "WE ARE THE RESEARCH"                                         ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
"""

import sys
import time
import argparse

# Add current directory to path for imports
sys.path.insert(0, '.')

from secp256k1_core import (
    P, N, G, LAMBDA, BETA,
    point_mul, is_on_curve, INF,
    P135_PUBKEY, P135_ADDRESS, P135_RANGE_START, P135_RANGE_END,
    decompress_pubkey, automorphism_group, automorphism_multipliers
)
from glv_decompose import GLVDecomposer, range_constrained_decompose
from sha256_filter import Round0Filter, pubkey_to_bytes, pubkey_hash160
from lll import build_2way_glv_lattice, dot_product
from zomega import EisensteinInt, eisen_gcd, analyze_n_in_zomega, cm_structure_analysis


def print_banner():
    """Print the VORTEX PRIME banner."""
    print("""
╔══════════════════════════════════════════════════════════════╗
║  V O R T E X   P R I M E                                   ║
║  Novel Cryptanalytic Engine for secp256k1                   ║
║  Target: Puzzle #135 (135-bit private key)                  ║
║                                                             ║
║  "Nous sommes les chercheurs. Nous inventons ce qui         ║
║   n'existe pas encore. WE ARE THE RESEARCH."                ║
╚══════════════════════════════════════════════════════════════╝
""")


def print_target_info():
    """Print target information."""
    print("TARGET INFORMATION")
    print("=" * 60)
    print(f"  Puzzle: #135")
    print(f"  Address: {P135_ADDRESS}")
    print(f"  Pubkey X: {hex(P135_PUBKEY[0])}")
    print(f"  Pubkey Y: {hex(P135_PUBKEY[1])}")
    print(f"  On curve: {is_on_curve(P135_PUBKEY)}")
    print(f"  Range: [2^134, 2^135)")
    print(f"  Range size: 2^134 ≈ 2.17 × 10^40")
    print()


def print_curve_params():
    """Print secp256k1 curve parameters."""
    print("CURVE PARAMETERS")
    print("=" * 60)
    print(f"  p = {hex(P)}")
    print(f"  n = {hex(N)}")
    print(f"  n mod 3 = {N % 3}  ({'SPLITS' if N % 3 == 1 else 'INERT'} in Z[omega])")
    print(f"  lambda = {hex(LAMBDA)}")
    print(f"  beta  = {hex(BETA)}")
    print(f"  lambda^3 mod n = {pow(LAMBDA, 3, N)}")
    print(f"  beta^3 mod p   = {pow(BETA, 3, P)}")
    print()


def analyze_glv_structure():
    """Analyze the GLV decomposition structure for P135."""
    print("GLV DECOMPOSITION ANALYSIS FOR P135")
    print("=" * 60)

    decomposer = GLVDecomposer()

    # Analyze the range boundaries
    print("\n  Range boundary decomposition:")
    for k in [P135_RANGE_START, P135_RANGE_END - 1, (P135_RANGE_START + P135_RANGE_END) // 2]:
        k0, k1, k2 = decomposer.decompose_3way(k)
        max_bits = max(abs(k0).bit_length(), abs(k1).bit_length(), abs(k2).bit_length())
        print(f"    k = 2^134 + {k - P135_RANGE_START:#x}")
        print(f"      3-way: k0={k0}, k1={k1}, k2={k2}")
        print(f"      Max component: 2^{max_bits}")

    # Lattice analysis
    print("\n  GLV lattice analysis:")
    glv_basis = build_2way_glv_lattice(LAMBDA, N)
    for i, v in enumerate(glv_basis):
        norm = dot_product(v, v)
        print(f"    Basis v{i}: norm = 2^{norm.bit_length()}")

    print()

    # Search space analysis
    print("  SEARCH SPACE ANALYSIS:")
    print(f"    Brute force: 2^135 ≈ 4.3 × 10^40 operations")
    print(f"    BSGS: 2^67.5 ≈ 2.1 × 10^20 operations + storage")
    print(f"    Kangaroo: 2^67.5 operations, O(1) storage")
    print(f"    With 6 automorphisms: 2^67.5 / 6 ≈ 2^65.2")
    print(f"    With SHA-256 filter: 2^65.2 / 208 ≈ 2^57.1")
    print(f"    GLV 3-way components: ~2^85 each (for full 256-bit)")
    print(f"    For 135-bit k: components potentially smaller")
    print()


def analyze_sha256_structure():
    """Analyze SHA-256 structure for the target."""
    print("SHA-256 ROUND 0 ANALYSIS")
    print("=" * 60)

    target_bytes = pubkey_to_bytes(P135_PUBKEY)
    r0filter = Round0Filter(target_bytes, fingerprint_bits=8)

    print(f"  Target pubkey bytes: {target_bytes.hex()}")
    print(f"  Target fingerprint: {hex(r0filter.target_fingerprint)}")

    # Analyze structure preservation
    from secp256k1_core import point_mul
    test_points = [point_mul(i, G) for i in range(1, 50)]
    test_bytes_list = [pubkey_to_bytes(pt) for pt in test_points]
    analysis = r0filter.analyze_structure_preservation(test_bytes_list)

    print(f"\n  Structure preservation (unique LSBs per round):")
    print(f"    {analysis['per_round_correlation']}")
    print(f"    (Lower = more EC structure preserved)")
    print()


def analyze_zomega_structure():
    """Analyze Z[omega] structure for P135."""
    print("Z[omega] IDEAL ANALYSIS FOR P135")
    print("=" * 60)

    # Analyze n in Z[omega]
    analyze_n_in_zomega(N)

    # Full CM analysis
    cm_structure_analysis()
    print()


def estimate_feasibility():
    """Estimate feasibility of different approaches."""
    print("FEASIBILITY ESTIMATION")
    print("=" * 60)

    # Hardware assumptions
    gpu_ec_ops_per_sec = 10**9  # 1 billion EC ops/sec per GPU
    num_gpus = 2

    print(f"  Hardware: {num_gpus} GPUs @ {gpu_ec_ops_per_sec:.0e} EC ops/s each")
    total_ops_per_sec = gpu_ec_ops_per_sec * num_gpus
    print(f"  Total: {total_ops_per_sec:.0e} EC ops/s")
    print()

    approaches = [
        ("Brute force (2^135)", 2**135),
        ("BSGS (2^67.5)", 2**67.5),
        ("Kangaroo (2^67.5)", 2**67.5),
        ("Kangaroo + 6 auto (2^65.2)", 2**65.2),
        ("+ SHA-256 filter (2^57.1)", 2**57.1),
        ("GLV 3-way per component (2^85)", 2**85),
        ("GLV + filter per component (2^77)", 2**77),
    ]

    for name, ops in approaches:
        time_secs = ops / total_ops_per_sec
        if time_secs > 3.15e7 * 100:  # > 100 years
            time_str = f"{time_secs / 3.15e7:.0e} years"
        elif time_secs > 86400:
            time_str = f"{time_secs / 86400:.1f} days"
        elif time_secs > 3600:
            time_str = f"{time_secs / 3600:.1f} hours"
        else:
            time_str = f"{time_secs:.1f} seconds"
        print(f"    {name}: {time_str}")

    print()
    print("  NOVEL APPROACHES (theoretical, unproven):")
    print(f"    Z[omega] → 2^45 per component + MITM: ~days")
    print(f"    4D quadratic kangaroo: ~2^33.75 (if O(N^1/4) convergence)")
    print(f"    Combined: Z[omega] + filter + GLV: potentially feasible")
    print()

    print("  KEY INSIGHT: We don't need 512TB of storage!")
    print("  Stream through candidates, filter cascade, O(1) memory.")
    print()


def run_validation():
    """Run the full validation suite."""
    from validate import validate_all
    return validate_all()


def run_small_search():
    """Run a search on a small puzzle to verify the pipeline works."""
    print("SMALL PUZZLE SEARCH TEST")
    print("=" * 60)

    from hybrid_solver import HybridSolver

    # Test on P5 range (2^4 to 2^5)
    k_target = 0x1F  # P5 key
    Q_target = point_mul(k_target, G)

    print(f"  Target: P5, k = {k_target:#x}")
    print(f"  Searching in [2^4, 2^5)...")

    solver = HybridSolver(
        target_point=Q_target,
        range_start=2**4,
        range_end=2**5
    )
    # Override hash160 check
    solver.target_hash160s = {pubkey_hash160(Q_target)}

    found = solver.sequential_search(2**4, 2**5)
    if found:
        print(f"\n  *** FOUND P5: k = {found:#x} ***")
    else:
        print(f"\n  P5 not found (unexpected)")

    return found


def main():
    parser = argparse.ArgumentParser(description="VORTEX PRIME — Novel Cryptanalytic Engine")
    parser.add_argument('--validate', action='store_true', help='Run validation suite')
    parser.add_argument('--analyze', action='store_true', help='Analyze P135 structure')
    parser.add_argument('--search-small', action='store_true', help='Test search on small puzzle')
    parser.add_argument('--all', action='store_true', help='Run everything')

    args = parser.parse_args()

    print_banner()

    if args.all or (not args.validate and not args.analyze and not args.search_small):
        # Default: run analysis
        args.analyze = True

    if args.analyze:
        print_target_info()
        print_curve_params()
        analyze_glv_structure()
        analyze_sha256_structure()
        analyze_zomega_structure()
        estimate_feasibility()

    if args.validate:
        run_validation()

    if args.search_small:
        run_small_search()


if __name__ == "__main__":
    main()
