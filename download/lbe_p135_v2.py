#!/usr/bin/env python3
"""
VORTEX PRIME — LBE P135 Solver v2 (FIXED LLL + Kangaroo)
==========================================================
FIXES from v1:
  1. LLL uses EXACT Gram-Schmidt (Fraction arithmetic) — no rounding errors
  2. Babai CVP uses exact GS — correct reconstruction
  3. Kangaroo uses ONLY properly-sized steps (no 2^256 lattice steps!)
  4. Step sizes = sqrt(range)/4 for optimal collision
"""

import sys
import time
from fractions import Fraction
from typing import List, Tuple, Optional

# ============================================================
# secp256k1 CONSTANTS
# ============================================================

P  = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N  = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

LAMBDA = 0x5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72
BETA   = 0x7AE96A2B657C0710B4DCD6D3D794D4AC2C5F1194C1589F2531C4178A46BD2F7B

PI_A = 0x114ca50f7a8e2f3f657c1108d9d44cfd8
PI_B = 0x3086d221a7d46bcde86c90e49284eb15

P135_PUBKEY = "02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16"
P70_PUBKEY  = "033bb4c229d8050ecab17f8f7f762a5327096ac05c8dfefcaca944460ca04574a54"

# ============================================================
# EC POINT OPERATIONS (proven correct)
# ============================================================

class Point:
    __slots__ = ['x', 'y', 'inf']
    def __init__(self, x=0, y=0, inf=False):
        self.x = x; self.y = y; self.inf = inf
    def is_on_curve(self):
        if self.inf: return True
        return (self.y * self.y - self.x * self.x * self.x - 7) % P == 0
    def neg(self):
        if self.inf: return self
        return Point(self.x, P - self.y, False)

class JPoint:
    __slots__ = ['X', 'Y', 'Z']
    def __init__(self, X=1, Y=1, Z=0):
        self.X = X; self.Y = Y; self.Z = Z
    
    @staticmethod
    def infinity(): return JPoint(1, 1, 0)
    
    @staticmethod
    def from_affine(p):
        return JPoint.infinity() if p.inf else JPoint(p.x, p.y, 1)
    
    def to_affine(self):
        if self.Z == 0: return Point(inf=True)
        z_inv = pow(self.Z, -1, P)
        z_inv2 = z_inv * z_inv % P
        z_inv3 = z_inv2 * z_inv % P
        return Point(self.X * z_inv2 % P, self.Y * z_inv3 % P, False)
    
    def double(self):
        if self.Z == 0 or self.Y == 0: return JPoint.infinity()
        A = self.Y * self.Y % P
        B = 4 * self.X * A % P
        C = 8 * A * A % P
        D = 3 * self.X * self.X % P
        X3 = (D * D - 2 * B) % P
        Y3 = (D * (B - X3) - C) % P
        Z3 = 2 * self.Y * self.Z % P
        return JPoint(X3, Y3, Z3)
    
    def add_mixed(self, p):
        if self.Z == 0: return JPoint.from_affine(p)
        if p.inf: return JPoint(self.X, self.Y, self.Z)
        Z1_sq = self.Z * self.Z % P
        U2 = p.x * Z1_sq % P
        Z1_cu = Z1_sq * self.Z % P
        S2 = p.y * Z1_cu % P
        if self.X == U2:
            return self.double() if self.Y == S2 else JPoint.infinity()
        H = (U2 - self.X) % P
        R = (S2 - self.Y) % P
        H2 = H * H % P
        H3 = H2 * H % P
        X3 = (R * R - H3 - 2 * self.X * H2) % P
        Y3 = (R * (self.X * H2 % P - X3) - self.Y * H3) % P
        Z3 = H * self.Z % P
        return JPoint(X3, Y3, Z3)

G = Point(GX, GY)

def scalar_mul(k):
    """Compute k*G"""
    if k == 0: return Point(inf=True)
    aff = G
    result = JPoint.infinity()
    for i in range(k.bit_length() - 1, -1, -1):
        result = result.double()
        if (k >> i) & 1:
            result = result.add_mixed(aff)
    return result.to_affine()

def decompress_pubkey(pubkey_hex):
    """Decompress compressed public key"""
    prefix = int(pubkey_hex[:2], 16)
    x = int(pubkey_hex[2:], 16)
    y_sq = (pow(x, 3, P) + 7) % P
    y = pow(y_sq, (P + 1) // 4, P)
    if pow(y, 2, P) != y_sq:
        return None
    if prefix == 0x02 and y % 2 != 0: y = P - y
    elif prefix == 0x03 and y % 2 == 0: y = P - y
    pt = Point(x, y)
    return pt if pt.is_on_curve() else None

# ============================================================
# EXACT GRAM-SCHMIDT + LLL (using Fraction for 6D)
# ============================================================

def exact_gram_schmidt(basis):
    """
    Compute exact Gram-Schmidt orthogonalization using Fraction.
    Returns: (b_star, mu, norms_sq) where:
      b_star[i] = list of Fractions (the GS vectors)
      mu[i][j] = Fraction (GS coefficient)
      norms_sq[i] = Fraction (||b*[i]||^2)
    """
    n = len(basis)
    dim = len(basis[0])
    
    b_star = [[Fraction(basis[i][j]) for j in range(dim)] for i in range(n)]
    mu = [[Fraction(0)] * n for _ in range(n)]
    norms_sq = [Fraction(0)] * n
    
    for i in range(n):
        # Start with b[i]
        b_star[i] = [Fraction(basis[i][j]) for j in range(dim)]
        
        # Subtract projections
        for j in range(i):
            if norms_sq[j] == 0:
                mu[i][j] = Fraction(0)
                continue
            
            # mu[i][j] = <b[i], b*[j]> / <b*[j], b*[j]>
            # IMPORTANT: use ORIGINAL basis[i], not current b_star[i]
            dot_num = sum(Fraction(basis[i][d]) * b_star[j][d] for d in range(dim))
            mu[i][j] = dot_num / norms_sq[j]
            
            for d in range(dim):
                b_star[i][d] -= mu[i][j] * b_star[j][d]
        
        # Compute norm squared
        norms_sq[i] = sum(b_star[i][d] * b_star[i][d] for d in range(dim))
    
    return b_star, mu, norms_sq


def lll_reduce_exact(basis, delta=Fraction(3, 4), max_iter=2000):
    """
    LLL reduction using EXACT Gram-Schmidt with Fraction arithmetic.
    No rounding errors!
    """
    n = len(basis)
    dim = len(basis[0])
    b = [list(v) for v in basis]  # Integer vectors
    
    iter_count = 0
    k = 1
    
    while k < n and iter_count < max_iter:
        iter_count += 1
        
        # Recompute exact GS for current basis
        b_star, mu, norms_sq = exact_gram_schmidt(b)
        
        # Size-reduce b[k] with respect to b[0..k-1]
        for j in range(k - 1, -1, -1):
            mu_kj = mu[k][j]
            if abs(mu_kj) > Fraction(1, 2):
                # Round to nearest integer
                r = round(mu_kj)
                for d in range(dim):
                    b[k][d] -= r * b[j][d]
        
        # Recompute GS after size reduction
        b_star, mu, norms_sq = exact_gram_schmidt(b)
        
        # Lovász condition: ||b*[k]||^2 >= (delta - mu[k][k-1]^2) * ||b*[k-1]||^2
        lhs = norms_sq[k]
        rhs = (delta - mu[k][k-1] * mu[k][k-1]) * norms_sq[k-1]
        
        if lhs >= rhs:
            k += 1
        else:
            b[k], b[k-1] = b[k-1], b[k]
            k = max(k - 1, 1)
    
    print(f"  [LLL-EXACT] Completed in {iter_count} iterations")
    for i, v in enumerate(b):
        bits = [v[j].bit_length() if v[j] != 0 else 0 for j in range(dim)]
        norm_sq = sum(x*x for x in v)
        print(f"    v{i}: bits=({bits[0]},{bits[1]},{bits[2]},{bits[3]},{bits[4]},{bits[5]}), |v|²=2^{norm_sq.bit_length()}")
    
    return b


def babai_cvp_exact(basis, target_1d):
    """
    Babai nearest plane CVP using EXACT Gram-Schmidt.
    Returns: (coefficients, residual)
    """
    dim = len(basis[0])
    n = len(basis)
    
    # Compute exact GS
    b_star, mu, norms_sq = exact_gram_schmidt(basis)
    
    # Target in 6D
    t = [Fraction(target_1d)] + [Fraction(0)] * (dim - 1)
    
    coefficients = [0] * n
    
    for i in range(n - 1, -1, -1):
        if norms_sq[i] == 0:
            coefficients[i] = 0
            continue
        
        # ci = round(<t, b*[i]> / <b*[i], b*[i]>)
        dot_val = sum(t[d] * b_star[i][d] for d in range(dim))
        ci_exact = dot_val / norms_sq[i]
        ci = round(ci_exact)
        
        coefficients[i] = ci
        
        # t = t - ci * basis[i]
        for d in range(dim):
            t[d] -= ci * Fraction(basis[i][d])
    
    # Convert residual to integers
    residual = [int(t[d]) for d in range(dim)]
    
    return coefficients, residual


def reconstruct_scalar(basis, coefficients):
    """Reconstruct k = Σ ci * vi[0] (mod N)"""
    k = 0
    for i in range(len(coefficients)):
        k = (k + coefficients[i] * basis[i][0]) % N
    return k


# ============================================================
# 6D LATTICE CONSTRUCTION
# ============================================================

class Lattice6D:
    def __init__(self, range_start, range_end):
        self.range_start = range_start
        self.range_end = range_end
        self.range_center = (range_start + range_end) >> 1
        self.n = N
        self.lam = LAMBDA
        self.lam_sq = pow(LAMBDA, 2, N)
        self.pi_a = PI_A
        self.pi_b = PI_B
    
    def build_basis(self):
        """Build 6x6 lattice basis matrix (all integers, signed allowed)"""
        neg_lam = N - LAMBDA
        neg_lam_sq = N - self.lam_sq
        rc = self.range_center % N
        pa = self.pi_a % N
        pb = self.pi_b % N
        
        # SIGNED basis: use negative values for shorter vectors
        basis = [
            [int(N), 0, 0, 0, 0, 0],         # v0: modular period
            [int(-LAMBDA % N), 1, 0, 0, 0, 0], # v1: GLV lambda (signed!)
            [int(-self.lam_sq % N), 0, 1, 0, 0, 0], # v2: lambda^2 (signed!)
            [int(rc), 0, 0, 1, 0, 0],          # v3: range center
            [int(pa), 0, 0, 0, 1, 0],           # v4: pi.a
            [int(pb), 0, 0, 0, 0, 1],           # v5: pi.b
        ]
        return basis
    
    def build_basis_signed(self):
        """Build SIGNED lattice basis for better LLL reduction.
        Uses signed integers so LLL can find shorter combinations."""
        # We allow NEGATIVE first components for shorter vectors
        # This is key: -lambda is MUCH shorter than N-lambda
        rc = self.range_center % N
        pa = self.pi_a % N
        pb = self.pi_b % N
        
        basis = [
            [int(N), 0, 0, 0, 0, 0],         # v0: modular period
            [int(-LAMBDA), 1, 0, 0, 0, 0],    # v1: -lambda (SIGNED!)
            [int(-self.lam_sq), 0, 1, 0, 0, 0], # v2: -lambda^2 (SIGNED!)
            [int(rc), 0, 0, 1, 0, 0],          # v3: range center
            [int(pa), 0, 0, 0, 1, 0],           # v4: pi.a
            [int(pb), 0, 0, 0, 0, 1],           # v5: pi.b
        ]
        return basis


# ============================================================
# POLLARD KANGAROO (properly sized steps)
# ============================================================

def kangaroo_solve(target_point, range_bits, max_hops=10_000_000, extra_step_points=None):
    """
    Pollard Kangaroo with PROPERLY SIZED step points.
    
    Key: step sizes must be ~sqrt(range_size)/4 for optimal collision.
    NOT 2^256 lattice step points!
    """
    range_start = 1 << (range_bits - 1)
    range_end = 1 << range_bits
    range_center = (range_start + range_end) >> 1
    
    # Step sizes: geometric from 2^(half_range-8) to 2^(half_range+8)
    half_range = range_bits // 2
    base_step = max(1, half_range - 8)
    num_steps = 32
    
    print(f"  [KANG] Range: [2^{range_bits-1}, 2^{range_bits})")
    print(f"  [KANG] Step sizes: 2^{base_step} to 2^{base_step + num_steps - 1}")
    print(f"  [KANG] Mean step: 2^{base_step + num_steps//2} (optimal: 2^{half_range-2})")
    
    # Precompute step points
    print(f"  [KANG] Precomputing {num_steps} step points...")
    t0 = time.time()
    step_points = []
    step_scalars = []
    for j in range(num_steps):
        s = 1 << (base_step + j)
        pt = scalar_mul(s)
        step_points.append(pt)
        step_scalars.append(s)
    print(f"  [KANG] Step points ready ({time.time()-t0:.2f}s)")
    
    # Add extra lattice step points if provided AND properly sized
    if extra_step_points:
        for pt, sc in extra_step_points:
            sc_bits = sc.bit_length()
            # Only use lattice steps if they're in a useful range
            if base_step <= sc_bits <= base_step + num_steps + 8:
                step_points.append(pt)
                step_scalars.append(sc)
                print(f"  [KANG] Added lattice step: 2^{sc_bits} bits")
    
    num_steps_total = len(step_points)
    
    # Tame: starts at range_center * G
    print(f"  [KANG] Computing tame start...")
    tame_pt = JPoint.from_affine(scalar_mul(range_center))
    tame_dist = 0  # Distance from range_center
    
    # Wild: starts at target Q
    wild_pt = JPoint.from_affine(target_point)
    wild_dist = 0  # Distance from Q (= 0)
    
    # DP storage
    dp_mask_bits = max(4, min(10, range_bits // 8))
    dp_mask = (1 << dp_mask_bits) - 1
    tame_dps = {}
    wild_dps = {}
    
    print(f"  [KANG] DP mask: {dp_mask_bits} bits (1/{1<<dp_mask_bits} chance)")
    print(f"  [KANG] Expected hops for collision: O(2^{(range_bits+1)//2})")
    
    def hash_step(pt):
        return pt.X % num_steps_total
    
    def check_dp(pt):
        if pt.Z == 0: return None
        if pt.X & dp_mask != 0: return None
        aff = pt.to_affine()
        if aff.x & dp_mask != 0: return None
        return aff.x
    
    def try_recover(tame_d, wild_d):
        k_candidate = (range_center + tame_d - wild_d) % N
        if range_start <= k_candidate < range_end:
            q_check = scalar_mul(k_candidate)
            if not q_check.inf and q_check.x == target_point.x:
                return k_candidate
        # Check GLV automorphisms
        for lam_pow in [LAMBDA, pow(LAMBDA, 2, N)]:
            k_auto = k_candidate * lam_pow % N
            if range_start <= k_auto < range_end:
                q_check = scalar_mul(k_auto)
                if not q_check.inf and q_check.x == target_point.x:
                    return k_auto
        # Check negation
        k_neg = N - k_candidate
        if range_start <= k_neg < range_end:
            q_check = scalar_mul(k_neg)
            if not q_check.inf and q_check.x == target_point.x:
                return k_neg
        return None
    
    # Warmup
    for _ in range(200):
        si = hash_step(tame_pt)
        tame_pt = tame_pt.add_mixed(step_points[si])
        tame_dist = (tame_dist + step_scalars[si]) % N
    for _ in range(200):
        si = hash_step(wild_pt)
        wild_pt = wild_pt.add_mixed(step_points[si])
        wild_dist = (wild_dist + step_scalars[si]) % N
    
    # Main search
    print(f"  [KANG] Starting search (max {max_hops} hops)...")
    t0 = time.time()
    total_hops = 0
    last_report = 0
    report_interval = max(1000, min(100000, max_hops // 100))
    
    while total_hops < max_hops:
        total_hops += 1
        
        # Tame hop
        si = hash_step(tame_pt)
        tame_pt = tame_pt.add_mixed(step_points[si])
        tame_dist = (tame_dist + step_scalars[si]) % N
        
        dp = check_dp(tame_pt)
        if dp is not None:
            if dp in wild_dps:
                k = try_recover(tame_dist, wild_dps[dp])
                if k is not None:
                    elapsed = time.time() - t0
                    print(f"\n  *** KEY FOUND! ***")
                    print(f"  k = 0x{k:064x} ({k.bit_length()} bits)")
                    print(f"  Hops: {total_hops}, Time: {elapsed:.2f}s, Rate: {total_hops/elapsed:.0f} hops/s")
                    return k
            tame_dps[dp] = tame_dist
        
        # Wild hop
        si = hash_step(wild_pt)
        wild_pt = wild_pt.add_mixed(step_points[si])
        wild_dist = (wild_dist + step_scalars[si]) % N
        
        dp = check_dp(wild_pt)
        if dp is not None:
            if dp in tame_dps:
                k = try_recover(tame_dps[dp], wild_dist)
                if k is not None:
                    elapsed = time.time() - t0
                    print(f"\n  *** KEY FOUND! ***")
                    print(f"  k = 0x{k:064x} ({k.bit_length()} bits)")
                    print(f"  Hops: {total_hops}, Time: {elapsed:.2f}s, Rate: {total_hops/elapsed:.0f} hops/s")
                    return k
            wild_dps[dp] = wild_dist
        
        if total_hops - last_report >= report_interval:
            elapsed = time.time() - t0
            rate = total_hops / elapsed if elapsed > 0 else 0
            print(f"  [KANG] {total_hops} hops | {rate:.0f} hops/s | DPs: {len(tame_dps)}+{len(wild_dps)}")
            last_report = total_hops
    
    elapsed = time.time() - t0
    rate = total_hops / elapsed if elapsed > 0 else 0
    print(f"  [KANG] Not found in {total_hops} hops ({rate:.0f} hops/s)")
    return None


# ============================================================
# MAIN
# ============================================================

if __name__ == "__main__":
    print("╔══════════════════════════════════════════════════════════╗")
    print("║  VORTEX PRIME — LBE P135 Solver v2 (FIXED LLL)         ║")
    print("║  Exact Gram-Schmidt + Proper Kangaroo Steps              ║")
    print("╚══════════════════════════════════════════════════════════╝")
    
    # Verify EC
    print("\n--- EC Verification ---")
    G2 = scalar_mul(2)
    G7 = scalar_mul(7)
    print(f"  G on curve: {G.is_on_curve()}")
    print(f"  2*G on curve: {G2.is_on_curve()}")
    print(f"  7*G on curve: {G7.is_on_curve()}")
    
    # Verify Z[omega]
    lhs = (PI_A * PI_A - PI_A * PI_B + PI_B * PI_B) % N
    print(f"  Z[ω] π·π̄ = n (mod N): {lhs == 0}")
    
    # Test 1: Small range with known key
    print(f"\n{'='*60}")
    print(f"  TEST 1: 40-bit range with known key")
    print(f"{'='*60}")
    
    range_bits = 40
    range_start = 1 << (range_bits - 1)
    range_end = 1 << range_bits
    k_known = range_start + 0xDEADBEEF
    
    print(f"  Known key: k = 0x{k_known:x} ({k_known.bit_length()} bits)")
    Q = scalar_mul(k_known)
    print(f"  Q = k*G on curve: {Q.is_on_curve()}")
    
    # Build lattice
    print(f"\n  --- 6D Lattice (Exact LLL) ---")
    lattice = Lattice6D(range_start, range_end)
    basis = lattice.build_basis_signed()
    
    print(f"  Running EXACT LLL...")
    t0 = time.time()
    reduced = lll_reduce_exact(basis)
    print(f"  LLL done ({time.time()-t0:.2f}s)")
    
    # Babai CVP for known key
    print(f"\n  --- Babai CVP for known key ---")
    t0 = time.time()
    coeffs, residual = babai_cvp_exact(reduced, k_known)
    print(f"  CVP done ({time.time()-t0:.2f}s)")
    k_recon = reconstruct_scalar(reduced, coeffs)
    print(f"  Reconstruction matches k mod N: {k_recon == k_known % N}")
    res_bits = [abs(r).bit_length() for r in residual]
    print(f"  Residual bits: {res_bits}")
    print(f"  Coefficients: {coeffs}")
    
    # Run kangaroo (standard steps only)
    print(f"\n  --- Kangaroo (standard steps) ---")
    result = kangaroo_solve(Q, range_bits, max_hops=5_000_000)
    if result:
        print(f"  SUCCESS! k = 0x{result:x}, match: {result == k_known}")
    
    # Test 2: 50-bit range
    if result:
        print(f"\n{'='*60}")
        print(f"  TEST 2: 50-bit range with known key")
        print(f"{'='*60}")
        
        range_bits = 50
        range_start = 1 << (range_bits - 1)
        range_end = 1 << range_bits
        k_known2 = range_start + 0xDEADBEEFCAFE
        
        print(f"  Known key: k = 0x{k_known2:x} ({k_known2.bit_length()} bits)")
        Q2 = scalar_mul(k_known2)
        print(f"  Q on curve: {Q2.is_on_curve()}")
        
        lattice2 = Lattice6D(range_start, range_end)
        basis2 = lattice2.build_basis_signed()
        reduced2 = lll_reduce_exact(basis2)
        
        coeffs2, res2 = babai_cvp_exact(reduced2, k_known2)
        k_recon2 = reconstruct_scalar(reduced2, coeffs2)
        print(f"  Reconstruction matches: {k_recon2 == k_known2 % N}")
        res_bits2 = [abs(r).bit_length() for r in res2]
        print(f"  Residual bits: {res_bits2}")
        
        result2 = kangaroo_solve(Q2, range_bits, max_hops=10_000_000)
    
    # Test 3: P135 lattice analysis
    print(f"\n{'='*60}")
    print(f"  P135 LATTICE ANALYSIS")
    print(f"{'='*60}")
    
    Q135 = decompress_pubkey(P135_PUBKEY)
    if Q135:
        print(f"  P135 pubkey decompressed: on curve = {Q135.is_on_curve()}")
        
        range_bits = 135
        range_start = 1 << (range_bits - 1)
        range_end = 1 << range_bits
        
        lattice135 = Lattice6D(range_start, range_end)
        basis135 = lattice135.build_basis_signed()
        
        print(f"\n  Running EXACT LLL for P135...")
        t0 = time.time()
        reduced135 = lll_reduce_exact(basis135)
        print(f"  LLL done ({time.time()-t0:.2f}s)")
        
        # CVP for range center
        coeffs135, res135 = babai_cvp_exact(reduced135, lattice135.range_center)
        res_bits135 = [abs(r).bit_length() for r in res135]
        print(f"  Range center residual bits: {res_bits135}")
        
        # Compute lattice step points from reduced basis
        # Only use the SHORT vectors (< 2^80 scalar bits)
        print(f"\n  --- Lattice Step Points ---")
        lattice_steps = []
        for i, v in enumerate(reduced135):
            scalar = v[0] % N
            scalar_bits = scalar.bit_length()
            pt = scalar_mul(scalar)
            print(f"  v{i}[0]*G: scalar=2^{scalar_bits} bits, on curve: {pt.is_on_curve()}")
            # Only add as step point if scalar is in useful range
            if 50 <= scalar_bits <= 80:
                lattice_steps.append((pt, scalar))
                print(f"    -> Added as lattice step point!")
        
        # Run P135 kangaroo
        print(f"\n  --- P135 Kangaroo ---")
        result135 = kangaroo_solve(
            Q135, range_bits,
            max_hops=500_000,
            extra_step_points=lattice_steps if lattice_steps else None
        )
        
        if result135:
            print(f"\n  *** P135 KEY FOUND! k = 0x{result135:064x} ***")
        else:
            print(f"\n  P135 not found in 500K hops (Python speed limit)")
            print(f"  Need: O(2^67) hops at 10^6+/s (Rust/GPU)")
    else:
        print(f"  ERROR: Cannot decompress P135 public key!")
