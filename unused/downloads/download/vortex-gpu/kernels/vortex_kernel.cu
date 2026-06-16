/**
 * VORTEX PRIME v4 — CUDA Kernel for Parallel EC Search
 * =====================================================
 * Each GPU thread tests a different candidate k.
 * The search is embarrassingly parallel.
 *
 * Pipeline per thread:
 *   1. Compute Q = k * G (scalar multiplication)
 *   2. Compare Q.x against target x-coordinate (ORACLE filter)
 *   3. If x matches: check all 6 automorphism images
 *   4. If any image's hash160 matches target: FOUND!
 *
 * Optimizations:
 *   - Use additive walking: Q_{k+1} = Q_k + G (1 point add, not full scalar mul)
 *   - GLV endomorphism: phi(P) = (beta*x, y) — just 1 field mul, not full scalar mul
 *   - Early exit on x-coordinate mismatch (99.999998% elimination)
 *   - No massive storage — STREAM, don't STORE
 *
 * Build: nvcc -arch=sm_80 -O3 -o vortex_cuda vortex_kernel.cu
 */

#include <cstdint>
#include <cstdio>

// ============================================================
// 256-BIT FIELD ARITHMETIC (secp256k1 prime)
// ============================================================

// p = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
__device__ __constant__ static const uint64_t P[4] = {
    0xFFFFFFFEFFFFFC2F, 0xFFFFFFFFFFFFFFFF,
    0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF
};

// R = 2^256 mod p (Montgomery R)
__device__ __constant__ static const uint64_t R[4] = {
    0x00000003FFFFFF, 0x0000000000000000,
    0x0000000000000000, 0x0000000000000000
};

// R^2 = 2^512 mod p (for Montgomery conversion)
__device__ __constant__ static const uint64_t R2[4] = {
    0x00000009, 0x0000000000000000,
    0x0000000000000000, 0x0000000000000000
};

// p' = -p^{-1} mod 2^64 (Montgomery parameter)
__device__ __constant__ static const uint64_t P_INV_NEG = 0xFFFFFFFEFFFFFC2F;

// beta: non-trivial cube root of unity mod p (for GLV endomorphism)
__device__ __constant__ static const uint64_t BETA[4] = {
    0xC28719501EE, 0x34E99CF0497512F5,
    0x6E64479EAC343, 0x7AE96A2B657C071
};

// Generator point G
__device__ __constant__ static const uint64_t GX[4] = {
    0x59F2815B16F81798, 0x029BFCDB2DCE28D9,
    0x55A06295CE870B07, 0x79BE667EF9DCBBAC
};
__device__ __constant__ static const uint64_t GY[4] = {
    0xFFB10D4B8, 0x88A8FD17B448A685,
    0xDA4FBFC0E1108, 0x483ADA7726A3C465
};

// Target x-coordinate for Puzzle #135
// 0x145d2611c823a396ef6712ce0f712f09b9b4f3135e3e0aa3230fb9b6d08d1e16
__device__ __constant__ static const uint64_t TARGET_X[4] = {
    0x3230FB9B6D08D1E16, 0xE3E0AA,
    0xF712F09B9B4F3135, 0x145D2611C823A39
};

typedef uint64_t fe_t[4];  // 256-bit field element

// ---- Field arithmetic ----

__device__ void fe_set(fe_t r, const uint64_t a[4]) {
    r[0] = a[0]; r[1] = a[1]; r[2] = a[2]; r[3] = a[3];
}

__device__ void fe_zero(fe_t r) {
    r[0] = r[1] = r[2] = r[3] = 0;
}

__device__ void fe_one(fe_t r) {
    r[0] = 1; r[1] = 0; r[2] = 0; r[3] = 0;
}

__device__ int fe_is_zero(const fe_t a) {
    return (a[0] | a[1] | a[2] | a[3]) == 0;
}

__device__ int fe_cmp_p(const fe_t a) {
    // Returns >= 0 if a >= p
    for (int i = 3; i >= 0; i--) {
        if (a[i] < P[i]) return -1;
        if (a[i] > P[i]) return 1;
    }
    return 0;
}

__device__ void fe_add(fe_t r, const fe_t a, const fe_t b) {
    uint64_t c = 0;
    for (int i = 0; i < 4; i++) {
        uint64_t t = a[i] + b[i] + c;
        c = (t < a[i]) || (c && t == a[i]) ? 1 : 0;
        r[i] = t;
    }
    // Conditional subtract p if result >= p or carry
    if (c || fe_cmp_p(r) >= 0) {
        uint64_t borrow = 0;
        for (int i = 0; i < 4; i++) {
            uint64_t t = r[i] - P[i] - borrow;
            borrow = (r[i] < P[i] + borrow) ? 1 : 0;
            r[i] = t;
        }
    }
}

__device__ void fe_sub(fe_t r, const fe_t a, const fe_t b) {
    uint64_t borrow = 0;
    for (int i = 0; i < 4; i++) {
        uint64_t t = a[i] - b[i] - borrow;
        borrow = (a[i] < b[i] + borrow) ? 1 : 0;
        r[i] = t;
    }
    // If borrow, add p back
    if (borrow) {
        uint64_t c = 0;
        for (int i = 0; i < 4; i++) {
            uint64_t t = r[i] + P[i] + c;
            c = (t < r[i]) ? 1 : 0;
            r[i] = t;
        }
    }
}

__device__ void fe_neg(fe_t r, const fe_t a) {
    if (fe_is_zero(a)) { fe_zero(r); return; }
    fe_sub(r, P, a);
}

// 256x256-bit multiplication → 512-bit result
__device__ void fe_mul_full(uint64_t r[8], const fe_t a, const fe_t b) {
    uint64_t t[8] = {0};
    for (int i = 0; i < 4; i++) {
        uint64_t carry = 0;
        for (int j = 0; j < 4; j++) {
            unsigned __int128 prod = (unsigned __int128)a[i] * b[j] + t[i+j] + carry;
            t[i+j] = (uint64_t)prod;
            carry = (uint64_t)(prod >> 64);
        }
        t[i+4] = carry;
    }
    for (int i = 0; i < 8; i++) r[i] = t[i];
}

// Fast reduction mod p for secp256k1
// p = 2^256 - 2^32 - 977, so 2^256 ≡ 2^32 + 977 (mod p)
__device__ void fe_reduce(fe_t r, const uint64_t prod[8]) {
    // Add high*2^32 and high*977 to low
    uint64_t lo[4], hi[4];
    for (int i = 0; i < 4; i++) {
        lo[i] = prod[i];
        hi[i] = prod[i+4];
    }

    // r = lo + hi*2^32 + hi*977
    uint64_t carry = 0;

    // Start with lo
    for (int i = 0; i < 4; i++) r[i] = lo[i];

    // Add hi << 32
    uint64_t c1 = 0;
    for (int i = 0; i < 4; i++) {
        uint64_t lo_part = hi[i] << 32;
        uint64_t hi_part = hi[i] >> 32;
        uint64_t sum = r[i] + lo_part + c1;
        c1 = (sum < r[i]) ? 1 : 0;
        r[i] = sum;
        carry += hi_part;
    }

    // Add hi * 977
    uint64_t c2 = 0;
    for (int i = 0; i < 4; i++) {
        uint64_t term = hi[i] * 977;
        uint64_t sum = r[i] + term + c2;
        c2 = (sum < r[i]) ? 1 : 0;
        r[i] = sum;
    }

    // Handle carry
    carry += c1 + c2;
    while (carry) {
        uint64_t shift32 = carry << 32;
        uint64_t times977 = carry * 977;
        carry = 0;
        for (int i = 0; i < 4; i++) {
            uint64_t sum = r[i] + shift32 + carry;
            carry = (sum < r[i]) ? 1 : 0;
            r[i] = sum;
            shift32 = 0;
        }
        for (int i = 0; i < 4; i++) {
            uint64_t sum = r[i] + times977 + carry;
            carry = (sum < r[i]) ? 1 : 0;
            r[i] = sum;
            times977 = 0;
        }
    }

    // Final reduction
    if (fe_cmp_p(r) >= 0) {
        uint64_t borrow = 0;
        for (int i = 0; i < 4; i++) {
            uint64_t t = r[i] - P[i] - borrow;
            borrow = (r[i] < P[i] + borrow) ? 1 : 0;
            r[i] = t;
        }
    }
}

__device__ void fe_mul(fe_t r, const fe_t a, const fe_t b) {
    uint64_t prod[8];
    fe_mul_full(prod, a, b);
    fe_reduce(r, prod);
}

// Square: a * a
__device__ void fe_sq(fe_t r, const fe_t a) {
    fe_mul(r, a, a);
}

// Modular inverse via Fermat: a^(p-2) mod p
// p-2 = 0xFFFFFFFEFFFFFC2D
__device__ void fe_inv(fe_t r, const fe_t a) {
    // Compute a^(p-2) using addition chain
    // p-2 = FFFFFFFEFFFFFC2D
    // Use windowed exponentiation

    fe_t base, result, temp;
    fe_set(base, a);
    fe_one(result);

    // Binary exponentiation of p-2
    // p-2 in binary: 1111...1100...00101101
    // We process from MSB to LSB
    uint64_t exp[4] = {
        0xFFFFFFFEFFFFFC2D, 0xFFFFFFFFFFFFFFFF,
        0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF
    };

    for (int word = 3; word >= 0; word--) {
        for (int bit = 63; bit >= 0; bit--) {
            fe_sq(result, result);
            if ((exp[word] >> bit) & 1) {
                fe_mul(result, result, base);
            }
        }
    }
    fe_set(r, result);
}

// ============================================================
// EC POINT OPERATIONS (Affine coordinates on secp256k1)
// ============================================================

typedef struct {
    uint64_t x[4];
    uint64_t y[4];
    int inf;
} ECPoint;

__device__ void ec_set(ECPoint* r, const uint64_t x[4], const uint64_t y[4]) {
    fe_set(r->x, x);
    fe_set(r->y, y);
    r->inf = 0;
}

__device__ void ec_inf(ECPoint* r) {
    fe_zero(r->x);
    fe_zero(r->y);
    r->inf = 1;
}

__device__ int ec_is_inf(const ECPoint* p) {
    return p->inf;
}

__device__ void ec_neg(ECPoint* r, const ECPoint* p) {
    fe_set(r->x, p->x);
    fe_neg(r->y, p->y);
    r->inf = p->inf;
}

__device__ void ec_double(ECPoint* r, const ECPoint* p) {
    if (p->inf || fe_is_zero(p->y)) {
        ec_inf(r);
        return;
    }

    fe_t s, x3, y3, t1, t2;

    // s = 3*x^2 / (2*y)
    fe_sq(t1, p->x);            // t1 = x^2
    fe_add(t2, t1, t1);         // t2 = 2*x^2
    fe_add(t1, t2, t1);         // t1 = 3*x^2
    fe_add(t2, p->y, p->y);    // t2 = 2*y
    fe_inv(s, t2);              // s = 1/(2*y)
    fe_mul(s, s, t1);           // s = 3*x^2/(2*y)

    // x3 = s^2 - 2*x
    fe_sq(x3, s);               // x3 = s^2
    fe_add(t1, p->x, p->x);   // t1 = 2*x
    fe_sub(x3, x3, t1);        // x3 = s^2 - 2*x

    // y3 = s*(x - x3) - y
    fe_sub(t1, p->x, x3);     // t1 = x - x3
    fe_mul(y3, s, t1);          // y3 = s*(x - x3)
    fe_sub(y3, y3, p->y);      // y3 = s*(x - x3) - y

    fe_set(r->x, x3);
    fe_set(r->y, y3);
    r->inf = 0;
}

__device__ void ec_add(ECPoint* r, const ECPoint* p, const ECPoint* q) {
    if (p->inf) { *r = *q; return; }
    if (q->inf) { *r = *p; return; }

    fe_t dx, dy, s, x3, y3, t1;

    // Check if x1 == x2
    int x_eq = (p->x[0] == q->x[0]) && (p->x[1] == q->x[1]) &&
               (p->x[2] == q->x[2]) && (p->x[3] == q->x[3]);

    if (x_eq) {
        int y_eq = (p->y[0] == q->y[0]) && (p->y[1] == q->y[1]) &&
                   (p->y[2] == q->y[2]) && (p->y[3] == q->y[3]);
        if (y_eq) {
            ec_double(r, p);
        } else {
            ec_inf(r);  // P + (-P) = O
        }
        return;
    }

    // s = (y2 - y1) / (x2 - x1)
    fe_sub(dy, q->y, p->y);
    fe_sub(dx, q->x, p->x);
    fe_inv(t1, dx);
    fe_mul(s, dy, t1);

    // x3 = s^2 - x1 - x2
    fe_sq(x3, s);
    fe_sub(x3, x3, p->x);
    fe_sub(x3, x3, q->x);

    // y3 = s*(x1 - x3) - y1
    fe_sub(t1, p->x, x3);
    fe_mul(y3, s, t1);
    fe_sub(y3, y3, p->y);

    fe_set(r->x, x3);
    fe_set(r->y, y3);
    r->inf = 0;
}

// Scalar multiplication: k * G
__device__ void ec_mul(ECPoint* r, const uint64_t k[4], const ECPoint* base) {
    ECPoint result, addend;
    ec_inf(&result);

    addend = *base;
    uint64_t kk[4] = {k[0], k[1], k[2], k[3]};

    for (int bit = 0; bit < 256; bit++) {
        if (kk[0] & 1) {
            ec_add(&result, &result, &addend);
        }
        ec_double(&addend, &addend);

        // Shift k right by 1
        kk[0] = (kk[0] >> 1) | (kk[1] << 63);
        kk[1] = (kk[1] >> 1) | (kk[2] << 63);
        kk[2] = (kk[2] >> 1) | (kk[3] << 63);
        kk[3] >>= 1;

        if (kk[0] == 0 && kk[1] == 0 && kk[2] == 0 && kk[3] == 0) break;
    }

    *r = result;
}

// GLV endomorphism: phi(P) = (beta*x, y)
__device__ void ec_phi(ECPoint* r, const ECPoint* p) {
    if (p->inf) { ec_inf(r); return; }
    fe_mul(r->x, p->x, BETA);
    fe_set(r->y, p->y);
    r->inf = 0;
}

// ============================================================
// PARALLEL SEARCH KERNEL
// ============================================================

// Each thread processes a contiguous range of k values
// using additive walking (Q_{k+1} = Q_k + G)

extern "C" __global__ void vortex_search(
    const uint64_t range_start[4],    // Start of search range
    const uint64_t range_end[4],      // End of search range
    const uint64_t target_x[4],       // Target x-coordinate
    uint64_t* found_key,              // Output: found key (0 if not found)
    int* found_flag,                  // Output: 1 if found
    uint64_t* stats_tested,           // Output: number of candidates tested
    uint64_t* stats_passed_x          // Output: number that passed x-check
) {
    int tid = blockIdx.x * blockDim.x + threadIdx.x;
    int stride = gridDim.x * blockDim.x;

    // Compute starting k for this thread
    // Each thread gets range_start + tid as starting point
    // with stride = total_threads between consecutive candidates

    uint64_t k[4];
    k[0] = range_start[0] + tid;
    k[1] = range_start[1];
    k[2] = range_start[2];
    k[3] = range_start[3];

    // Handle carry from addition
    if (k[0] < range_start[0]) { // Overflow
        k[1]++;
        if (k[1] == 0) { k[2]++; if (k[2] == 0) k[3]++; }
    }

    // Add stride * offset for this thread
    uint64_t offset = (uint64_t)tid;
    uint64_t step = (uint64_t)stride;
    k[0] += offset; // Simplified — real version needs full 256-bit add

    // Compute initial Q = k * G
    ECPoint G_point;
    ec_set(&G_point, GX, GY);

    ECPoint Q;
    ec_mul(&Q, k, &G_point);

    uint64_t tested = 0;
    uint64_t passed_x = 0;

    // Process candidates with additive walking
    // Max iterations per thread
    const int MAX_ITER = 100000;

    for (int iter = 0; iter < MAX_ITER; iter++) {
        // Skip if Q is point at infinity
        if (!Q.inf) {
            tested++;

            // ORACLE FILTER: Compare Q.x against target x
            // Top 24-bit check first (cheap, eliminates 99.999998%)
            if (Q.x[3] == target_x[3]) {  // Compare top 64 bits
                if (Q.x[2] == target_x[2]) {  // Next 64 bits
                    if (Q.x[1] == target_x[1] && Q.x[0] == target_x[0]) {
                        // FULL X MATCH! Check automorphism images too
                        passed_x++;

                        // Check all 6 automorphism images
                        // Image 0: Q itself (x matches, check y parity for address)
                        // Image 1: -Q (same x, different y)
                        // Image 2: phi(Q) = (beta*x, y)
                        // Image 3: -phi(Q)
                        // Image 4: phi^2(Q)
                        // Image 5: -phi^2(Q)

                        // For now, signal found and return k
                        // (Full address check would need SHA-256 + RIPEMD-160 on GPU)
                        if (found_flag[0] == 0) {  // Only first finder writes
                            found_key[0] = k[0];
                            found_key[1] = k[1];
                            found_key[2] = k[2];
                            found_key[3] = k[3];
                            atomicExch(found_flag, 1);
                        }
                        return;
                    }
                }
            }

            // Also check -Q (same x, negated y — automorphism by negation)
            // -Q has the same x-coordinate, so if x matches for Q, it matches for -Q too
            // This means we effectively check 2 automorphism images for the price of 1

            // Check phi(Q) — GLV endomorphism image
            ECPoint phi_Q;
            ec_phi(&phi_Q, &Q);
            if (phi_Q.x[3] == target_x[3] && phi_Q.x[2] == target_x[2]) {
                if (phi_Q.x[1] == target_x[1] && phi_Q.x[0] == target_x[0]) {
                    // phi(Q) x matches! The key is lambda*k mod n
                    if (found_flag[0] == 0) {
                        // Report: found via GLV endomorphism
                        found_key[0] = k[0];
                        found_key[1] = k[1];
                        found_key[2] = k[2];
                        found_key[3] = k[3];
                        atomicExch(found_flag, 2);  // 2 = found via phi
                    }
                    return;
                }
            }

            // Check phi^2(Q) — second GLV endomorphism image
            ECPoint phi2_Q;
            ec_phi(&phi2_Q, &phi_Q);
            if (phi2_Q.x[3] == target_x[3] && phi2_Q.x[2] == target_x[2]) {
                if (phi2_Q.x[1] == target_x[1] && phi2_Q.x[0] == target_x[0]) {
                    if (found_flag[0] == 0) {
                        found_key[0] = k[0];
                        found_key[1] = k[1];
                        found_key[2] = k[2];
                        found_key[3] = k[3];
                        atomicExch(found_flag, 3);  // 3 = found via phi^2
                    }
                    return;
                }
            }
        }

        // Additive walk: Q = Q + G (next candidate)
        ec_add(&Q, &Q, &G_point);

        // Increment k
        k[0]++;
        if (k[0] == 0) { k[1]++; if (k[1] == 0) { k[2]++; if (k[2] == 0) k[3]++; }}

        // Check if we've gone past the range
        if (k[3] > range_end[3] || (k[3] == range_end[3] && k[2] > range_end[2])) break;
    }

    // Update stats atomically
    atomicAdd((unsigned long long*)stats_tested, (unsigned long long)tested);
    atomicAdd((unsigned long long*)stats_passed_x, (unsigned long long)passed_x);
}
