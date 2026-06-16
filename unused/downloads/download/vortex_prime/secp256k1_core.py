"""
VORTEX PRIME — secp256k1 Core Library
======================================
Pure Python implementation of secp256k1 elliptic curve arithmetic.

Includes:
- Point addition, doubling, scalar multiplication
- GLV endomorphism: phi(x,y) = (beta*x, y) where beta^3 ≡ 1 mod p, lambda^3 ≡ 1 mod n
- Full 6-element automorphism group: {id, -id, phi, -phi, phi^2, -phi^2}
- Decomposition of scalars using the endomorphism ring Z[omega]

Target: Puzzle #135
Address: 16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v
Pubkey:  02145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16
"""

# secp256k1 curve parameters
P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
A = 0
B = 7
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

# GLV endomorphism constants
# beta^3 ≡ 1 mod p (non-trivial cube root of unity mod p)
BETA = 0x7AE96A2B657C07106E64479EAC3434E99CF0497512F58995C1396C28719501EE
# lambda^3 ≡ 1 mod n (non-trivial cube root of unity mod n)
LAMBDA = 0x5363AD4CC05C30E0A5261C028812645A122E22EA20816678DF02967C1B23BD72

# Verify beta^3 ≡ 1 mod p
assert pow(BETA, 3, P) == 1, "beta^3 !== 1 mod p"
# Verify lambda^3 ≡ 1 mod n
assert pow(LAMBDA, 3, N) == 1, "lambda^3 !== 1 mod n"
# Verify beta != 1 (non-trivial)
assert BETA != 1, "beta is trivial"
# Verify lambda != 1 (non-trivial)
assert LAMBDA != 1, "lambda is trivial"

# Point at infinity
INF = (None, None)


def modinv(a, m):
    """Modular inverse using extended Euclidean algorithm."""
    if a < 0:
        a = a % m
    g, x, _ = extended_gcd(a, m)
    if g != 1:
        raise ValueError(f"No inverse for {a} mod {m}")
    return x % m


def extended_gcd(a, b):
    """Extended Euclidean algorithm. Returns (gcd, x, y) such that a*x + b*y = gcd."""
    if a == 0:
        return b, 0, 1
    g, x, y = extended_gcd(b % a, a)
    return g, y - (b // a) * x, x


def point_add(p1, p2):
    """Add two points on secp256k1."""
    x1, y1 = p1
    x2, y2 = p2

    if x1 is None:  # p1 is infinity
        return p2
    if x2 is None:  # p2 is infinity
        return p1

    if x1 == x2:
        if y1 != y2:  # P + (-P) = O
            return INF
        # Point doubling
        if y1 == 0:
            return INF
        s = (3 * x1 * x1 * modinv(2 * y1, P)) % P
    else:
        s = ((y2 - y1) * modinv(x2 - x1, P)) % P

    x3 = (s * s - x1 - x2) % P
    y3 = (s * (x1 - x3) - y1) % P
    return (x3, y3)


def point_neg(p):
    """Negate a point on secp256k1."""
    x, y = p
    if x is None:
        return INF
    return (x, (-y) % P)


def point_double(p):
    """Double a point on secp256k1."""
    return point_add(p, p)


def point_mul(k, p):
    """Scalar multiplication using double-and-add."""
    if k == 0 or p[0] is None:
        return INF
    if k < 0:
        k = k % N
    result = INF
    addend = p
    while k > 0:
        if k & 1:
            result = point_add(result, addend)
        addend = point_add(addend, addend)
        k >>= 1
    return result


def point_mul_windowed(k, p, window=4):
    """Scalar multiplication with windowed method for better performance."""
    if k == 0 or p[0] is None:
        return INF
    if k < 0:
        k = k % N

    # Precompute window table
    table = [INF] * (1 << window)
    table[0] = INF
    table[1] = p
    for i in range(2, 1 << window):
        table[i] = point_add(table[i - 1], p)

    result = INF
    bits = k.bit_length()
    # Process from MSB
    i = bits - 1
    while i >= 0:
        # Find the window
        if i < window - 1:
            w = (k >> 0) & ((1 << (i + 1)) - 1)
            result = point_add(point_mul(1 << (i + 1), result), table[w])
            break

        w = (k >> (i - window + 1)) & ((1 << window) - 1)
        if w == 0:
            result = point_double(result)
            i -= 1
            continue

        result = point_mul(1 << window, result)
        result = point_add(result, table[w])
        i -= window

    return result


# ============================================================
# GLV ENDOMORPHISM
# ============================================================

def glv_endomorphism(p):
    """Apply the GLV endomorphism phi: (x, y) -> (beta*x, y).

    This is the map [lambda] on secp256k1, i.e., phi(P) = [lambda]P.
    It satisfies phi^3 = identity (since lambda^3 ≡ 1 mod n).
    """
    x, y = p
    if x is None:
        return INF
    return ((BETA * x) % P, y)


def glv_endomorphism_squared(p):
    """Apply phi^2: (x, y) -> (beta^2*x, y).

    This corresponds to [lambda^2]P.
    """
    x, y = p
    if x is None:
        return INF
    beta2 = (BETA * BETA) % P
    return ((beta2 * x) % P, y)


# ============================================================
# FULL 6-ELEMENT AUTOMORPHISM GROUP
# ============================================================

def automorphism_group(p):
    """Return all 6 elements of the automorphism group for point p.

    The automorphism group of secp256k1 is generated by:
    - Negation: P -> -P (order 2)
    - Endomorphism phi: P -> phi(P) (order 3)

    This gives 6 automorphisms:
    1. id:       P
    2. -id:      -P
    3. phi:      (beta*x, y)
    4. -phi:     (beta*x, -y)
    5. phi^2:    (beta^2*x, y)
    6. -phi^2:   (beta^2*x, -y)

    Each corresponds to multiplying k by:
    1, -1, lambda, -lambda, lambda^2, -lambda^2  (mod n)
    """
    x, y = p
    if x is None:
        return [INF] * 6

    neg_p = point_neg(p)
    phi_p = glv_endomorphism(p)
    neg_phi_p = point_neg(phi_p)
    phi2_p = glv_endomorphism_squared(p)
    neg_phi2_p = point_neg(phi2_p)

    return [p, neg_p, phi_p, neg_phi_p, phi2_p, neg_phi2_p]


def automorphism_multipliers():
    """Return the 6 scalars corresponding to the automorphism group.

    If Q = kP, then the 6 automorphisms of Q correspond to:
    k, -k, lambda*k, -lambda*k, lambda^2*k, -lambda^2*k  (mod n)
    """
    lam = LAMBDA
    lam2 = (lam * lam) % N
    neg_lam = (N - lam) % N
    neg_lam2 = (N - lam2) % N
    return [1, N - 1, lam, neg_lam, lam2, neg_lam2]


# ============================================================
# POINT VALIDATION
# ============================================================

def is_on_curve(p):
    """Check if a point is on the secp256k1 curve."""
    x, y = p
    if x is None:
        return True
    return (y * y - x * x * x - B) % P == 0


def decompress_pubkey(x, is_even):
    """Decompress a public key from x-coordinate and parity."""
    y_sq = (pow(x, 3, P) + B) % P
    y = pow(y_sq, (P + 1) // 4, P)
    if y % 2 != is_even:
        y = P - y
    return (x, y)


# ============================================================
# PUZZLE #135 TARGET
# ============================================================

P135_PUBKEY_X = 0x145D2611C823A396EF6712CE0F712F09B9B4F3135E3E0AA3230FB9B6D08D1E16
P135_PUBKEY = decompress_pubkey(P135_PUBKEY_X, is_even=True)  # 02 prefix = even
P135_ADDRESS = "16RGFo6hjq9ym6Pj7N5H7L1NR1rVPJyw2v"
P135_RANGE_START = 2**134
P135_RANGE_END = 2**135

# Verify target pubkey is on curve
assert is_on_curve(P135_PUBKEY), "P135 pubkey not on curve!"

# Verify endomorphism works
G = (GX, GY)
phi_G = glv_endomorphism(G)
lambda_G = point_mul(LAMBDA, G)
assert phi_G == lambda_G, "GLV endomorphism verification failed!"

# Verify automorphism group
autos = automorphism_group(G)
for pt in autos:
    assert is_on_curve(pt), f"Automorphism produced invalid point!"

print("[OK] secp256k1 core library loaded and verified")
print(f"  Curve: y^2 = x^3 + 7 over F_p")
print(f"  Generator G validated on curve")
print(f"  GLV endomorphism phi(G) = [lambda]G verified")
print(f"  6-element automorphism group verified")
print(f"  P135 target pubkey on curve: {is_on_curve(P135_PUBKEY)}")
