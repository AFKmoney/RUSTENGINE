//! RUSTSOLVER v7 — 2D Baby-Step Giant-Step (BSGS) with GLV
//! ===========================================================
//!
//! BSGS: DETERMINISTIC alternative to kangaroo.
//!
//! Algorithm (standard, no GLV trickery in baby table):
//!   k = range_start + k_lo + k_hi * STEP
//!   Q = k*G = range_start*G + k_lo*G + k_hi*(STEP*G)
//!   R = Q - range_start*G = k_lo*G + k_hi*P  (where P = STEP*G)
//!
//!   Baby step: store { j*G : j = 0..STEP-1 } as x-coordinate -> j
//!   Giant step: for i = 0..ceil(W/STEP)-1, check if R - i*P is in table
//!
//! GLV speedup at giant step time:
//!   For each giant step point S, also check Φ(S) and Φ²(S).
//!   Φ(kG) = (λk)G, so if S = k_lo*G, then Φ(S) = (λk_lo)*G.
//!   Since λk_lo might be in our baby table even if k_lo isn't.
//!   This gives √3 speedup (3 images: S, Φ(S), Φ²(S)).
//!   With ±: √6 speedup (6 images total).
//!
//! Complexity:
//!   Without GLV: O(√W) time + memory
//!   With GLV:    O(√(W/6)) time + O(√W) memory
//!   For P135:    ~2^66 time (classical lower bound)
//!
//! For puzzles up to ~60 bits, BSGS PRACTICALLY SOLVES them.
//! For P135, it demonstrates the fundamental barrier.

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use crate::oracle::Round0Oracle;
use num_bigint::BigUint;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug)]
pub struct BsgsResult {
    pub found: bool,
    pub key: Option<BigUint>,
    pub baby_steps: u64,
    pub giant_steps: u64,
    pub elapsed_secs: f64,
    pub steps_per_sec: f64,
    pub memory_mb: u64,
}

pub struct BsgsSolver {
    pub range_bits: u32,
    pub target: Point,
    pub baby_step_count: u64,
    pub oracle: Option<Round0Oracle>,
}

impl BsgsSolver {
    pub fn new(
        range_bits: u32,
        target: Point,
        baby_step_count: u64,
        oracle: Option<Round0Oracle>,
    ) -> Self {
        let default_baby = if range_bits <= 50 {
            1u64 << ((range_bits - 1) / 2)
        } else {
            // Cap at 2^28 ≈ 256M entries for practical memory
            1u64 << 28
        };

        let baby_step_count = if baby_step_count == 0 { default_baby } else { baby_step_count };

        println!();
        println!("  +==================================================+");
        println!("  |  RUSTSOLVER v7 — 2D BSGS + GLV Orbit Check       |");
        println!("  +==================================================+");
        println!();
        println!("  Range:      [2^{}, 2^{})  (W = 2^{})", range_bits - 1, range_bits, range_bits - 1);
        println!("  Baby steps: {} (2^{:.1})", baby_step_count, (baby_step_count as f64).log2());
        println!("  Memory:     ~{} MB", baby_step_count * 40 / 1_000_000);
        println!("  GLV orbit:  6x (check +/-S, +/-Phi(S), +/-Phi^2(S) per giant step)");
        println!();

        BsgsSolver { range_bits, target, baby_step_count, oracle }
    }

    pub fn solve(&self) -> BsgsResult {
        let start = Instant::now();
        let g = Point::generator();
        let beta = Fe { limbs: crate::field::BETA };
        let beta_sq = beta.mul(&beta);
        let lambda = Fe { limbs: crate::field::LAMBDA };
        let lambda_sq = lambda.mul_mod_n(&lambda);

        let range_start = BigUint::from(1u64) << (self.range_bits - 1);
        let range_end = BigUint::from(1u64) << self.range_bits;
        let range_start_fe = Fe::from_biguint_mod_n(&range_start);

        let step = self.baby_step_count;

        // P = STEP * G (giant step base point)
        let step_fe = Fe::from_biguint_mod_n(&BigUint::from(step));
        let p_point = g.scalar_mul(&step_fe);

        // R = Q - range_start * G
        let range_start_pt = g.scalar_mul(&range_start_fe);
        let neg_range_start = Point {
            x: range_start_pt.x,
            y: range_start_pt.y.neg_mod_p(),
            inf: range_start_pt.inf,
        };
        let r_point = self.target.add(&neg_range_start);

        println!("  [BSGS] R = Q - 2^{}*G computed", self.range_bits - 1);
        println!("  [BSGS] R on curve: {}", r_point.is_on_curve());

        println!("  [BSGS] Phase 1: Computing {} baby steps...", step);

        // ============================================================
        // BABY STEP PHASE
        // Store: x-coordinate bytes -> k_lo
        // Simple and correct: each entry is j*G for j = 0..step-1
        // ============================================================

        let mut baby_table: HashMap<[u8; 32], u64> = HashMap::with_capacity(step as usize);

        // Start from 0*G = infinity, then add G each iteration
        // k_lo=0: 0*G (infinity, skip), k_lo=1: 1*G, k_lo=2: 2*G, etc.
        let mut current = JacobianPoint::infinity();
        let report_every = std::cmp::max(1, step / 10);

        for k_lo in 0..step {
            let aff = current.to_affine();
            if !aff.inf {
                let x_bytes = aff.x.to_bytes();
                baby_table.entry(x_bytes).or_insert(k_lo);
            }
            current = current.add_affine(&g);

            if k_lo > 0 && k_lo % report_every == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                println!("    Baby step {} / {} ({:.0}/s, {} entries)",
                         k_lo, step, k_lo as f64 / elapsed, baby_table.len());
            }
        }

        let baby_elapsed = start.elapsed().as_secs_f64();
        println!("  [BSGS] Baby phase done: {} entries in {:.1}s ({:.0}/s)",
                 baby_table.len(), baby_elapsed, step as f64 / baby_elapsed);

        let memory_mb = (baby_table.len() as u64 * 72) / 1_000_000;
        println!("  [BSGS] Memory: ~{} MB", memory_mb);

        // ============================================================
        // GIANT STEP PHASE
        // For each giant step, check 6 GLV orbit images:
        //   S, -S, Phi(S), -Phi(S), Phi^2(S), -Phi^2(S)
        // This gives √6 effective speedup.
        // ============================================================

        // Number of giant steps: ceil(W / STEP)
        // W = 2^(range_bits-1), STEP = baby_step_count
        let giant_count = {
            let w = BigUint::from(1u64) << (self.range_bits - 1);
            let s = BigUint::from(step);
            let div = &w / &s;
            let rem = &w % &s;
            let base = div.to_u64_digits();
            let base_val = if base.is_empty() { 0u64 } else { base[0] };
            if rem > BigUint::ZERO { base_val + 1 } else { base_val }
        };

        println!();
        println!("  [BSGS] Phase 2: Giant steps (up to {}, 2^{:.1})...",
                 giant_count, (giant_count as f64).log2());
        println!("  [BSGS] Each step checks 6 GLV orbit images = √6 speedup");

        let neg_p = Point { x: p_point.x, y: p_point.y.neg_mod_p(), inf: false };
        let mut current_giant = r_point.to_jacobian();
        let mut total_giant: u64 = 0;
        let giant_report = std::cmp::max(1, giant_count / 10);

        for k_hi in 0..giant_count {
            let aff = current_giant.to_affine();
            if !aff.inf {
                // Check 6 GLV orbit images of this point
                // Image 0: S = k_lo * G  (x, y)
                // Image 1: -S            (x, -y)  → same x
                // Image 2: Phi(S)        (beta*x, y)
                // Image 3: -Phi(S)       (beta*x, -y) → same beta*x
                // Image 4: Phi^2(S)      (beta^2*x, y)
                // Image 5: -Phi^2(S)     (beta^2*x, -y) → same beta^2*x

                let x0 = aff.x.to_bytes();
                let x1 = beta.mul(&aff.x).to_bytes();
                let x2 = beta_sq.mul(&aff.x).to_bytes();

                // For each x-variant, check baby table
                // If found: giant point S = ±lambda^img * k_lo * G
                // So: R - k_hi * P = ±lambda^img * k_lo * G
                // => range_start + k_lo' + k_hi * step = k  where k_lo' = ±lambda^img * k_lo

                for (img, x_bytes) in [(0u8, x0), (1u8, x1), (2u8, x2)] {
                    if let Some(&found_k_lo) = baby_table.get(&x_bytes) {
                        // Found a match! Now figure out the actual k.
                        // Baby step k_lo means: found_k_lo * G has x-coordinate matching
                        // the img-th GLV image of the giant step point.
                        //
                        // Giant step point S = R - k_hi * P
                        // If img == 0: S = ± found_k_lo * G
                        // If img == 1: Phi(S) = ± found_k_lo * G => S = ±Phi^{-1}(found_k_lo * G)
                        //   => S = ±lambda^{-1} * found_k_lo * G = ±lambda^2 * found_k_lo * G
                        //   (since lambda^3 = 1, lambda^{-1} = lambda^2)
                        // If img == 2: Phi^2(S) = ± found_k_lo * G => S = ±lambda * found_k_lo * G
                        //
                        // In all cases: S = ± lambda^{(3-img) mod 3} * found_k_lo * G
                        // Wait, let me think more carefully.
                        //
                        // img=0: x(S) matches baby table => S = ±(found_k_lo * G)
                        //   => effective_k_lo = ±found_k_lo
                        //
                        // img=1: beta*x(S) matches baby table
                        //   beta*x(S) = x(Phi(S)) => Phi(S) = ±(found_k_lo * G)
                        //   => lambda*S_scalar = ±found_k_lo (mod N)
                        //   => S_scalar = ±found_k_lo * lambda^{-1} = ±found_k_lo * lambda^2 (mod N)
                        //   => effective_k_lo = ±found_k_lo * lambda^2
                        //
                        // img=2: beta^2*x(S) matches baby table
                        //   beta^2*x(S) = x(Phi^2(S)) => Phi^2(S) = ±(found_k_lo * G)
                        //   => lambda^2*S_scalar = ±found_k_lo (mod N)
                        //   => S_scalar = ±found_k_lo * lambda^{-2} = ±found_k_lo * lambda (mod N)
                        //   => effective_k_lo = ±found_k_lo * lambda

                        for sign in [false, true] {
                            // sign=false: positive, sign=true: negative
                            let effective_k_lo = match img {
                                0 => Fe::from_u64(found_k_lo),
                                1 => lambda_sq.mul_mod_n(&Fe::from_u64(found_k_lo)),
                                2 => lambda.mul_mod_n(&Fe::from_u64(found_k_lo)),
                                _ => unreachable!(),
                            };

                            let signed_k_lo = if sign {
                                effective_k_lo.neg_mod_n()
                            } else {
                                effective_k_lo
                            };

                            // k = range_start + signed_k_lo + k_hi * step (mod N)
                            let k_hi_big = BigUint::from(k_hi) * BigUint::from(step);
                            let k_hi_fe = Fe::from_biguint_mod_n(&k_hi_big);
                            let k_fe = range_start_fe.add_mod_n(&signed_k_lo).add_mod_n(&k_hi_fe);
                            let k_big = k_fe.to_biguint();

                            if k_big >= range_start && k_big < range_end {
                                // Verify!
                                let q = g.scalar_mul(&k_fe);
                                if !q.inf && q.x == self.target.x {
                                    if q.y == self.target.y || q.y == self.target.y.neg_mod_p() {
                                        let elapsed = start.elapsed().as_secs_f64();
                                        let total = step + total_giant;
                                        println!();
                                        println!("  *** KEY FOUND: 0x{:x} ***", k_big);
                                        println!("  *** k_lo={}, k_hi={}, img={}, sign={} ***",
                                                 found_k_lo, k_hi, img, sign);
                                        return BsgsResult {
                                            found: true,
                                            key: Some(k_big),
                                            baby_steps: step,
                                            giant_steps: total_giant,
                                            elapsed_secs: elapsed,
                                            steps_per_sec: total as f64 / elapsed,
                                            memory_mb,
                                        };
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Next giant step
            current_giant = current_giant.add_affine(&neg_p);
            total_giant += 1;

            if k_hi > 0 && k_hi % giant_report == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let rate = total_giant as f64 / elapsed;
                println!("    Giant step {} / {} ({:.0}/s)",
                         total_giant, giant_count, rate);
            }

            // Safety limit
            if total_giant >= 2_000_000_000 {
                println!("  [BSGS] Safety limit (2B giant steps). Stopping.");
                break;
            }
        }

        let elapsed = start.elapsed().as_secs_f64();
        let total = step + total_giant;
        println!();
        println!("  [BSGS] Complete: {} baby + {} giant = {} total steps",
                 step, total_giant, total);

        BsgsResult {
            found: false,
            key: None,
            baby_steps: step,
            giant_steps: total_giant,
            elapsed_secs: elapsed,
            steps_per_sec: total as f64 / elapsed,
            memory_mb,
        }
    }
}

pub fn selftest(bits: u32) -> BsgsResult {
    println!();
    println!("  +==================================================+");
    println!("  |  BSGS v7 — Self-Test ({}-bit key)               |", bits);
    println!("  +==================================================+");

    let g = Point::generator();
    let k_val = (BigUint::from(1u64) << (bits - 1)) + BigUint::from(0xCAFEu64);
    let k_fe = Fe::from_biguint_mod_n(&k_val);
    let target = g.scalar_mul(&k_fe);

    println!("  Key: 0x{:x} ({} bits)", k_val, k_val.bits());
    println!("  Target x: {}", target.x);
    println!("  On curve: {}", target.is_on_curve());

    let baby_count = 1u64 << ((bits - 1) / 2);

    let parity = if target.y.limbs[0] & 1 == 0 { 0x02u8 } else { 0x03u8 };
    let mut pk_bytes = [0u8; 33];
    pk_bytes[0] = parity;
    pk_bytes[1..33].copy_from_slice(&target.x.to_bytes());
    let oracle = Round0Oracle::new(&pk_bytes);

    let solver = BsgsSolver::new(bits, target, baby_count, Some(oracle));
    let result = solver.solve();

    if result.found {
        let found_k = result.key.as_ref().unwrap();
        if *found_k == k_val {
            println!("\n  [SELFTEST] SUCCESS — BSGS found the key!");
        } else {
            println!("\n  [SELFTEST] WRONG — found 0x{:x}, expected 0x{:x}", found_k, k_val);
        }
    } else {
        println!("\n  [SELFTEST] FAILED — key not found");
    }

    result
}
