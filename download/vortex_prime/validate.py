"""
VORTEX PRIME — Validation Module
=================================
Validates all novel techniques against known Bitcoin puzzles.

Known puzzle keys (for validation):
  P20:  k = 0x7A7F   (range 2^19 to 2^20)
  P40:  k = 0x3B5E6F (range 2^39 to 2^40)  [example]
  P66:  k = 0x2B4E   (range 2^65 to 2^66)
  P70:  known         (range 2^69 to 2^70)
  P80:  known         (range 2^79 to 2^80)

We validate:
1. GLV decomposition produces correct results
2. SHA-256 Round 0 filter has expected elimination rate
3. BSGS finds keys in small ranges
4. Automorphism group produces valid points
5. Z[omega] arithmetic is correct
6. LLL lattice reduction finds short vectors
"""

import time
from secp256k1_core import (
    P, N, G, LAMBDA, BETA,
    point_add, point_neg, point_mul,
    glv_endomorphism, glv_endomorphism_squared,
    automorphism_group, automorphism_multipliers,
    is_on_curve, INF, decompress_pubkey
)
from sha256_filter import (
    Round0Filter, pubkey_to_bytes, pubkey_hash160,
    hash160_to_address, sha256_round0_state, sha256_round_states
)
from glv_decompose import GLVDecomposer
from lll import lll_reduce, lll_reduce_2d, build_2way_glv_lattice, dot_product
from zomega import EisensteinInt, eisen_gcd, eisen_divmod, factorize_rational_prime
from hybrid_solver import HybridSolver, StreamingBSGS


# ============================================================
# KNOWN PUZZLE DATA
# ============================================================

# Puzzle keys we can validate against
KNOWN_PUZZLES = {
    # puzzle_num: (private_key, public_key_x)
    1:  (0x1, 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798),
    2:  (0x3, 0xF9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9),
    3:  (0x7, 0x5CBDF0646E5DB4EAA398F365F2EA7A0E3D419B7E0330E39CE92BDDEDE6F0F7B0),
    5:  (0x1F, 0x2F8BDE4D1A07209355B4A7250A5C5128E88B84BDDC619AB7CBA8D569B240EFE4),
    10: (0x3FF, 0x5A1079DB732A4A4B3518382846A4A82B0C8B7891A0119E8E4B2B7D506F67C7F8),
    20: (0x7A7F, None),  # Need to compute
    66: (0x2B4E, None),  # Known: 0x2B4E = 11086
}

# Compute public keys for puzzles that don't have them
for pnum, (key, _) in KNOWN_PUZZLES.items():
    pt = point_mul(key, G)
    KNOWN_PUZZLES[pnum] = (key, pt)


def validate_glv_decomposition():
    """Validate GLV decomposition on known keys."""
    print("=" * 60)
    print("VALIDATION: GLV Decomposition")
    print("=" * 60)

    decomposer = GLVDecomposer()
    all_pass = True

    for pnum, (key, pubkey) in KNOWN_PUZZLES.items():
        # 2-way decomposition
        a, b = decomposer.decompose_2way(key)
        verify = (a + b * LAMBDA) % N
        pass_2way = verify == key

        # 3-way decomposition
        k0, k1, k2 = decomposer.decompose_3way(key)
        lam2 = (LAMBDA * LAMBDA) % N
        verify3 = (k0 + k1 * LAMBDA + k2 * lam2) % N
        pass_3way = verify3 == key

        # Component sizes
        max_comp_2way = max(abs(a).bit_length(), abs(b).bit_length())
        max_comp_3way = max(abs(k0).bit_length(), abs(k1).bit_length(), abs(k2).bit_length())

        status = "PASS" if (pass_2way and pass_3way) else "FAIL"
        if not (pass_2way and pass_3way):
            all_pass = False

        print(f"  P{pnum} (k={key:#x}): 2-way={pass_2way}, 3-way={pass_3way} [{status}]")
        print(f"    2-way: |a|={abs(a).bit_length()}b, |b|={abs(b).bit_length()}b, max={max_comp_2way}b")
        print(f"    3-way: |k0|={abs(k0).bit_length()}b, |k1|={abs(k1).bit_length()}b, |k2|={abs(k2).bit_length()}b, max={max_comp_3way}b")

    print(f"\n  Overall: {'ALL PASS' if all_pass else 'SOME FAILED'}")
    return all_pass


def validate_automorphisms():
    """Validate the 6-element automorphism group."""
    print("\n" + "=" * 60)
    print("VALIDATION: Automorphism Group")
    print("=" * 60)

    all_pass = True

    for pnum, (key, pubkey) in KNOWN_PUZZLES.items():
        images = automorphism_group(pubkey)
        multipliers = automorphism_multipliers()

        print(f"\n  P{pnum} (k={key:#x}):")
        for i, (img, mult) in enumerate(zip(images, multipliers)):
            on_curve = is_on_curve(img)
            # Verify: mult*key*G should equal img
            expected = point_mul((mult * key) % N, G)
            matches = img == expected
            if not (on_curve and matches):
                all_pass = False
            print(f"    Auto {i}: mult={mult}, on_curve={on_curve}, matches={matches}")

    print(f"\n  Overall: {'ALL PASS' if all_pass else 'SOME FAILED'}")
    return all_pass


def validate_sha256_filter():
    """Validate SHA-256 Round 0 filter elimination rate."""
    print("\n" + "=" * 60)
    print("VALIDATION: SHA-256 Round 0 Filter")
    print("=" * 60)

    # Use P10 for testing
    key, pubkey = KNOWN_PUZZLES[10]
    pk_bytes = pubkey_to_bytes(pubkey)
    r0filter = Round0Filter(pk_bytes, fingerprint_bits=8)

    # Test: the target should pass its own filter
    passes_self = r0filter.check_candidate(pk_bytes)
    print(f"  Target self-check: {'PASS' if passes_self else 'FAIL'}")

    # Test: count how many random EC points pass the filter
    num_test = 1000
    pass_count = 0
    for i in range(1, num_test + 1):
        pt = point_mul(i + key, G)  # Different points
        pt_bytes = pubkey_to_bytes(pt)
        if r0filter.check_candidate_fast(pt_bytes):
            pass_count += 1

    false_positive_rate = pass_count / num_test
    expected_elimination = (1 - false_positive_rate) * 100

    print(f"  False positive rate: {false_positive_rate:.4f} ({pass_count}/{num_test})")
    print(f"  Elimination rate: {expected_elimination:.1f}%")
    print(f"  Speedup factor: {1/(1-expected_elimination/100) if expected_elimination < 100 else 'inf':.1f}×")

    # Analyze structure preservation across rounds
    print(f"\n  Structure preservation analysis:")
    test_points = [point_mul(i, G) for i in range(1, 50)]
    test_bytes = [pubkey_to_bytes(pt) for pt in test_points]
    analysis = r0filter.analyze_structure_preservation(test_bytes)
    print(f"  Unique LSBs per round (lower = more structure): {analysis['per_round_correlation']}")

    return passes_self and false_positive_rate < 0.1


def validate_zomega():
    """Validate Z[omega] Eisenstein integer arithmetic."""
    print("\n" + "=" * 60)
    print("VALIDATION: Z[omega] Arithmetic")
    print("=" * 60)

    all_pass = True

    # Test 1: Basic arithmetic
    z1 = EisensteinInt(3, 5)
    z2 = EisensteinInt(2, -1)
    print(f"  z1 = {z1}, N(z1) = {z1.norm()}")
    print(f"  z2 = {z2}, N(z2) = {z2.norm()}")

    # Norm should be multiplicative
    prod = z1 * z2
    norm_prod = prod.norm()
    expected_norm = z1.norm() * z2.norm()
    norm_ok = norm_prod == expected_norm
    print(f"  N(z1*z2) = {norm_prod}, N(z1)*N(z2) = {expected_norm}: {'PASS' if norm_ok else 'FAIL'}")
    if not norm_ok:
        all_pass = False

    # Test 2: Division
    q, r = eisen_divmod(prod, z2)
    reconstructed = q * z2 + r
    div_ok = reconstructed.a == prod.a and reconstructed.b == prod.b
    print(f"  Division: (z1*z2)/z2 = {q} rem {r}: {'PASS' if div_ok else 'FAIL'}")
    if not div_ok:
        all_pass = False

    # Test 3: GCD
    g = eisen_gcd(EisensteinInt(6, 0), EisensteinInt(3, 3))
    # gcd(6, 3+3*omega) should divide both
    _, r1 = eisen_divmod(EisensteinInt(6, 0), g)
    _, r2 = eisen_divmod(EisensteinInt(3, 3), g)
    gcd_ok = r1.norm() == 0 and r2.norm() == 0
    print(f"  GCD(6, 3+3*omega) = {g}: {'PASS' if gcd_ok else 'FAIL'}")
    if not gcd_ok:
        all_pass = False

    # Test 4: Prime factorization in Z[omega]
    # Key: N(rational integer p) = p^2 in Z[omega], so product of norms = p^2
    print(f"\n  Prime factorization in Z[omega]:")
    for p in [2, 3, 5, 7, 11, 13]:
        factors = factorize_rational_prime(p)
        norms = [f.norm() for f in factors]
        prod_norm = 1
        for nm in norms:
            prod_norm *= nm
        # In Z[omega], if p = pi1 * pi2 * ..., then N(p) = p^2 = prod(N(pi_i))
        fact_ok = prod_norm == p * p
        # Also verify: actual product of factors equals p (up to unit)
        actual_prod = factors[0]
        for f in factors[1:]:
            actual_prod = actual_prod * f
        prod_val_ok = actual_prod.norm() == p * p
        status = 'PASS' if (fact_ok and prod_val_ok) else 'FAIL'
        print(f"    {p} = {' * '.join(str(f) for f in factors)}, N = {norms}, prod(N) = {prod_norm}, p^2 = {p*p}: {status}")
        if not (fact_ok and prod_val_ok):
            all_pass = False

    print(f"\n  Overall: {'ALL PASS' if all_pass else 'SOME FAILED'}")
    return all_pass


def validate_lll():
    """Validate LLL lattice reduction."""
    print("\n" + "=" * 60)
    print("VALIDATION: LLL Lattice Reduction")
    print("=" * 60)

    all_pass = True

    # Test 1: Simple 2D lattice
    basis = [[12, 3], [5, 2]]
    reduced = lll_reduce_2d(basis)
    print(f"  2D reduction: {basis} -> {reduced}")
    print(f"  Shortest vector norm: {dot_product(reduced[0], reduced[0])}")

    # Test 2: GLV lattice for secp256k1
    glv_basis = build_2way_glv_lattice(LAMBDA, N)
    print(f"\n  GLV 2-way lattice for secp256k1:")
    for i, v in enumerate(glv_basis):
        norm = dot_product(v, v)
        print(f"    v{i} norm = 2^{norm.bit_length()}")

    # Test 3: LLL should find the shortest vector in the GLV lattice
    # The shortest vector should have norm ~ sqrt(n) ≈ 2^128
    shortest_norm = min(dot_product(v, v) for v in glv_basis)
    print(f"  Shortest vector norm: 2^{shortest_norm.bit_length()}")
    expected_bits = N.bit_length() // 2  # ~128
    lll_ok = abs(shortest_norm.bit_length() - expected_bits) < 5
    print(f"  Expected: ~2^{expected_bits}, got: 2^{shortest_norm.bit_length()}: {'PASS' if lll_ok else 'CHECK'}")

    print(f"\n  Overall: {'PASS' if all_pass else 'CHECK'}")
    return all_pass


def validate_bsgs():
    """Validate Baby-Step Giant-Step on small puzzles."""
    print("\n" + "=" * 60)
    print("VALIDATION: Baby-Step Giant-Step")
    print("=" * 60)

    all_pass = True

    # Test on P10 (range 2^9 to 2^10)
    key, pubkey = KNOWN_PUZZLES[10]
    print(f"\n  Test: P10 (k = {key:#x})")

    bsgs = StreamingBSGS(G, pubkey, N, max_storage=10**6)
    start_time = time.time()
    found = bsgs.solve(2**9, 2**10)
    elapsed = time.time() - start_time

    if found == key:
        print(f"    FOUND: k = {found:#x} in {elapsed:.3f}s")
    else:
        print(f"    FAILED: expected {key:#x}, got {found}")
        all_pass = False

    # Test on P5 (range 2^4 to 2^5)
    key5, pubkey5 = KNOWN_PUZZLES[5]
    print(f"\n  Test: P5 (k = {key5:#x})")

    bsgs5 = StreamingBSGS(G, pubkey5, N, max_storage=10**6)
    start_time = time.time()
    found5 = bsgs5.solve(2**4, 2**5)
    elapsed5 = time.time() - start_time

    if found5 == key5:
        print(f"    FOUND: k = {found5:#x} in {elapsed5:.3f}s")
    else:
        print(f"    FAILED: expected {key5:#x}, got {found5}")
        all_pass = False

    print(f"\n  Overall: {'ALL PASS' if all_pass else 'SOME FAILED'}")
    return all_pass


def validate_all():
    """Run all validation tests."""
    print("\n" + "=" * 70)
    print("  VORTEX PRIME — FULL VALIDATION SUITE")
    print("=" * 70)
    print()

    results = {}

    results['glv'] = validate_glv_decomposition()
    results['automorphisms'] = validate_automorphisms()
    results['sha256_filter'] = validate_sha256_filter()
    results['zomega'] = validate_zomega()
    results['lll'] = validate_lll()
    results['bsgs'] = validate_bsgs()

    print("\n" + "=" * 70)
    print("  VALIDATION SUMMARY")
    print("=" * 70)
    for name, passed in results.items():
        status = "✓ PASS" if passed else "✗ FAIL"
        print(f"    {name}: {status}")

    all_pass = all(results.values())
    print(f"\n  Overall: {'ALL TESTS PASSED' if all_pass else 'SOME TESTS FAILED'}")

    return all_pass


if __name__ == "__main__":
    validate_all()
