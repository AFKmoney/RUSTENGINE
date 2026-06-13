//! Kangaroo Debug — Systematic diagnosis of the kangaroo collision bug
//! Tests each component in isolation, then runs a full kangaroo with
//! DP_MASK_BITS=0 (every point is a DP) to find the root cause.

use crate::field::Fe;
use crate::point::{Point, JacobianPoint};
use std::collections::HashSet;

/// Test 1: scalar_mul correctness
pub fn test_scalar_mul() {
    println!("\n  [DEBUG-1] === scalar_mul Correctness ===");
    let g = Point::generator();
    
    for k_val in [1u64, 2, 3, 7, 100, 255, 0xFFFF].iter() {
        let k = Fe::from_u64(*k_val);
        let q = g.scalar_mul(&k);
        let on_curve = q.is_on_curve();
        println!("  scalar_mul({}) → on_curve: {}, inf: {}", k_val, on_curve, q.inf);
    }
    
    let k7 = Fe::from_u64(7);
    let g7 = g.scalar_mul(&k7);
    let expected_7g_x = Fe::from_hex("5cbdf0646e5db4eaa398f365f2ea7a0e3d419b7e0330e39ce92bddedcac4f9bc");
    println!("  7*G.x matches known: {}", g7.x == expected_7g_x);
    
    let k2 = Fe::from_u64(2);
    let g2 = g.scalar_mul(&k2);
    let expected_2g_x = Fe::from_hex("c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5");
    println!("  2*G.x matches known: {}", g2.x == expected_2g_x);
}

/// Test 2: add_affine correctness
pub fn test_add_affine() {
    println!("\n  [DEBUG-2] === add_affine Correctness ===");
    let g = Point::generator();
    
    let test_cases = [
        (100u64, 200u64),
        (50u64, 50u64),
        (1u64, 1u64),
        (255u64, 1u64),
        (1000u64, 500u64),
    ];
    
    for (k_val, s_val) in test_cases.iter() {
        let k = Fe::from_u64(*k_val);
        let s = Fe::from_u64(*s_val);
        let expected_k = Fe::from_u64(*k_val + *s_val);
        
        let p = g.scalar_mul(&k);
        let step = g.scalar_mul(&s);
        let expected = g.scalar_mul(&expected_k);
        
        let result = p.to_jacobian().add_affine(&step).to_affine();
        
        let x_match = result.x == expected.x;
        let y_match = result.y == expected.y;
        let on_curve = result.is_on_curve();
        
        println!("  {}*G + {}*G → x_match: {}, y_match: {}, on_curve: {}", 
                 k_val, s_val, x_match, y_match, on_curve);
    }
}

/// Test 3: Multi-hop scalar tracking consistency
pub fn test_multi_hop() {
    println!("\n  [DEBUG-3] === Multi-Hop Consistency ===");
    let g = Point::generator();
    
    let start_k = Fe::from_u64(192);
    let mut point = g.scalar_mul(&start_k).to_jacobian();
    let mut scalar = start_k;
    
    let step_sizes = [2u64, 4, 8, 16, 32, 64, 128];
    
    for (i, &step_val) in step_sizes.iter().cycle().take(20).enumerate() {
        let step_scalar = Fe::from_u64(step_val);
        let step_point = g.scalar_mul(&step_scalar);
        
        point = point.add_affine(&step_point);
        scalar = scalar.add_mod_n(&step_scalar);
        
        let expected = g.scalar_mul(&scalar);
        let actual = point.to_affine();
        
        let x_match = actual.x == expected.x;
        if !x_match || i < 5 {
            println!("  Hop {}: step={}, → x_match: {}, on_curve: {}", 
                     i, step_val, x_match, actual.is_on_curve());
        }
        
        if !x_match {
            println!("  *** MISMATCH at hop {}! ***", i);
            break;
        }
    }
}

/// Test 7: FULL kangaroo walk with DP_MASK_BITS=0 on a 16-bit key
/// This is the definitive test — if this doesn't produce collisions,
/// there's a fundamental bug in the walk logic.
pub fn test_full_kangaroo_16bit() {
    println!("\n  [DEBUG-7] === FULL Kangaroo Walk (16-bit, DP_MASK=0) ===");
    let g = Point::generator();
    
    let k_val = 0xFFFFu64;
    let k = Fe::from_u64(k_val);
    let q = g.scalar_mul(&k);
    println!("  k = 0x{:x}, Q on curve: {}", k_val, q.is_on_curve());
    
    let range_bits: u32 = 17; // range = [2^16, 2^17)
    let rs = Fe::power_of_2(range_bits - 1);
    let re = Fe::power_of_2(range_bits);
    
    // Check k is in range
    let in_range = k.cmp_val(&rs.limbs).is_ge() && k.cmp_val(&re.limbs).is_lt();
    println!("  k in [2^{}, 2^{}): {}", range_bits - 1, range_bits, in_range);
    
    // Step sizes: same formula as kangaroo.rs V16.2
    let num_steps = 32;
    let base_exp = if range_bits >= 14 { (range_bits / 2).saturating_sub(6) as usize } else { 0 };
    let base_unit = if base_exp == 0 { Fe::from_u64(1) } else { Fe::power_of_2(base_exp as u32) };
    
    let step_scalars: Vec<Fe> = (0..num_steps)
        .map(|j| base_unit.mul_mod_n(&Fe::from_u64((j as u64) + 1)))
        .collect();
    let step_points: Vec<Point> = step_scalars.iter().map(|s| g.scalar_mul(s)).collect();
    
    println!("  base_unit = 2^{}, step range: {} to {} * base", base_exp, 1, num_steps);
    
    // Tame: starts at range_end * G
    let mut tame_pt = g.scalar_mul(&re).to_jacobian();
    let mut tame_scalar = re.clone();
    
    // Wild: starts at Q
    let mut wild_pt = q.to_jacobian();
    let mut wild_scalar = Fe::from_u64(0);
    
    // Track ALL x-coordinates (DP_MASK_BITS=0)
    let mut tame_xs: HashSet<[u8; 32]> = HashSet::new();
    let mut wild_xs: HashSet<[u8; 32]> = HashSet::new();
    let mut collisions = 0u64;
    let mut step_dist = vec![0u32; num_steps];
    
    let num_hops = 500_000;
    
    for hop in 0..num_hops {
        // Normalize for hash (CORRECT deterministic walk)
        let tame_x_norm = normalize_x(&tame_pt);
        let tame_step = (tame_x_norm.limbs[0] as usize) % num_steps;
        
        let wild_x_norm = normalize_x(&wild_pt);
        let wild_step = (wild_x_norm.limbs[0] as usize) % num_steps;
        
        step_dist[tame_step] += 1;
        
        // Do the hops
        tame_pt = tame_pt.add_affine(&step_points[tame_step]);
        tame_scalar = tame_scalar.add_mod_n(&step_scalars[tame_step]);
        
        wild_pt = wild_pt.add_affine(&step_points[wild_step]);
        wild_scalar = wild_scalar.add_mod_n(&step_scalars[wild_step]);
        
        // Collect x-coordinates (DP_MASK=0, every point is DP)
        if !tame_pt.z.is_zero() && !wild_pt.z.is_zero() {
            let tx = normalize_x(&tame_pt).to_bytes();
            let wx = normalize_x(&wild_pt).to_bytes();
            
            tame_xs.insert(tx);
            wild_xs.insert(wx);
            
            // Check collision
            if tx == wx {
                collisions += 1;
                println!("  *** COLLISION at hop {}! ***", hop);
                
                // Try key recovery
                let k_pos = tame_scalar.sub_mod_n(&wild_scalar);
                let verify_pos = g.scalar_mul(&k_pos);
                if !verify_pos.inf && verify_pos.x == q.x {
                    println!("  *** KEY FOUND (positive)! ***");
                    return;
                }
                
                let k_neg = tame_scalar.add_mod_n(&wild_scalar).neg_mod_n();
                let verify_neg = g.scalar_mul(&k_neg);
                if !verify_neg.inf && verify_neg.x == q.x {
                    println!("  *** KEY FOUND (negative)! ***");
                    return;
                }
                
                println!("  Collision but key recovery failed. Continuing...");
            }
        }
        
        // Progress
        if hop > 0 && hop % 100_000 == 0 {
            let overlap = tame_xs.intersection(&wild_xs).count();
            println!("  Hop {}: tame_unique={}, wild_unique={}, overlap={}, coll={}", 
                hop, tame_xs.len(), wild_xs.len(), overlap, collisions);
            println!("    Step dist: {:?}", &step_dist[..8]);
        }
    }
    
    let overlap = tame_xs.intersection(&wild_xs).count();
    println!("\n  Final: tame_unique={}, wild_unique={}, overlap={}, collisions={}", 
        tame_xs.len(), wild_xs.len(), overlap, collisions);
    
    // Verify scalar tracking
    let tame_check = g.scalar_mul(&tame_scalar);
    let tame_actual = tame_pt.to_affine();
    println!("  Tame scalar tracking: {}", if tame_check.x == tame_actual.x { "CORRECT" } else { "WRONG" });
    
    let wild_total = k.add_mod_n(&wild_scalar);
    let wild_check = g.scalar_mul(&wild_total);
    let wild_actual = wild_pt.to_affine();
    println!("  Wild scalar tracking: {}", if wild_check.x == wild_actual.x { "CORRECT" } else { "WRONG" });
    
    // The CRITICAL test: check if the tame and wild are even visiting 
    // the same REGION of the group
    println!("\n  --- Region Analysis ---");
    // Are the walks cycling? Check tame self-collisions
    let mut tame_self_coll = 0u64;
    let mut tame_seen: HashSet<[u8; 32]> = HashSet::new();
    // Re-run a short walk to check
    let mut pt = g.scalar_mul(&re).to_jacobian();
    for _ in 0..10_000 {
        let x_norm = normalize_x(&pt);
        let step = (x_norm.limbs[0] as usize) % num_steps;
        pt = pt.add_affine(&step_points[step]);
        if !pt.z.is_zero() {
            let x_bytes = normalize_x(&pt).to_bytes();
            if !tame_seen.insert(x_bytes) {
                tame_self_coll += 1;
            }
        }
    }
    println!("  Tame self-collisions in 10K hops: {} (cycles!)", tame_self_coll);
    
    // Same for wild
    let mut wild_self_coll = 0u64;
    let mut wild_seen: HashSet<[u8; 32]> = HashSet::new();
    let mut pt2 = q.to_jacobian();
    for _ in 0..10_000 {
        let x_norm = normalize_x(&pt2);
        let step = (x_norm.limbs[0] as usize) % num_steps;
        pt2 = pt2.add_affine(&step_points[step]);
        if !pt2.z.is_zero() {
            let x_bytes = normalize_x(&pt2).to_bytes();
            if !wild_seen.insert(x_bytes) {
                wild_self_coll += 1;
            }
        }
    }
    println!("  Wild self-collisions in 10K hops: {} (cycles!)", wild_self_coll);
}

/// Normalize the x-coordinate of a Jacobian point: x = X/Z²
fn normalize_x(point: &JacobianPoint) -> Fe {
    if point.z.is_zero() {
        return Fe::ZERO;
    }
    let z_inv = point.z.modinv();
    let z_inv_sq = z_inv.mul(&z_inv);
    point.x.mul(&z_inv_sq)
}

/// Run all debug tests
pub fn run_all_debug() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  KANGAROO DEBUG — Systematic Diagnosis V2               ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    
    test_scalar_mul();
    test_add_affine();
    test_multi_hop();
    test_full_kangaroo_16bit();
    
    println!("\n  [DEBUG] All diagnosis tests complete.");
}
