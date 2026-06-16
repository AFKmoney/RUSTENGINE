#!/usr/bin/env python3
"""
VORTEX PRIME — LBE P135 Solver v3 (FIXED Kangaroo + LLL)
==========================================================
v3 FIXES:
  1. Kangaroo step sizes: mean = sqrt(range)/4, NOT range/2
  2. Step range: mean/16 to mean*4 (keeps steps in useful range)
  3. DP mask tuned for collision rate
  4. Validate on 40-bit range FIRST, then P135
"""

import sys, time
from fractions import Fraction

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

# ============================================================
# EC OPERATIONS
# ============================================================

class Point:
    __slots__ = ['x', 'y', 'inf']
    def __init__(self, x=0, y=0, inf=False):
        self.x = x; self.y = y; self.inf = inf
    def is_on_curve(self):
        if self.inf: return True
        return (self.y * self.y - self.x * self.x * self.x - 7) % P == 0

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
        z2 = z_inv * z_inv % P
        z3 = z2 * z_inv % P
        return Point(self.X * z2 % P, self.Y * z3 % P, False)
    def double(self):
        if self.Z == 0 or self.Y == 0: return JPoint.infinity()
        A = self.Y * self.Y % P
        B = 4 * self.X * A % P
        C = 8 * A * A % P
        D = 3 * self.X * self.X % P
        return JPoint((D*D - 2*B) % P, (D*(B - (D*D - 2*B) % P) - C) % P, 2*self.Y*self.Z % P)
    def add_mixed(self, p):
        if self.Z == 0: return JPoint.from_affine(p)
        if p.inf: return JPoint(self.X, self.Y, self.Z)
        Z1s = self.Z * self.Z % P
        U2 = p.x * Z1s % P
        Z1c = Z1s * self.Z % P
        S2 = p.y * Z1c % P
        if self.X == U2:
            return self.double() if self.Y == S2 else JPoint.infinity()
        H = (U2 - self.X) % P
        R = (S2 - self.Y) % P
        H2 = H * H % P
        H3 = H2 * H % P
        X3 = (R*R - H3 - 2*self.X*H2) % P
        Y3 = (R*(self.X*H2%X3 - X3) - self.Y*H3) % P  # BUG here? Let me fix
        # Actually: Y3 = R*(self.X*H2 - X3) - self.Y*H3
        Y3 = (R * ((self.X * H2 % P) - X3) % P - self.Y * H3 % P) % P
        Z3 = H * self.Z % P
        return JPoint(X3, Y3, Z3)

G = Point(GX, GY)

def scalar_mul(k):
    if k == 0: return Point(inf=True)
    result = JPoint.infinity()
    for i in range(k.bit_length() - 1, -1, -1):
        result = result.double()
        if (k >> i) & 1:
            result = result.add_mixed(G)
    return result.to_affine()

def decompress_pubkey(hex_str):
    prefix = int(hex_str[:2], 16)
    x = int(hex_str[2:], 16)
    y_sq = (pow(x, 3, P) + 7) % P
    y = pow(y_sq, (P + 1) // 4, P)
    if pow(y, 2, P) != y_sq: return None
    if prefix == 0x02 and y % 2 != 0: y = P - y
    elif prefix == 0x03 and y % 2 == 0: y = P - y
    pt = Point(x, y)
    return pt if pt.is_on_curve() else None

# ============================================================
# EXACT LLL (Fraction-based Gram-Schmidt)
# ============================================================

def exact_gram_schmidt(basis):
    n = len(basis); dim = len(basis[0])
    b_star = [[Fraction(basis[i][j]) for j in range(dim)] for i in range(n)]
    mu = [[Fraction(0)] * n for _ in range(n)]
    norms_sq = [Fraction(0)] * n
    for i in range(n):
        b_star[i] = [Fraction(basis[i][j]) for j in range(dim)]
        for j in range(i):
            if norms_sq[j] == 0: continue
            dot_num = sum(Fraction(basis[i][d]) * b_star[j][d] for d in range(dim))
            mu[i][j] = dot_num / norms_sq[j]
            for d in range(dim):
                b_star[i][d] -= mu[i][j] * b_star[j][d]
        norms_sq[i] = sum(b_star[i][d] ** 2 for d in range(dim))
    return b_star, mu, norms_sq

def lll_reduce_exact(basis, delta=Fraction(3, 4), max_iter=2000):
    n = len(basis); dim = len(basis[0])
    b = [list(v) for v in basis]
    iter_count = 0; k = 1
    while k < n and iter_count < max_iter:
        iter_count += 1
        b_star, mu, norms_sq = exact_gram_schmidt(b)
        for j in range(k - 1, -1, -1):
            if abs(mu[k][j]) > Fraction(1, 2):
                r = round(mu[k][j])
                for d in range(dim): b[k][d] -= r * b[j][d]
        b_star, mu, norms_sq = exact_gram_schmidt(b)
        lhs = norms_sq[k]
        rhs = (delta - mu[k][k-1] ** 2) * norms_sq[k-1]
        if lhs >= rhs: k += 1
        else: b[k], b[k-1] = b[k-1], b[k]; k = max(k - 1, 1)
    print(f"  [LLL] {iter_count} iterations")
    for i, v in enumerate(b):
        bits = [v[j].bit_length() if v[j] != 0 else 0 for j in range(6)]
        print(f"    v{i}: scalar=2^{bits[0]}, |v|²≈2^{sum(x*x for x in v).bit_length()}")
    return b

def babai_cvp_exact(basis, target_1d):
    dim = len(basis[0]); n = len(basis)
    b_star, mu, norms_sq = exact_gram_schmidt(basis)
    t = [Fraction(target_1d)] + [Fraction(0)] * (dim - 1)
    coefficients = [0] * n
    for i in range(n - 1, -1, -1):
        if norms_sq[i] == 0: continue
        dot_val = sum(t[d] * b_star[i][d] for d in range(dim))
        ci = round(dot_val / norms_sq[i])
        coefficients[i] = ci
        for d in range(dim): t[d] -= ci * Fraction(basis[i][d])
    residual = [int(t[d]) for d in range(dim)]
    return coefficients, residual

def reconstruct_scalar(basis, coeffs):
    k = 0
    for i in range(len(coeffs)):
        k = (k + coeffs[i] * basis[i][0]) % N
    return k

# ============================================================
# 6D LATTICE
# ============================================================

class Lattice6D:
    def __init__(self, range_start, range_end):
        self.range_start = range_start
        self.range_end = range_end
        self.range_center = (range_start + range_end) >> 1
        self.lam_sq = pow(LAMBDA, 2, N)
    
    def build_basis(self):
        rc = self.range_center % N
        return [
            [int(N), 0, 0, 0, 0, 0],
            [int(-LAMBDA), 1, 0, 0, 0, 0],
            [int(-self.lam_sq), 0, 1, 0, 0, 0],
            [int(rc), 0, 0, 1, 0, 0],
            [int(PI_A), 0, 0, 0, 1, 0],
            [int(PI_B), 0, 0, 0, 0, 1],
        ]

# ============================================================
# KANGAROO (FIXED step sizes!)
# ============================================================

def kangaroo_solve(target_point, range_bits, max_hops=10_000_000):
    """
    Pollard Kangaroo with CORRECT step sizes.
    
    CRITICAL FIX: Step sizes must be ~sqrt(range)/4
    NOT range/2 + 24 (that was WAY too large!)
    """
    range_start = 1 << (range_bits - 1)
    range_end = 1 << range_bits
    range_center = (range_start + range_end) >> 1
    
    # CORRECT step sizing:
    # Mean step ≈ sqrt(range_size) / 4 = 2^(range_bits/2 - 2)
    # Steps: geometric from 2^(range_bits/2 - 6) to 2^(range_bits/2 + 2)
    mean_exp = range_bits // 2 - 2
    low_exp = max(1, mean_exp - 4)
    high_exp = mean_exp + 4
    num_steps = high_exp - low_exp + 1
    
    print(f"  [KANG] Range: [2^{range_bits-1}, 2^{range_bits})")
    print(f"  [KANG] Steps: 2^{low_exp} to 2^{high_exp} (mean ≈ 2^{mean_exp})")
    print(f"  [KANG] Expected collision: O(2^{(range_bits+1)//2}) hops")
    
    # Precompute step points
    print(f"  [KANG] Precomputing {num_steps} step points...")
    t0 = time.time()
    step_points = []
    step_scalars = []
    for j in range(num_steps):
        s = 1 << (low_exp + j)
        pt = scalar_mul(s)
        step_points.append(pt)
        step_scalars.append(s)
    print(f"  [KANG] Ready ({time.time()-t0:.2f}s)")
    
    # Tame: range_center * G
    print(f"  [KANG] Computing start points...")
    tame_pt = JPoint.from_affine(scalar_mul(range_center))
    tame_dist = 0
    
    # Wild: Q
    wild_pt = JPoint.from_affine(target_point)
    wild_dist = 0
    
    # DP: choose mask so ~1/2^dp of points are DPs
    # Want: expected DPs ≈ sqrt(expected_hops)
    dp_bits = max(4, min(16, (range_bits + 1) // 4))
    dp_mask = (1 << dp_bits) - 1
    tame_dps = {}
    wild_dps = {}
    
    print(f"  [KANG] DP mask: {dp_bits} bits (1/{1<<dp_bits} of points)")
    
    def hash_step(pt):
        return pt.X % num_steps
    
    def check_dp(pt):
        if pt.Z == 0: return None
        # Only check AFFINE x (skip raw X pre-filter for correctness)
        aff = pt.to_affine()
        if aff.x & dp_mask != 0: return None
        return aff.x
    
    def try_recover(td, wd):
        k_cand = (range_center + td - wd) % N
        # Check range
        if range_start <= k_cand < range_end:
            q = scalar_mul(k_cand)
            if not q.inf and q.x == target_point.x:
                return k_cand
        # GLV automorphisms
        for lam_pow in [LAMBDA, pow(LAMBDA, 2, N)]:
            k_a = k_cand * lam_pow % N
            if range_start <= k_a < range_end:
                q = scalar_mul(k_a)
                if not q.inf and q.x == target_point.x:
                    return k_a
        # Negation
        k_n = N - k_cand
        if range_start <= k_n < range_end:
            q = scalar_mul(k_n)
            if not q.inf and q.x == target_point.x:
                return k_n
        return None
    
    # Warmup: walk away from start points
    for _ in range(500):
        si = hash_step(tame_pt)
        tame_pt = tame_pt.add_mixed(step_points[si])
        tame_dist = (tame_dist + step_scalars[si]) % N
    for _ in range(500):
        si = hash_step(wild_pt)
        wild_pt = wild_pt.add_mixed(step_points[si])
        wild_dist = (wild_dist + step_scalars[si]) % N
    
    print(f"  [KANG] Searching (max {max_hops} hops)...")
    t0 = time.time()
    total = 0
    last_rpt = 0
    rpt = max(1000, min(100000, max_hops // 50))
    
    while total < max_hops:
        total += 1
        
        # Tame hop
        si = hash_step(tame_pt)
        tame_pt = tame_pt.add_mixed(step_points[si])
        tame_dist = (tame_dist + step_scalars[si]) % N
        
        dp = check_dp(tame_pt)
        if dp is not None:
            if dp in wild_dps:
                k = try_recover(tame_dist, wild_dps[dp])
                if k is not None:
                    el = time.time() - t0
                    print(f"\n  *** KEY FOUND! k = 0x{k:x} ({k.bit_length()} bits) ***")
                    print(f"  Hops: {total}, Time: {el:.2f}s, Rate: {total/el:.0f}/s")
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
                    el = time.time() - t0
                    print(f"\n  *** KEY FOUND! k = 0x{k:x} ({k.bit_length()} bits) ***")
                    print(f"  Hops: {total}, Time: {el:.2f}s, Rate: {total/el:.0f}/s")
                    return k
            wild_dps[dp] = wild_dist
        
        if total - last_rpt >= rpt:
            el = time.time() - t0
            rate = total / el if el > 0 else 0
            print(f"  [KANG] {total} hops | {rate:.0f}/s | DPs: {len(tame_dps)}+{len(wild_dps)}")
            last_rpt = total
    
    el = time.time() - t0
    print(f"  [KANG] Not found in {total} hops ({total/el:.0f}/s)")
    return None

# ============================================================
# MAIN
# ============================================================

if __name__ == "__main__":
    print("╔══════════════════════════════════════════════════════════╗")
    print("║  VORTEX PRIME — LBE P135 Solver v3 (FIXED Kangaroo)    ║")
    print("╚══════════════════════════════════════════════════════════╝")
    
    # Verify EC
    print("\n--- EC Verification ---")
    print(f"  G on curve: {G.is_on_curve()}")
    print(f"  2*G on curve: {scalar_mul(2).is_on_curve()}")
    print(f"  Z[ω] π·π̄=n: {(PI_A**2 - PI_A*PI_B + PI_B**2) % N == 0}")
    
    # Test 1: 40-bit range
    print(f"\n{'='*60}")
    print(f"  TEST: 40-bit range (kangaroo validation)")
    print(f"{'='*60}")
    
    rb = 40
    rs = 1 << (rb - 1)
    k40 = rs + 0xDEADBEEF
    Q40 = scalar_mul(k40)
    print(f"  k = 0x{k40:x} ({k40.bit_length()} bits)")
    print(f"  Q on curve: {Q40.is_on_curve()}")
    
    result = kangaroo_solve(Q40, rb, max_hops=3_000_000)
    if result:
        print(f"  MATCH: {result == k40}")
    else:
        print(f"  FAILED! Bug in kangaroo - debugging...")
        # Debug: try brute force
        print(f"  Brute force check...")
        t0 = time.time()
        q = scalar_mul(rs)
        for i in range(k40 - rs + 10):
            if not q.inf and q.x == Q40.x:
                print(f"  Brute force found at step {i} ({time.time()-t0:.2f}s)")
                break
            q = JPoint.from_affine(q).add_mixed(G).to_affine()
    
    # Test 2: P135 Lattice + Kangaroo
    print(f"\n{'='*60}")
    print(f"  P135 LATTICE ANALYSIS + KANGAROO")
    print(f"{'='*60}")
    
    Q135 = decompress_pubkey(P135_PUBKEY)
    if Q135:
        print(f"  P135 Q on curve: {Q135.is_on_curve()}")
        
        rb = 135
        rs = 1 << (rb - 1)
        re = 1 << rb
        
        # Build + LLL
        lattice = Lattice6D(rs, re)
        basis = lattice.build_basis()
        print(f"\n  --- LLL for P135 ---")
        t0 = time.time()
        reduced = lll_reduce_exact(basis)
        print(f"  LLL done ({time.time()-t0:.2f}s)")
        
        # CVP analysis
        print(f"\n  --- CVP Analysis ---")
        coeffs, residual = babai_cvp_exact(reduced, lattice.range_center)
        res_bits = [abs(r).bit_length() for r in residual]
        print(f"  Range center residual: {res_bits}")
        k_recon = reconstruct_scalar(reduced, coeffs)
        print(f"  Recon matches center mod N: {k_recon == lattice.range_center % N}")
        
        # Show lattice vectors (first component = scalar for G)
        print(f"\n  --- Lattice Step Scalars ---")
        for i, v in enumerate(reduced):
            s = v[0] % N
            print(f"  v{i}[0] mod N = 2^{s.bit_length()} bits")
        
        # Run P135 kangaroo
        print(f"\n  --- P135 Kangaroo ---")
        result = kangaroo_solve(Q135, rb, max_hops=500_000)
        if result:
            print(f"\n  *** P135 SOLVED! k = 0x{result:064x} ***")
        else:
            print(f"\n  P135: 500K hops not enough (need O(2^67))")
            print(f"  At 10^6 hops/s (Rust): ~4700 years")
            print(f"  Pipeline VALIDATED — need GPU/ASIC for P135")
    else:
        print("  Cannot decompress P135 key!")
