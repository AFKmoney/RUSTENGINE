#!/usr/bin/env python3
"""
VORTEX PRIME — LBE (Lattice Ball Enumeration) — Correct Implementation
=======================================================================

This script implements the TRUE LBE approach with sphere decoding
(Fincke-Pohst enumeration) in the 6D lattice coefficient space.

CRITICAL MATHEMATICAL ANALYSIS:
-------------------------------
The 6D lattice decomposition gives k = c0*v0[0] + c1*v1[0] + ... + c5*v5[0] (mod n)
where ci ≈ n^(1/6) ≈ 2^43.

For LBE to work as claimed (O(√256)=O(16) steps), we would need to know the
Babai CVP decomposition of the ACTUAL key k. But we don't know k — that's 
what we're trying to find!

The CVP decomposition of the range CENTER gives coefficients that are 
~2^134 away from the true key's coefficients. The "ball" of lattice points
around the range center has radius ~2^134/2 = 2^133, containing 
V6 * R^6 / det(L) = 5.17 * 2^(6*133) / 2^256 = 5.17 * 2^542 lattice points.

The kangaroo in this space needs O(√(5.17 * 2^542)) = O(2^271) steps — 
FAR WORSE than the standard O(2^67) kangaroo.

The ONLY case where LBE works instantly is when we ALREADY KNOW k and
want to verify the decomposition — which is circular.

For P70: Standard kangaroo O(2^34.5) — feasible in hours/days
For P135: Standard kangaroo O(2^67) — infeasible on CPU

However, the 6D lattice IS useful for one thing: it provides an efficient
PARAMETERIZATION of the search space. Combined with GLV endomorphism and
parallel computation, it can speed up the standard kangaroo by constant factors.
"""

import sys
import time
from typing import List, Tuple, Optional

# Try to use gmpy2 for speed, fall back to native Python
try:
    import gmpy2
    from gmpy2 import mpz, isqrt, invert
    HAS_GMPY2 = True
    print("[LBE] Using gmpy2 for fast arithmetic")
except ImportError:
    HAS_GMPY2 = False
    print("[LBE] gmpy2 not available, using native Python (slower)")

# ============================================================
# secp256k1 CONSTANTS
# ============================================================

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
A_CURVE = 0
B_CURVE = 7

# GLV endomorphism
LAMBDA = 0x5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72
BETA = 0x7AE96A2B657C0710B4DCD6D3D794D4AC2C5F1194C1589F2531C4178A46BD2F7B

# Generator
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

# Z[omega] factorization
PI_A = 0x114ca50f7a8e2f3f657c1108d9d44cfd8
PI_B = 0x3086d221a7d46bcde86c90e49284eb15

# ============================================================
# MODULAR ARITHMETIC
# ============================================================

def modinv(a, m):
    """Modular inverse using extended Euclidean or gmpy2"""
    if HAS_GMPY2:
        return int(invert(mpz(a), mpz(m)))
    return pow(a, -1, m)

def mod(a, m):
    """a mod m (always non-negative)"""
    return a % m

# ============================================================
# EC POINT OPERATIONS (Jacobian coordinates for speed)
# ============================================================

class Point:
    """Affine point on secp256k1"""
    __slots__ = ['x', 'y', 'inf']
    
    def __init__(self, x=0, y=0, inf=False):
        self.x = x
        self.y = y
        self.inf = inf
    
    def is_on_curve(self):
        if self.inf:
            return True
        lhs = mod(self.y * self.y, P)
        rhs = mod(pow(self.x, 3, P) + B_CURVE, P)
        return lhs == rhs
    
    def __repr__(self):
        if self.inf:
            return "Point(inf)"
        return f"Point(0x{self.x:064x}, 0x{self.y:064x})"

class JPoint:
    """Jacobian point: (X, Y, Z) where x = X/Z^2, y = Y/Z^3"""
    __slots__ = ['X', 'Y', 'Z']
    
    def __init__(self, X=1, Y=1, Z=0):
        self.X = X
        self.Y = Y
        self.Z = Z
    
    @staticmethod
    def infinity():
        return JPoint(1, 1, 0)
    
    @staticmethod
    def from_affine(p):
        if p.inf:
            return JPoint.infinity()
        return JPoint(p.x, p.y, 1)
    
    def to_affine(self):
        if self.Z == 0:
            return Point(inf=True)
        z_inv = modinv(self.Z, P)
        z_inv2 = mod(z_inv * z_inv, P)
        z_inv3 = mod(z_inv2 * z_inv, P)
        x = mod(self.X * z_inv2, P)
        y = mod(self.Y * z_inv3, P)
        return Point(x, y)
    
    def double(self):
        """Point doubling in Jacobian coordinates (a=0)"""
        if self.Z == 0 or self.Y == 0:
            return JPoint.infinity()
        
        A = mod(self.Y * self.Y, P)
        B = mod(4 * self.X * A, P)
        C = mod(8 * A * A, P)
        D = mod(3 * self.X * self.X, P)  # a=0 for secp256k1
        
        X3 = mod(D * D - 2 * B, P)
        Y3 = mod(D * (B - X3) - C, P)
        Z3 = mod(2 * self.Y * self.Z, P)
        
        return JPoint(X3, Y3, Z3)
    
    def add_mixed(self, p_affine):
        """Mixed addition: Jacobian + Affine"""
        if self.Z == 0:
            return JPoint.from_affine(p_affine)
        if p_affine.inf:
            return JPoint(self.X, self.Y, self.Z)
        
        Z1_sq = mod(self.Z * self.Z, P)
        U2 = mod(p_affine.x * Z1_sq, P)
        Z1_cu = mod(Z1_sq * self.Z, P)
        S2 = mod(p_affine.y * Z1_cu, P)
        
        if self.X == U2:
            if self.Y == S2:
                return self.double()
            return JPoint.infinity()
        
        H = mod(U2 - self.X, P)
        R = mod(S2 - self.Y, P)
        H2 = mod(H * H, P)
        H3 = mod(H2 * H, P)
        
        X3 = mod(R * R - H3 - 2 * self.X * H2, P)
        Y3 = mod(R * (self.X * H2 - X3) - self.Y * H3, P)
        Z3 = mod(H * self.Z, P)
        
        return JPoint(X3, Y3, Z3)
    
    def scalar_mul(self, k):
        """Double-and-add with mixed addition"""
        if k == 0 or self.Z == 0:
            return JPoint.infinity()
        
        # Precompute affine form for mixed addition
        aff = self.to_affine()
        
        result = JPoint.infinity()
        bits = k.bit_length()
        
        for i in range(bits - 1, -1, -1):
            result = result.double()
            if (k >> i) & 1:
                result = result.add_mixed(aff)
        
        return result

# Generator point
G = Point(GX, GY)

def scalar_mul(k):
    """Compute k*G"""
    jg = JPoint.from_affine(G)
    result = jg.scalar_mul(k)
    return result.to_affine()

# ============================================================
# 6D LATTICE CONSTRUCTION + LLL + BABAI CVP
# ============================================================

class Lattice6D:
    def __init__(self, range_start, range_end):
        self.range_start = range_start
        self.range_end = range_end
        self.range_center = (range_start + range_end) >> 1
        self.n = N
        self.lam = LAMBDA
        self.lam_sq = mod(LAMBDA * LAMBDA, N)
        self.pi_a = PI_A
        self.pi_b = PI_B
    
    def build_basis(self):
        """Build 6x6 lattice basis matrix (list of 6 vectors of 6 components)"""
        neg_lam = mod(N - LAMBDA, N)
        neg_lam_sq = mod(N - self.lam_sq, N)
        rc = mod(self.range_center, N)
        pa = mod(self.pi_a, N)
        pb = mod(self.pi_b, N)
        
        # Each row is a 6D vector: [scalar_part, e1, e2, e3, e4, e5]
        basis = [
            [N, 0, 0, 0, 0, 0],           # v0: modular period
            [neg_lam, 1, 0, 0, 0, 0],      # v1: GLV lambda
            [neg_lam_sq, 0, 1, 0, 0, 0],   # v2: lambda^2
            [rc, 0, 0, 1, 0, 0],            # v3: range center
            [pa, 0, 0, 0, 1, 0],            # v4: pi.a
            [pb, 0, 0, 0, 0, 1],            # v5: pi.b
        ]
        return basis
    
    def lll_reduce(self, basis, delta=0.75, max_iter=500):
        """LLL reduction in 6D using exact integer arithmetic"""
        b = [list(v) for v in basis]  # Deep copy
        n = len(b)
        
        def dot(u, v):
            """Dot product of two signed vectors"""
            s = 0
            for i in range(len(u)):
                s += u[i] * v[i]
            return s
        
        def vec_sub(u, v):
            return [u[i] - v[i] for i in range(len(u))]
        
        def vec_add_scaled(u, v, c):
            """u + c*v"""
            return [u[i] + c * v[i] for i in range(len(u))]
        
        iter_count = 0
        k = 1
        while k < n and iter_count < max_iter:
            iter_count += 1
            
            # Size-reduce b[k] with respect to b[0..k-1]
            for j in range(k - 1, -1, -1):
                d_jj = dot(b[j], b[j])
                if d_jj == 0:
                    continue
                d_kj = dot(b[k], b[j])
                # mu = round(d_kj / d_jj)
                mu = (2 * d_kj + d_jj) // (2 * d_jj) if d_jj > 0 else -(2 * (-d_kj) + (-d_jj)) // (2 * (-d_jj))
                # Simple rounding
                if d_jj > 0:
                    q, r = divmod(2 * d_kj, d_jj)
                    if r < 0:
                        q -= 1
                        r += d_jj
                    if 2 * r >= d_jj:
                        mu = q + 1
                    elif 2 * r <= -d_jj:
                        mu = q - 1
                    else:
                        mu = q
                else:
                    mu = 0
                
                if mu != 0:
                    b[k] = vec_add_scaled(b[k], b[j], -mu)
            
            # Lovász condition
            d_kk = dot(b[k], b[k])
            d_k1k1 = dot(b[k-1], b[k-1])
            
            if 4 * d_kk >= 3 * d_k1k1:
                k += 1
            else:
                b[k], b[k-1] = b[k-1], b[k]
                k = max(k - 1, 1)
        
        print(f"  [LLL] Completed in {iter_count} iterations")
        for i, v in enumerate(b):
            bits = [v[j].bit_length() if v[j] != 0 else 0 for j in range(6)]
            norm_sq = sum(x*x for x in v)
            print(f"    v{i}: bits={bits}, |v|^2=2^{norm_sq.bit_length()}")
        
        return b
    
    def babai_cvp(self, basis, target_1d):
        """
        Babai nearest plane CVP for a target k (1D: we only know the scalar part).
        Returns: (coefficients, residual)
        - coefficients: [c0, ..., c5] such that k ≈ Σ ci * vi[0] (mod n)
        - residual: the 6D residual vector
        """
        # Target in 6D: (k, 0, 0, 0, 0, 0)
        target = [target_1d, 0, 0, 0, 0, 0]
        
        # Gram-Schmidt orthogonalization
        n = len(basis)
        b_star = [list(v) for v in basis]
        mu = [[0]*n for _ in range(n)]
        
        for i in range(n):
            for j in range(i):
                # Compute mu[i][j] = <b[i], b*[j]> / <b*[j], b*[j]>
                num = sum(basis[i][d] * b_star[j][d] for d in range(6))
                den = sum(b_star[j][d] * b_star[j][d] for d in range(6))
                if den != 0:
                    # Round to nearest integer
                    mu[i][j] = (2 * num + den) // (2 * den) if den > 0 else 0
                    # Better rounding
                    q, r = divmod(2 * num, den)
                    if r < 0:
                        q -= 1
                        r += den
                    if 2 * r >= den:
                        mu[i][j] = q + 1
                    elif 2 * r <= -den:
                        mu[i][j] = q - 1
                    else:
                        mu[i][j] = q
                
                # Subtract projection
                for d in range(6):
                    b_star[i][d] -= mu[i][j] * b_star[j][d]
        
        # Babai nearest plane: process from i = n-1 down to 0
        t = list(target)
        coefficients = [0] * n
        
        for i in range(n - 1, -1, -1):
            # ci = round(<t, b*[i]> / <b*[i], b*[i]>)
            num = sum(t[d] * b_star[i][d] for d in range(6))
            den = sum(b_star[i][d] * b_star[i][d] for d in range(6))
            
            if den != 0:
                q, r = divmod(2 * num, den)
                if r < 0:
                    q -= 1
                    r += den
                if 2 * r >= den:
                    ci = q + 1
                elif 2 * r <= -den:
                    ci = q - 1
                else:
                    ci = q
            else:
                ci = 0
            
            coefficients[i] = ci
            
            # t = t - ci * basis[i]
            for d in range(6):
                t[d] -= ci * basis[i][d]
        
        return coefficients, t
    
    def reconstruct_scalar(self, basis, coefficients):
        """Reconstruct k = Σ ci * vi[0] (mod n)"""
        k = 0
        for i in range(len(coefficients)):
            k = mod(k + coefficients[i] * basis[i][0], N)
        return k


# ============================================================
# SPHERE ENUMERATION (Fincke-Pohst)
# ============================================================

def sphere_enumerate(basis, target_1d, radius, g_point):
    """
    Enumerate all lattice points within a ball of given radius around
    the Babai CVP solution for target k.
    
    For each candidate k, verify k*G == target.
    
    Returns the key if found, None otherwise.
    """
    # First, get Babai CVP decomposition
    lattice = Lattice6D(0, 0)  # Dummy range
    lattice.n = N
    lattice.lam = LAMBDA
    lattice.lam_sq = mod(LAMBDA * LAMBDA, N)
    lattice.pi_a = PI_A
    lattice.pi_b = PI_B
    
    coefficients, residual = lattice.babai_cvp(basis, target_1d)
    
    print(f"  [SPHERE] Babai CVP coefficients: {coefficients}")
    print(f"  [SPHERE] Residual max bits: {max(abs(r).bit_length() for r in residual)}")
    
    # Now enumerate small offsets around the CVP coefficients
    # The offset in each dimension i is bounded by radius / ||b*_i||
    # For a well-reduced lattice, ||b*_i|| ≈ n^(1/6) ≈ 2^43
    
    # Compute Gram-Schmidt norms
    n = len(basis)
    b_star = [list(v) for v in basis]
    
    for i in range(n):
        for j in range(i):
            num = sum(basis[i][d] * b_star[j][d] for d in range(6))
            den = sum(b_star[j][d] * b_star[j][d] for d in range(6))
            if den != 0:
                q, r = divmod(2 * num, den)
                if r < 0: q -= 1; r += den
                if 2 * r >= den: mu = q + 1
                elif 2 * r <= -den: mu = q - 1
                else: mu = q
                for d in range(6):
                    b_star[i][d] -= mu * b_star[j][d]
    
    gs_norms = [sum(b_star[i][d] * b_star[i][d] for d in range(6)) for i in range(n)]
    gs_norm_bits = [n.bit_length() for n in gs_norms]
    print(f"  [SPHERE] GS norm bits: {gs_norm_bits}")
    
    # For each dimension, compute the range of offsets
    max_offsets = []
    for i in range(n):
        if gs_norms[i] > 0:
            # offset_i ≤ radius * sqrt(gs_norms[i]) / gs_norms[i]
            # ≈ radius / sqrt(gs_norms[i])
            import math
            max_offset = int(radius / math.sqrt(gs_norms[i])) + 1
        else:
            max_offset = 0
        max_offsets.append(max_offset)
        print(f"  [SPHERE] dim {i}: max_offset = 2^{max_offset.bit_length()}, GS norm = 2^{gs_norm_bits[i]}")
    
    # Total search space
    total = 1
    for mo in max_offsets:
        total *= (2 * mo + 1)
    print(f"  [SPHERE] Total candidates: {total} ({total.bit_length()} bits)")
    
    if total > 10_000_000:
        print(f"  [SPHERE] Too many candidates! Need smaller radius or better lattice.")
        return None
    
    # Enumerate all offsets using DFS
    target_point = scalar_mul(target_1d)
    target_x = target_point.x
    checked = 0
    
    def dfs(dim, current_coeffs):
        nonlocal checked
        
        if dim == n:
            # Reconstruct k from coefficients
            k = 0
            for i in range(n):
                k = mod(k + current_coeffs[i] * basis[i][0], N)
            
            # Check if k is in the range
            # For self-test, we know the range
            
            # Verify k*G == target
            checked += 1
            q = scalar_mul(k)
            if not q.inf and q.x == target_x:
                return k
            return None
        
        result = None
        for offset in range(-max_offsets[dim], max_offsets[dim] + 1):
            new_coeffs = list(current_coeffs)
            new_coeffs[dim] = coefficients[dim] + offset
            
            # Pruning: check if partial sum is within bounds
            # (Skip for simplicity in this prototype)
            
            result = dfs(dim + 1, new_coeffs)
            if result is not None:
                break
        
        return result
    
    start = time.time()
    result = dfs(0, list(coefficients))
    elapsed = time.time() - start
    
    print(f"  [SPHERE] Checked {checked} candidates in {elapsed:.2f}s")
    
    return result


# ============================================================
# SELF-TEST: Verify LBE on known keys
# ============================================================

def selftest_lbe(range_bits, key_offset=0x12345):
    """Test LBE with a known key in a given range"""
    print(f"\n{'='*60}")
    print(f"  LBE SELF-TEST: range_bits={range_bits}")
    print(f"{'='*60}")
    
    # Generate known key
    range_start = 1 << (range_bits - 1)
    range_end = 1 << range_bits
    k_known = range_start + key_offset
    
    print(f"  Known key: k = 0x{k_known:x} ({k_known.bit_length()} bits)")
    print(f"  Range: [2^{range_bits-1}, 2^{range_bits})")
    
    # Compute Q = k * G
    print(f"  Computing Q = k*G...")
    t0 = time.time()
    Q = scalar_mul(k_known)
    print(f"  Q on curve: {Q.is_on_curve()} ({time.time()-t0:.2f}s)")
    
    if not Q.is_on_curve():
        print("  ERROR: Q not on curve!")
        return
    
    # Build 6D lattice
    print(f"\n  --- 6D Lattice Construction ---")
    lattice = Lattice6D(range_start, range_end)
    basis = lattice.build_basis()
    
    # LLL reduce
    print(f"  LLL reducing...")
    t0 = time.time()
    reduced = lattice.lll_reduce(basis)
    print(f"  LLL done ({time.time()-t0:.2f}s)")
    
    # Babai CVP for the known key
    print(f"\n  --- Babai CVP for known key ---")
    t0 = time.time()
    coeffs, residual = lattice.babai_cvp(reduced, k_known)
    print(f"  CVP done ({time.time()-t0:.2f}s)")
    print(f"  Coefficients: {coeffs}")
    res_bits = [abs(r).bit_length() for r in residual]
    print(f"  Residual bits: {res_bits}")
    print(f"  Max residual: 2^{max(res_bits)}")
    
    # Verify reconstruction
    k_recon = lattice.reconstruct_scalar(reduced, coeffs)
    print(f"  Reconstruction matches k: {k_recon == mod(k_known, N)}")
    
    # NOW: the key question — can we find k from Q?
    # The Babai CVP on the range CENTER (not k) gives different coefficients
    print(f"\n  --- Babai CVP for range center ---")
    rc = lattice.range_center
    coeffs_rc, residual_rc = lattice.babai_cvp(reduced, rc)
    res_rc_bits = [abs(r).bit_length() for r in residual_rc]
    print(f"  Residual bits: {res_rc_bits}")
    
    # The distance between k and range_center in coefficient space
    print(f"\n  --- Coefficient space analysis ---")
    diff_coeffs = [coeffs[i] - coeffs_rc[i] for i in range(6)]
    diff_bits = [abs(d).bit_length() for d in diff_coeffs]
    print(f"  Coefficient differences (k vs center): bits = {diff_bits}")
    print(f"  Max difference: 2^{max(diff_bits)}")
    
    # The true key's distance from the range center in scalar space
    scalar_dist = abs(k_known - rc)
    print(f"  Scalar distance: 2^{scalar_dist.bit_length()} (k - center)")
    
    # Sphere enumeration around the CVP solution for k
    # If we knew k's CVP coefficients exactly, we could enumerate in O(1)
    # But we DON'T know k, so we can't do this!
    
    print(f"\n  --- TRUTH: Can LBE find k without knowing it? ---")
    print(f"  The CVP ball around the range center has radius ~2^{max(res_rc_bits)}")
    
    # Count lattice points in ball of various radii
    import math
    V6 = math.pi**3 / 6  # Volume of unit ball in 6D
    det_L = N  # Determinant of the lattice ≈ 2^256
    
    for R_bits in [max(res_rc_bits), scalar_dist.bit_length(), range_bits]:
        R = 1 << R_bits
        n_points = V6 * R**6 / det_L
        print(f"  Ball radius 2^{R_bits}: ~{n_points:.1f} lattice points")
    
    print(f"\n  CONCLUSION:")
    print(f"  - LBE sphere enumeration works ONLY if we know the CVP of k")
    print(f"  - Without knowing k, we must search the ENTIRE range")
    print(f"  - Standard kangaroo: O(sqrt(2^{range_bits})) = O(2^{(range_bits+1)//2}) hops")
    print(f"  - LBE does NOT reduce the search complexity for unknown keys")


def kangaroo_solve(target_point, range_bits, max_hops=10_000_000):
    """Standard Pollard kangaroo solver"""
    print(f"\n{'='*60}")
    print(f"  POLLARD KANGAROO: range_bits={range_bits}")
    print(f"{'='*60}")
    
    range_start = 1 << (range_bits - 1)
    range_end = 1 << range_bits
    range_center = (range_start + range_end) >> 1
    
    # Step sizes: mean step ≈ sqrt(range_size) / 4
    half_range = range_bits // 2
    base_step = half_range - 4
    num_steps = 32
    step_sizes = [1 << (base_step + j) for j in range(num_steps)]
    
    print(f"  Step sizes: 2^{base_step} to 2^{base_step + num_steps - 1}")
    print(f"  Mean step: 2^{base_step + num_steps//2}")
    
    # Precompute step points
    print(f"  Precomputing {num_steps} step points...")
    t0 = time.time()
    step_points = []
    for j in range(num_steps):
        s = scalar_mul(step_sizes[j])
        step_points.append(s)
    print(f"  Step points computed ({time.time()-t0:.2f}s)")
    
    # Tame kangaroo: starts at range_center * G
    print(f"  Computing tame start point...")
    tame_pt = JPoint.from_affine(scalar_mul(range_center))
    tame_dist = 0  # Distance from range_center
    
    # Wild kangaroo: starts at target Q
    wild_pt = JPoint.from_affine(target_point)
    wild_dist = 0  # Distance from target
    
    # Distinguished point storage
    dp_mask = (1 << 8) - 1  # 8-bit DP mask
    tame_dps = {}
    wild_dps = {}
    
    def hash_step(pt):
        """Hash Jacobian point to step index"""
        return (pt.X ^ (pt.X >> 64)) % num_steps
    
    def check_dp(pt):
        """Check if point is distinguished"""
        if pt.Z == 0:
            return None
        # Quick check on raw X
        if pt.X & dp_mask != 0:
            return None
        # Normalize and check
        aff = pt.to_affine()
        x_bytes = aff.x
        if x_bytes & dp_mask != 0:
            return None
        return x_bytes
    
    # Warmup
    for _ in range(100):
        si = hash_step(tame_pt)
        tame_pt = tame_pt.add_mixed(step_points[si])
        tame_dist += step_sizes[si]
    
    for _ in range(100):
        si = hash_step(wild_pt)
        wild_pt = wild_pt.add_mixed(step_points[si])
        wild_dist += step_sizes[si]
    
    # Main loop
    print(f"  Starting search (max {max_hops} hops)...")
    t0 = time.time()
    
    for hop in range(max_hops):
        # Tame hop
        si = hash_step(tame_pt)
        tame_pt = tame_pt.add_mixed(step_points[si])
        tame_dist += step_sizes[si]
        
        dp = check_dp(tame_pt)
        if dp is not None:
            if dp in wild_dps:
                # Collision!
                wd = wild_dps[dp]
                k_candidate = mod(range_center + tame_dist - wd, N)
                if range_start <= k_candidate < range_end:
                    q_check = scalar_mul(k_candidate)
                    if not q_check.inf and q_check.x == target_point.x:
                        elapsed = time.time() - t0
                        print(f"\n  *** KEY FOUND! ***")
                        print(f"  k = 0x{k_candidate:064x} ({k_candidate.bit_length()} bits)")
                        print(f"  Hops: {hop+1}, Time: {elapsed:.2f}s")
                        return k_candidate
            tame_dps[dp] = tame_dist
        
        # Wild hop
        si = hash_step(wild_pt)
        wild_pt = wild_pt.add_mixed(step_points[si])
        wild_dist += step_sizes[si]
        
        dp = check_dp(wild_pt)
        if dp is not None:
            if dp in tame_dps:
                td = tame_dps[dp]
                k_candidate = mod(range_center + td - wild_dist, N)
                if range_start <= k_candidate < range_end:
                    q_check = scalar_mul(k_candidate)
                    if not q_check.inf and q_check.x == target_point.x:
                        elapsed = time.time() - t0
                        print(f"\n  *** KEY FOUND! ***")
                        print(f"  k = 0x{k_candidate:064x} ({k_candidate.bit_length()} bits)")
                        print(f"  Hops: {hop+1}, Time: {elapsed:.2f}s")
                        return k_candidate
            wild_dps[dp] = wild_dist
        
        if (hop + 1) % 100000 == 0:
            elapsed = time.time() - t0
            rate = (hop + 1) / elapsed
            print(f"  Hops: {hop+1} | Rate: {rate:.0f}/s | DPs: {len(tame_dps)}+{len(wild_dps)}")
    
    print(f"  Key not found in {max_hops} hops")
    return None


# ============================================================
# MAIN
# ============================================================

if __name__ == "__main__":
    print("╔══════════════════════════════════════════════════════════╗")
    print("║  VORTEX PRIME — LBE Analysis (Correct Implementation)  ║")
    print("╚══════════════════════════════════════════════════════════╝")
    
    # Verify EC arithmetic
    print("\n--- EC Arithmetic Verification ---")
    G2 = scalar_mul(2)
    G7 = scalar_mul(7)
    print(f"  G on curve: {G.is_on_curve()}")
    print(f"  2*G on curve: {G2.is_on_curve()}")
    print(f"  7*G on curve: {G7.is_on_curve()}")
    
    # Test with known P70 key (small)
    k70 = 0x6c3a4f
    Q70 = scalar_mul(k70)
    print(f"  0x6c3a4f*G on curve: {Q70.is_on_curve()}")
    
    # LBE Self-test on small range
    print("\n\n")
    selftest_lbe(40)
    
    print("\n\n")
    selftest_lbe(70)
    
    # Kangaroo test on small range
    print("\n\n")
    print("--- Kangaroo Test on 40-bit range ---")
    k_test = (1 << 39) + 0x12345
    Q_test = scalar_mul(k_test)
    result = kangaroo_solve(Q_test, 40, max_hops=500000)
    
    print("\n\n")
    print("="*60)
    print("  FINAL ANALYSIS FOR P135")
    print("="*60)
    print("""
  P135 Range: [2^134, 2^135)
  
  Standard Pollard Kangaroo:
    - Search space: 2^134
    - Required hops: O(2^67)
    - At 10^6 hops/s (CPU): ~4.7 million years
    - At 10^9 hops/s (GPU): ~4700 years
    - At 10^12 hops/s (GPU cluster): ~4.7 years
    
  LBE (Lattice Ball Enumeration):
    - 6D decomposition gives ci ≈ 2^43 components
    - But we DON'T KNOW k, so we can't do CVP on k
    - CVP on range center gives a point ~2^134 away from k
    - The "256 points in CVP ball" claim only works if we already know k
    - This is CIRCULAR — we can't use LBE to find k
    
  GLV Decomposition:
    - k = k1 + lambda*k2, where |k1|, |k2| ≈ 2^128
    - Kangaroo: O(2^64) — still infeasible
    
  Conclusion: P135 cannot be solved in seconds with any known approach.
  The minimum required is O(2^67) kangaroo hops, which needs massive
  GPU parallelism or a fundamental mathematical breakthrough.
""")
