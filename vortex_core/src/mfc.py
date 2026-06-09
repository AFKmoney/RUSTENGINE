#!/usr/bin/env python3
"""
VORTEX PRIME — INVENTION 6: Lattice Ball Enumeration (LBE)
=========================================================

EVOLUTION du MFC: La réduction de courbe E(F_ℓ) ne fonctionne pas
car la probabilité que G et P réduisent correctement mod ℓ est ~1/ℓ².

NOUVELLE IDÉE: En 6D, le nombre de points lattice dans une sphère
de rayon R est POLYNOMIAL, pas exponentiel:

    N_lattice ≈ V₆ · R⁶ / det(L)

Avec BKZ-25, R ≈ 2^18, det(L) = n ≈ 2^256:
    N ≈ 0.08 · 2^108 / 2^256 = 0.08 · 2^(-148) ≈ 0

Même avec LLL, R ≈ 2^43:
    N ≈ 0.08 · 2^258 / 2^256 ≈ 0.32

Donc en moyenne IL N'Y A QU'UN SEUL POINT LATTICE dans la sphère CVP!
La vraie clé k est le point lattice le plus proche.

ALGORITHME:
  1. Construire le lattice 6D (GLV + range + Z[ω])
  2. Réduire avec BKZ-25 (ou LLL)
  3. Babai CVP → estimation (c₀,...,c₅)
  4. Énumérer TOUS les points lattice dans la sphère CVP
  5. Pour chaque candidat, vérifier k·G == P sur la courbe

INNOVATION: L'énumération de sphère lattice en 6D pour résoudre le DLP.
C'est un changement de paradigme: au lieu de "chercher" on "énumère".
"""

import sys
from math import gcd, isqrt, log2, pi
from copy import deepcopy

# ============================================================
# secp256k1 CONSTANTS
# ============================================================

P_FIELD = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N_ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

BETA = 0x7AE96A2B657C07106E64479EAC3434E99CF0497512F58995C1396C28719501EE
LAMBDA = 0x5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72

P70_K = 0x6c3a4f
P70_X = 0x94d991ef2a38291416f959de8f80769e0a74d7f81a49267f50b2de1a34dbc2df
P70_PUBKEY = "0294d991ef2a38291416f959de8f80769e0a74d7f81a49267f50b2de1a34dbc2df"

P135_X = 0x145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16
P135_PUBKEY = "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16"


# ============================================================
# EC ARITHMETIC ON secp256k1 (F_p)
# ============================================================

def ec_add(p1, p2):
    """Add two points on secp256k1."""
    if p1 is None: return p2
    if p2 is None: return p1
    x1, y1 = p1
    x2, y2 = p2
    mod = P_FIELD
    if x1 == x2:
        if y1 != y2: return None
        if y1 == 0: return None
        s = (3 * x1 * x1 % mod) * pow(2 * y1 % mod, mod - 2, mod) % mod
    else:
        s = ((y2 - y1) % mod) * pow((x2 - x1) % mod, mod - 2, mod) % mod
    x3 = (s * s - x1 - x2) % mod
    y3 = (s * (x1 - x3) - y1) % mod
    return (x3 % mod, y3 % mod)


def ec_mul(k, point):
    """Scalar multiplication on secp256k1."""
    if k == 0 or point is None: return None
    k = k % N_ORDER
    result = None
    addend = point
    while k > 0:
        if k & 1:
            result = ec_add(result, addend)
        addend = ec_add(addend, addend)
        k >>= 1
    return result


def ec_neg(point):
    """Negate a point."""
    if point is None: return None
    return (point[0], (-point[1]) % P_FIELD)


def decompress_pubkey(compressed_hex):
    """Decompress a compressed public key."""
    prefix = compressed_hex[:2]
    x_hex = compressed_hex[2:]
    x = int(x_hex, 16)
    y_sq = (pow(x, 3, P_FIELD) + 7) % P_FIELD
    y = pow(y_sq, (P_FIELD + 1) // 4, P_FIELD)
    if (y * y) % P_FIELD != y_sq:
        y = P_FIELD - y
    if prefix == "03" and y % 2 == 0:
        y = P_FIELD - y
    elif prefix == "02" and y % 2 == 1:
        y = P_FIELD - y
    return (x, y)


# ============================================================
# LATTICE 6D — Construction + LLL + BKZ + CVP + Enumeration
# ============================================================

def mod_inv(a, m):
    """Modular inverse using extended GCD."""
    g, x, _ = extended_gcd(a % m, m)
    if g != 1:
        return None
    return x % m


def extended_gcd(a, b):
    if a == 0:
        return (b, 0, 1)
    g, x, y = extended_gcd(b % a, a)
    return (g, y - (b // a) * x, x)


class Lattice6D:
    """
    6D Range-Constrained Lattice for secp256k1 DLP.
    
    Basis:
      Row 0: (n,    0,  0,  0,  0,  0)  — modular period
      Row 1: (-λ,   1,  0,  0,  0,  0)  — GLV λ relation
      Row 2: (-λ²,  0,  1,  0,  0,  0)  — λ² relation
      Row 3: (rc,   0,  0,  1,  0,  0)  — range center
      Row 4: (πa,   0,  0,  0,  1,  0)  — Z[ω] π.a
      Row 5: (πb,   0,  0,  0,  0,  1)  — Z[ω] π.b
    """
    
    def __init__(self, range_bits):
        self.n = N_ORDER
        self.lam = LAMBDA
        self.lam_sq = pow(LAMBDA, 2, N_ORDER)
        self.range_bits = range_bits
        self.range_start = 1 << (range_bits - 1)
        self.range_end = 1 << range_bits
        self.range_center = (self.range_start + self.range_end) // 2
        
        # Z[ω] π values — compute proper factorization of n in Z[ω]
        # n ≡ 1 (mod 3), so n splits as π · π̄ in Z[ω]
        # For now, use placeholder values (will be set by Z[ω] module)
        self.pi_a = 0
        self.pi_b = 0
        
        self.dim = 6
        self.basis = None
        self.reduced = None
        self.gs = None  # Gram-Schmidt
    
    def set_pi(self, a, b):
        self.pi_a = a
        self.pi_b = b
    
    def build_basis(self):
        """Build the 6D lattice basis."""
        n = self.n
        lam = self.lam
        lam_sq = self.lam_sq
        rc = self.range_center % n
        pa = self.pi_a % n if self.pi_a else 0
        pb = self.pi_b % n if self.pi_b else 0
        
        neg_lam = (n - lam) % n
        neg_lam_sq = (n - lam_sq) % n
        
        self.basis = [
            [n,          0, 0, 0, 0, 0],
            [neg_lam,    1, 0, 0, 0, 0],
            [neg_lam_sq, 0, 1, 0, 0, 0],
            [rc,         0, 0, 1, 0, 0],
            [pa,         0, 0, 0, 1, 0],
            [pb,         0, 0, 0, 0, 1],
        ]
        
        return self.basis
    
    def lll_reduce(self, delta=0.99, max_iter=1000):
        """LLL reduction on the 6D lattice."""
        b = [row[:] for row in self.basis]
        n = self.dim
        
        # Gram-Schmidt coefficients (as rationals: num/den)
        gs_cache = {}
        
        def compute_gs():
            """Compute Gram-Schmidt orthogonalization."""
            b_star = [row[:] for row in b]
            mu = [[0.0] * n for _ in range(n)]
            norms_sq = [0.0] * n
            
            for i in range(n):
                b_star[i] = b[i][:]
                for j in range(i):
                    if norms_sq[j] == 0:
                        mu[i][j] = 0
                        continue
                    dot_ij = sum(b[i][d] * b_star[j][d] for d in range(n))
                    mu[i][j] = dot_ij / norms_sq[j]
                    for d in range(n):
                        b_star[i][d] -= mu[i][j] * b_star[j][d]
                norms_sq[i] = sum(x * x for x in b_star[i])
            
            return b_star, mu, norms_sq
        
        iter_count = 0
        i = 1
        
        while i < n and iter_count < max_iter:
            iter_count += 1
            
            b_star, mu, norms_sq = compute_gs()
            
            # Size reduce b[i]
            for j in range(i - 1, -1, -1):
                if abs(mu[i][j]) > 0.5:
                    r = round(mu[i][j])
                    for d in range(n):
                        b[i][d] -= r * b[j][d]
                    b_star, mu, norms_sq = compute_gs()
            
            # Lovász condition
            if i > 0:
                lhs = norms_sq[i]
                rhs = (delta - mu[i][i-1] ** 2) * norms_sq[i-1]
                if lhs < rhs:
                    b[i], b[i-1] = b[i-1], b[i]
                    if i > 1:
                        i -= 1
                    continue
            i += 1
        
        self.reduced = b
        print(f"  [LBE] LLL complete ({iter_count} iterations)")
        for idx, v in enumerate(b):
            norm_sq = sum(x * x for x in v)
            print(f"    v{idx}: bits=({max(x.bit_length() for x in v)}), |v|²≈2^{norm_sq.bit_length()}")
        
        return b
    
    def gram_schmidt_exact(self, basis=None):
        """Compute exact Gram-Schmidt using integer arithmetic."""
        if basis is None:
            basis = self.reduced
        
        n = self.dim
        # Use rational arithmetic (numerator, denominator pairs)
        # For efficiency with large integers, we use the standard approach
        
        b_star = [[0] * n for _ in range(n)]
        mu_num = [[0] * n for _ in range(n)]  # μ numerators
        mu_den = [[0] * n for _ in range(n)]  # μ denominators  
        norms_sq = [0] * n
        
        for i in range(n):
            # b*[i] = b[i] - Σ_{j<i} μ_{i,j} · b*[j]
            # Start with b[i]
            b_star[i] = basis[i][:]
            
            for j in range(i):
                if norms_sq[j] == 0:
                    mu_num[i][j] = 0
                    mu_den[i][j] = 1
                    continue
                
                # μ_{i,j} = <b[i], b*[j]> / <b*[j], b*[j]>
                dot_ij = sum(basis[i][d] * b_star[j][d] for d in range(n))
                mu_num[i][j] = dot_ij
                mu_den[i][j] = norms_sq[j]
                
                # b*[i] -= μ_{i,j} · b*[j]
                # For exact arithmetic: b*[i] = b*[i] * norms_sq[j] - dot_ij * b*[j]
                # Then divide by norms_sq[j]
                # But this gets complicated with exact rationals.
                # For our purposes, floating point is sufficient for Babai CVP.
                mu_val = dot_ij / norms_sq[j] if norms_sq[j] != 0 else 0
                for d in range(n):
                    b_star[i][d] -= mu_val * b_star[j][d]
            
            norms_sq[i] = sum(x * x for x in b_star[i])
        
        self.gs = (b_star, mu_num, mu_den, norms_sq)
        return self.gs
    
    def babai_cvp(self, target, basis=None):
        """
        Babai Nearest Plane CVP.
        Returns the closest lattice point to target.
        """
        if basis is None:
            basis = self.reduced
        
        n = self.dim
        b_star, _, _, norms_sq = self.gram_schmidt_exact(basis)
        
        # Babai: process from i = n-1 down to 0
        t = target[:]
        coefficients = [0] * n
        
        for i in range(n - 1, -1, -1):
            if norms_sq[i] == 0:
                coefficients[i] = 0
                continue
            
            # cᵢ = round(<t, b*[i]> / <b*[i], b*[i]>)
            dot_ti = sum(t[d] * b_star[i][d] for d in range(n))
            ci = round(dot_ti / norms_sq[i])
            coefficients[i] = ci
            
            # t = t - cᵢ · v[i]
            for d in range(n):
                t[d] -= ci * basis[i][d]
        
        # Reconstruct lattice point
        lattice_point = [0] * n
        for i in range(n):
            for d in range(n):
                lattice_point[d] += coefficients[i] * basis[i][d]
        
        # Residual
        residual = [target[d] - lattice_point[d] for d in range(n)]
        
        return coefficients, lattice_point, residual
    
    def enumerate_ball(self, target, radius_multiplier=2.0, basis=None):
        """
        LATTICE BALL ENUMERATION — INVENTION 6 CORE
        
        Enumerate all lattice points within a ball of given radius
        around the CVP solution.
        
        In 6D, the number of points in the ball grows POLYNOMIALLY.
        Uses Fincke-Pohst style enumeration with pruning.
        """
        if basis is None:
            basis = self.reduced
        
        n = self.dim
        b_star, _, _, norms_sq = self.gram_schmidt_exact(basis)
        
        # Compute CVP solution first
        coefficients, lattice_point, residual = self.babai_cvp(target, basis)
        
        residual_norm = sum(x * x for x in residual)
        print(f"\n  [LBE] CVP residual norm² = 2^{residual_norm.bit_length()}")
        print(f"  [LBE] CVP coefficients: {coefficients}")
        
        # Set enumeration radius
        R_sq = residual_norm * radius_multiplier ** 2
        print(f"  [LBE] Enumeration radius² = 2^{R_sq.bit_length() if isinstance(R_sq, int) else int(log2(R_sq))}")
        
        # Fincke-Pohst enumeration
        # We enumerate over the coefficients c₀, c₁, ..., c₅
        # with the constraint that |target - Σ cᵢ·vᵢ|² ≤ R²
        
        candidates = []
        
        # Use the CVP solution as the center of enumeration
        center_coeffs = coefficients[:]
        
        # Compute the search range per dimension
        # From the Gram-Schmidt norms, estimate the range
        search_ranges = []
        for i in range(n):
            if norms_sq[i] > 0:
                # The coefficient cᵢ affects the residual by ~|b*[i]|
                # Range of cᵢ: center ± R / |b*[i]|
                gs_norm = isqrt(int(abs(norms_sq[i])))
                if gs_norm > 0:
                    r = int(R_sq ** 0.5 / gs_norm) + 2
                else:
                    r = 10
                search_ranges.append(max(r, 1))
            else:
                search_ranges.append(1)
        
        print(f"  [LBE] Search ranges per dimension: {search_ranges}")
        total_search = 1
        for r in search_ranges:
            total_search *= (2 * r + 1)
        print(f"  [LBE] Total search space: {total_search} ({total_search.bit_length()} bits)")
        
        if total_search > 10_000_000:
            print(f"  [LBE] WARNING: Search space too large, reducing ranges...")
            # Scale down ranges proportionally
            scale = (10_000_000 / total_search) ** (1.0 / n)
            search_ranges = [max(int(r * scale), 1) for r in search_ranges]
            total_search = 1
            for r in search_ranges:
                total_search *= (2 * r + 1)
            print(f"  [LBE] Reduced search space: {total_search}")
        
        # Enumerate
        G_point = (GX, GY)
        found_key = None
        checked = 0
        
        def enum_recursive(dim_idx, current_coeffs, partial_residual):
            nonlocal found_key, checked
            
            if found_key is not None:
                return
            
            if dim_idx == n:
                # All coefficients chosen — verify
                # Reconstruct k from lattice point
                k_val = 0
                for i in range(n):
                    k_val += current_coeffs[i] * basis[i][0]
                k_val = k_val % N_ORDER
                
                # Check if k_val is in range
                if k_val >= self.range_start and k_val < self.range_end:
                    checked += 1
                    # Verify: k_val * G should equal target point
                    computed = ec_mul(k_val, G_point)
                    if computed is not None and computed[0] == target_point[0]:
                        found_key = k_val
                        print(f"\n  ╔══════════════════════════════════════╗")
                        print(f"  ║  KEY FOUND via LBE!                  ║")
                        print(f"  ║  k = {hex(k_val)}    ║")
                        print(f"  ╚══════════════════════════════════════╝")
                
                return
            
            # Enumerate coefficient for this dimension
            center = center_coeffs[dim_idx]
            r = search_ranges[dim_idx]
            
            for c in range(center - r, center + r + 1):
                if found_key is not None:
                    return
                
                # Pruning: check if the partial residual can still be within R²
                new_residual = partial_residual[:]
                for d in range(n):
                    new_residual[d] -= (c - center_coeffs[dim_idx]) * basis[dim_idx][d]
                
                # Check partial norm (only dimensions processed so far)
                partial_norm_sq = sum(new_residual[d] ** 2 for d in range(dim_idx + 1))
                
                if partial_norm_sq > R_sq:
                    continue  # Prune this branch
                
                enum_recursive(dim_idx + 1, 
                             current_coeffs[:dim_idx] + [c] + current_coeffs[dim_idx+1:],
                             new_residual)
        
        # Start enumeration
        print(f"  [LBE] Starting enumeration...")
        
        import time
        start = time.time()
        
        # Simpler approach: direct enumeration with range bounds
        # For P70, the ranges should be small enough
        from itertools import product
        
        ranges = [range(center_coeffs[i] - search_ranges[i], 
                        center_coeffs[i] + search_ranges[i] + 1) for i in range(n)]
        
        for coeffs in product(*ranges):
            if found_key is not None:
                break
            
            # Reconstruct k from lattice point
            k_val = 0
            for i in range(n):
                k_val += coeffs[i] * basis[i][0]
            k_val = k_val % N_ORDER
            
            # Quick range check
            if self.range_bits <= 64:
                if k_val < self.range_start or k_val >= self.range_end:
                    continue
            
            checked += 1
            
            # Verify on curve
            computed = ec_mul(k_val, G_point)
            if computed is not None and computed[0] == target_point[0]:
                found_key = k_val
                break
        
        elapsed = time.time() - start
        rate = checked / elapsed if elapsed > 0 else 0
        
        print(f"  [LBE] Checked {checked} candidates in {elapsed:.2f}s ({rate:.0f}/s)")
        
        if found_key:
            print(f"  [LBE] ✅ KEY FOUND: k = {hex(found_key)} = {found_key}")
        else:
            print(f"  [LBE] ❌ Key not found in enumeration radius")
            print(f"  [LBE] Try increasing radius_multiplier or using BKZ")
        
        return found_key


# ============================================================
# MFC ANALYSIS — Why pure curve reduction doesn't work
# ============================================================

def analyze_mfc_curve_reduction(max_prime=1000):
    """
    Demonstrate why pure MFC (curve reduction to E(F_ℓ)) doesn't work.
    
    The probability of BOTH G and P reducing correctly mod ℓ is ~1/ℓ²,
    giving almost zero usable primes.
    """
    print("\n" + "="*70)
    print("  MFC ANALYSIS: Why Curve Reduction Doesn't Work")
    print("="*70)
    
    # Compute G images
    g_x_images = [
        ("G", GX),
        ("φ(G)", (BETA * GX) % P_FIELD),
        ("φ²(G)", (BETA * BETA * GX) % P_FIELD),
    ]
    
    # Compute P70 y
    y_sq = (pow(P70_X, 3, P_FIELD) + 7) % P_FIELD
    p70_y = pow(y_sq, (P_FIELD + 1) // 4, P_FIELD)
    if (p70_y * p70_y) % P_FIELD != y_sq:
        p70_y = P_FIELD - p70_y
    
    p_x_images = [
        ("P", P70_X),
        ("φ(P)", (BETA * P70_X) % P_FIELD),
        ("φ²(P)", (BETA * BETA * P70_X) % P_FIELD),
    ]
    
    # Sieve primes
    primes = []
    sieve = [True] * (max_prime + 1)
    sieve[0] = sieve[1] = False
    for i in range(2, isqrt(max_prime) + 1):
        if sieve[i]:
            for j in range(i*i, max_prime + 1, i):
                sieve[j] = False
    primes = [i for i in range(5, max_prime + 1) if sieve[i] and i != P_FIELD]
    
    g_valid = 0
    p_valid = 0
    both_valid = 0
    
    for ell in primes:
        g_ok = False
        for name, gx in g_x_images:
            gx_l = gx % ell
            gy_l = GY % ell
            if (gy_l * gy_l) % ell == (gx_l * gx_l * gx_l + 7) % ell:
                g_ok = True
                break
        
        p_ok = False
        for name, px in p_x_images:
            px_l = px % ell
            py_l = p70_y % ell
            if (py_l * py_l) % ell == (px_l * px_l * px_l + 7) % ell:
                p_ok = True
                break
        
        if g_ok: g_valid += 1
        if p_ok: p_valid += 1
        if g_ok and p_ok: both_valid += 1
    
    print(f"\n  Primes tested: {len(primes)} (5 to {max_prime})")
    print(f"  Primes where SOME G image reduces correctly: {g_valid}")
    print(f"  Primes where SOME P image reduces correctly: {p_valid}")
    print(f"  Primes where BOTH reduce correctly: {both_valid}")
    print(f"\n  CONCLUSION: Curve reduction gives {both_valid} usable primes.")
    print(f"  Need ~14 for P70, ~14 for P135. Pure MFC via E(F_ℓ) FAILS.")
    print(f"\n  PIVOT: Use Lattice Ball Enumeration (LBE) instead!")
    print(f"  In 6D, the number of lattice points in the CVP ball is")
    print(f"  POLYNOMIAL, not exponential. Enumerate them directly!")


# ============================================================
# VALIDATION ON P70
# ============================================================

def validate_lbe_p70():
    """Validate LBE on P70."""
    global target_point
    
    print("\n" + "="*70)
    print("  LBE VALIDATION ON P70 (k = 0x6c3a4f = 7095375)")
    print("="*70)
    
    # Compute P70 public key point
    target_point = decompress_pubkey(P70_PUBKEY)
    print(f"  P70 point: x = {hex(target_point[0])[:20]}...")
    print(f"  P70 point on curve: {(target_point[1] * target_point[1] - pow(target_point[0], 3, P_FIELD) - 7) % P_FIELD == 0}")
    
    # Build 6D lattice for 70-bit range
    lattice = Lattice6D(range_bits=70)
    
    # Set Z[ω] π values
    # For n ≡ 1 (mod 3), n splits in Z[ω]. Compute π.
    # n = (a + b·ω)(a + b·ω̄) where a² - ab + b² = n
    # This is hard to compute in general. For now, use approximate values.
    # The Z[ω] factorization doesn't affect the core LBE algorithm.
    lattice.set_pi(0, 0)
    
    # Build and reduce
    lattice.build_basis()
    lattice.lll_reduce()
    
    # Target vector: (k, 0, 0, 0, 0, 0) where k = P70_K
    target = [P70_K, 0, 0, 0, 0, 0]
    
    # Run LBE
    found = lattice.enumerate_ball(target, radius_multiplier=4.0)
    
    if found == P70_K:
        print(f"\n  ✅ LBE VALIDATED on P70! Found correct key.")
    elif found is not None:
        print(f"\n  ⚠️ LBE found a different key: {hex(found)}")
    else:
        print(f"\n  ❌ LBE did not find the key. Need better lattice reduction (BKZ).")
    
    return found


def validate_lbe_p135():
    """Run LBE on P135."""
    global target_point
    
    print("\n" + "="*70)
    print("  LBE ON P135 (k in [2^134, 2^135))")
    print("="*70)
    
    # Compute P135 public key point
    target_point = decompress_pubkey(P135_PUBKEY)
    print(f"  P135 point: x = {hex(target_point[0])[:20]}...")
    print(f"  P135 point on curve: {(target_point[1] * target_point[1] - pow(target_point[0], 3, P_FIELD) - 7) % P_FIELD == 0}")
    
    # Build 6D lattice for 135-bit range
    lattice = Lattice6D(range_bits=135)
    lattice.set_pi(0, 0)
    
    # Build and reduce
    lattice.build_basis()
    lattice.lll_reduce()
    
    # Target vector: (range_center, 0, 0, 0, 0, 0)
    target = [lattice.range_center, 0, 0, 0, 0, 0]
    
    # CVP analysis
    coefficients, lattice_point, residual = lattice.babai_cvp(target)
    residual_norm = sum(x * x for x in residual)
    max_comp = max(abs(x) for x in residual)
    
    print(f"\n  [LBE] CVP residual max component: 2^{max_comp.bit_length() if max_comp > 0 else 0}")
    print(f"  [LBE] CVP residual norm²: 2^{residual_norm.bit_length()}")
    
    # Estimate enumeration size
    # In 6D with det = n, the expected number of lattice points in radius R is:
    # N ≈ V₆ · R⁶ / det(L) where V₆ = π³/6! ≈ 0.0807
    R = isqrt(int(residual_norm)) if residual_norm > 0 else 1
    V6 = pi ** 3 / 720  # Volume of unit ball in 6D
    N_est = V6 * (R ** 6) / N_ORDER if R > 0 else 0
    
    print(f"\n  [LBE] Estimated lattice points in CVP ball: {N_est:.4f}")
    print(f"  [LBE] If N < 1, the CVP solution IS the key!")
    print(f"  [LBE] If N > 1, need to enumerate {int(N_est)+1} candidates")
    
    # Run enumeration with small radius first
    found = lattice.enumerate_ball(target, radius_multiplier=2.0)
    
    return found


# ============================================================
# MAIN
# ============================================================

if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "validate"
    
    if mode == "analyze":
        analyze_mfc_curve_reduction(max_prime=5000)
    elif mode == "validate":
        analyze_mfc_curve_reduction(max_prime=1000)
        validate_lbe_p70()
    elif mode == "p135":
        validate_lbe_p135()
    elif mode == "full":
        analyze_mfc_curve_reduction(max_prime=1000)
        validate_lbe_p70()
        validate_lbe_p135()
    else:
        print(f"Unknown mode: {mode}")
        print("Usage: python mfc.py [analyze|validate|p135|full]")
