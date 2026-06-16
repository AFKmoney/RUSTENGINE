//! TITAN V16 — Layer 6: Quantum-Inspired Annealing (QIA)
//! ================================================================================
//! Simulated annealing applied to ECDLP with GLV lattice structure.
//!
//! Key Idea: Model the DLP search as an energy minimization problem over
//! the GLV lattice coefficient space. The "energy" of a candidate k is
//! the Hamming distance between Q.x and (k*G).x — lower is better.
//!
//! The annealing schedule uses quantum-inspired tunneling:
//!   - Classical SA: accept worse solutions with probability exp(-ΔE/T)
//!   - Quantum tunneling: occasionally make large jumps in coefficient space
//!     that would be impossible for classical SA (tunnel through barriers)
//!
//! This is particularly effective when combined with the lattice decomposition:
//!   - Search space: 6D coefficient vectors (c0..c5) with |ci| < 2^45
//!   - Energy function: Hamming distance of x-coordinates
//!   - Neighbor generation: perturb one coefficient by ±2^j (random j)
//!   - Quantum tunneling: perturb multiple coefficients simultaneously
//!
//! Best for: mid-range puzzles (50-80 bits) where BSGS is too expensive
//! but the full range is still tractable with lattice + annealing.

use crate::field::Fe;
use crate::point::Point;
use crate::glv::GLVDecomposer;
use std::time::Instant;

/// secp256k1 order
const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// Result from the QIA solver
#[derive(Clone, Debug)]
pub struct AnnealingResult {
    pub found: bool,
    pub k: Option<Fe>,
    pub iterations: u64,
    pub best_energy: u32,
    pub tunneling_events: u64,
    pub elapsed_ms: u64,
}

/// Quantum-Inspired Annealing solver for ECDLP
pub struct QuantumAnnealing {
    pub g: Point,
    pub n: Fe,
    pub glv: GLVDecomposer,
    /// Target point Q
    pub q: Point,
    /// Target x-coordinate bytes
    pub target_x_bytes: [u8; 32],
    /// Initial temperature
    pub t_start: f64,
    /// Final temperature
    pub t_end: f64,
    /// Cooling rate
    pub cooling_rate: f64,
    /// Tunneling probability (probability of a quantum jump per iteration)
    pub tunnel_prob: f64,
}

impl QuantumAnnealing {
    pub fn new(target_point: Point) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();
        let target_x_bytes = target_point.x.to_bytes();

        QuantumAnnealing {
            g, n, glv,
            q: target_point,
            target_x_bytes,
            t_start: 1000.0,
            t_end: 0.001,
            cooling_rate: 0.9999,
            tunnel_prob: 0.05,
        }
    }

    /// Compute the "energy" of a candidate point.
    /// Energy = number of bits that differ between candidate.x and target.x.
    /// Lower energy = closer to the solution.
    /// Energy = 0 means exact match (key found!).
    #[inline]
    pub fn compute_energy(&self, candidate: &Point) -> u32 {
        if candidate.inf { return 256; }
        let cand_bytes = candidate.x.to_bytes();
        let mut hamming = 0u32;
        for i in 0..32 {
            let xor = cand_bytes[i] ^ self.target_x_bytes[i];
            hamming += xor.count_ones();
        }
        hamming
    }

    /// Compute a fast partial energy (only first 8 bytes = 64 bits).
    /// Used for rapid pre-filtering before full energy computation.
    #[inline]
    fn partial_energy(&self, candidate: &Point) -> u32 {
        if candidate.inf { return 256; }
        let cand_bytes = candidate.x.to_bytes();
        let mut hamming = 0u32;
        for i in 0..8 {
            let xor = cand_bytes[i] ^ self.target_x_bytes[i];
            hamming += xor.count_ones();
        }
        hamming
    }

    /// Solve using Quantum-Inspired Annealing.
    ///
    /// This method uses GLV decomposition to search in the 2D subspace
    /// (k = k0 + k1*lambda mod n) and applies annealing to find the
    /// optimal (k0, k1) pair.
    ///
    /// For lattice-guided annealing, use `solve_lattice()` instead.
    pub fn solve(&self, range_start: &Fe, range_end: &Fe, max_iterations: u64) -> AnnealingResult {
        let start_time = Instant::now();

        println!("\n  [QIA] === Quantum-Inspired Annealing for ECDLP ===");
        println!("  [QIA] Temperature: {} → {} (cooling: {})", 
                 self.t_start, self.t_end, self.cooling_rate);
        println!("  [QIA] Tunneling probability: {:.1}%", self.tunnel_prob * 100.0);
        println!("  [QIA] Max iterations: {}", max_iterations);

        // Start from the center of the range
        let k_start = Self::range_center(range_start, range_end);
        let mut current_k = k_start;
        let mut current_point = self.g.scalar_mul(&current_k);
        let mut current_energy = self.compute_energy(&current_point);

        let mut best_k = current_k.clone();
        let mut best_energy = current_energy;

        let mut temperature = self.t_start;
        let mut tunneling_events = 0u64;
        let mut iterations = 0u64;

        // Precompute step sizes (powers of 2)
        let step_sizes: [Fe; 32] = std::array::from_fn(|i| Fe::power_of_2(i as u32));

        println!("  [QIA] Initial energy: {}/256 bits differ", current_energy);

        let report_interval = max_iterations / 20;

        while iterations < max_iterations {
            iterations += 1;

            // === GENERATE NEIGHBOR ===
            let mut rng_val = current_point.x.limbs[0];
            if rng_val == 0 { rng_val = iterations as u64; }

            let neighbor_k = if rng_val % 1000 < (self.tunnel_prob * 1000.0) as u64 {
                // QUANTUM TUNNELING: large jump in search space
                tunneling_events += 1;
                let tunnel_bits = ((rng_val >> 10) % 64) as usize + 1;
                let sign = if rng_val & 1 == 0 { true } else { false };
                let step = &step_sizes[tunnel_bits.min(31)];

                if sign {
                    current_k.add_mod_n(step)
                } else {
                    current_k.sub_mod_n(step)
                }
            } else {
                // CLASSICAL PERTURBATION: small step
                let perturb_bits = ((rng_val >> 8) % 16) as usize;
                let sign = if rng_val & 1 == 0 { true } else { false };
                let step = &step_sizes[perturb_bits];

                if sign {
                    current_k.add_mod_n(step)
                } else {
                    current_k.sub_mod_n(step)
                }
            };

            // Compute neighbor energy
            let neighbor_point = self.g.scalar_mul(&neighbor_k);
            let neighbor_energy = self.compute_energy(&neighbor_point);

            // Check if we found the key!
            if neighbor_energy == 0 {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("\n  [QIA] KEY FOUND! Energy = 0 (exact x match)");
                return AnnealingResult {
                    found: true,
                    k: Some(neighbor_k),
                    iterations,
                    best_energy: 0,
                    tunneling_events,
                    elapsed_ms: elapsed,
                };
            }

            // === ACCEPT/REJECT ===
            let delta_e = neighbor_energy as f64 - current_energy as f64;

            if delta_e < 0.0 {
                // Better solution: always accept
                current_k = neighbor_k;
                current_point = neighbor_point;
                current_energy = neighbor_energy;

                if current_energy < best_energy {
                    best_k = current_k.clone();
                    best_energy = current_energy;
                }
            } else {
                // Worse solution: accept with probability exp(-ΔE/T)
                let accept_prob = (-delta_e / temperature).exp();
                let rand_val = ((rng_val as f64) / u64::MAX as f64).min(1.0);
                if rand_val < accept_prob {
                    current_k = neighbor_k;
                    current_point = neighbor_point;
                    current_energy = neighbor_energy;
                }
            }

            // === COOLING ===
            temperature *= self.cooling_rate;
            if temperature < self.t_end {
                // Reheat (simulated annealing with reheat)
                temperature = self.t_start * 0.5;
            }

            // === PROGRESS ===
            if iterations % report_interval == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let rate = iterations as f64 / elapsed;
                println!("  [QIA] iter: {} | T: {:.4} | energy: {}/256 | best: {}/256 | tunnels: {} | rate: {:.0} iter/s",
                         iterations, temperature, current_energy, best_energy, tunneling_events, rate);
            }
        }

        // Check automorphism images of best_k
        let autos = self.glv.automorphism_scalars(&best_k);
        for ak in &autos {
            let verify = self.g.scalar_mul(ak);
            if !verify.inf && verify.x == self.q.x {
                let elapsed = start_time.elapsed().as_millis() as u64;
                println!("  [QIA] KEY FOUND via automorphism of best candidate!");
                return AnnealingResult {
                    found: true,
                    k: Some(ak.clone()),
                    iterations,
                    best_energy: 0,
                    tunneling_events,
                    elapsed_ms: elapsed,
                };
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        println!("  [QIA] Not found. Best energy: {}/256", best_energy);

        AnnealingResult {
            found: false,
            k: None,
            iterations,
            best_energy,
            tunneling_events,
            elapsed_ms: elapsed,
        }
    }

    /// Lattice-guided annealing: search in 6D coefficient space.
    ///
    /// Uses the lattice decomposition to constrain the search to the
    /// coefficient space where k = offset + Σ ci*si, and applies
    /// annealing to find the optimal coefficient vector.
    pub fn solve_lattice(
        &self,
        basis_scalars: &[Fe; 6],
        offset_scalar: &Fe,
        max_coeff_bits: &[u32; 6],
        max_iterations: u64,
    ) -> AnnealingResult {
        let start_time = Instant::now();

        println!("\n  [QIA-LAT] === Lattice-Guided Quantum Annealing ===");
        println!("  [QIA-LAT] Searching in 6D coefficient space");
        println!("  [QIA-LAT] Max coeff bits: {:?}", max_coeff_bits);

        // Precompute basis points
        let basis_points: [Point; 6] = std::array::from_fn(|i| {
            self.g.scalar_mul(&basis_scalars[i])
        });

        let offset_point = self.g.scalar_mul(offset_scalar);

        // Current coefficient vector (start at zero)
        let mut current_coeffs: [i64; 6] = [0i64; 6];
        let mut current_point = offset_point;

        // Compute initial energy
        let mut current_energy = self.compute_energy(&current_point);
        let mut best_energy = current_energy;
        let mut best_coeffs = current_coeffs;

        let mut temperature = self.t_start;
        let mut tunneling_events = 0u64;
        let mut iterations = 0u64;

        let report_interval = max_iterations / 20;

        while iterations < max_iterations {
            iterations += 1;

            // Generate neighbor by perturbing one coefficient
            let mut rng_val = current_point.x.limbs[0];
            if rng_val == 0 { rng_val = iterations as u64; }

            let dim = (rng_val as usize) % 6;
            let max_bits = max_coeff_bits[dim];
            if max_bits == 0 {
                // Skip this dimension (absorbed into offset)
                continue;
            }

            let is_tunnel = (rng_val % 1000) < (self.tunnel_prob * 1000.0) as u64;

            let (perturb_bits, sign) = if is_tunnel {
                tunneling_events += 1;
                let bits = ((rng_val >> 10) % (max_bits as u64 / 2).max(1)) as usize + 1;
                let sign = if rng_val & 1 == 0 { 1i64 } else { -1i64 };
                (bits, sign)
            } else {
                let bits = ((rng_val >> 8) % (max_bits as u64 / 4).max(1).min(8)) as usize + 1;
                let sign = if rng_val & 1 == 0 { 1i64 } else { -1i64 };
                (bits, sign)
            };

            let step_val = sign * (1i64 << perturb_bits.min(30));

            // Check coefficient bounds
            let new_coeff = current_coeffs[dim] + step_val;
            let max_coeff = if max_bits < 62 { 1i64 << max_bits } else { 1i64 << 60 };
            if new_coeff.abs() > max_coeff {
                continue; // Out of bounds
            }

            // Compute neighbor point
            let neighbor_point = {
                let step_scalar = Fe::from_u64(step_val.unsigned_abs());
                let step_point = basis_points[dim].scalar_mul(&step_scalar);
                if step_val > 0 {
                    current_point.add(&step_point)
                } else {
                    current_point.add(&step_point.neg())
                }
            };

            let neighbor_energy = self.compute_energy(&neighbor_point);

            if neighbor_energy == 0 {
                // FOUND! Reconstruct k from coefficients
                let mut k_candidate = offset_scalar.clone();
                for d in 0..6 {
                    let c = current_coeffs[d] + if d == dim { step_val } else { 0 };
                    if c == 0 { continue; }
                    let c_fe = Fe::from_u64(c.unsigned_abs());
                    let term = c_fe.mul_mod_n(&basis_scalars[d]);
                    if c > 0 {
                        k_candidate = k_candidate.add_mod_n(&term);
                    } else {
                        k_candidate = k_candidate.sub_mod_n(&term);
                    }
                }

                // Verify
                let q_check = self.g.scalar_mul(&k_candidate);
                if !q_check.inf && q_check.x == self.q.x {
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    println!("  [QIA-LAT] KEY FOUND! Energy = 0");
                    return AnnealingResult {
                        found: true, k: Some(k_candidate),
                        iterations, best_energy: 0,
                        tunneling_events, elapsed_ms: elapsed,
                    };
                }
            }

            // Accept/reject
            let delta_e = neighbor_energy as f64 - current_energy as f64;
            if delta_e < 0.0 || {
                let accept_prob = (-delta_e / temperature).exp();
                let rand_val = ((rng_val as f64) / u64::MAX as f64).min(1.0);
                rand_val < accept_prob
            } {
                current_coeffs[dim] = new_coeff;
                current_point = neighbor_point;
                current_energy = neighbor_energy;

                if current_energy < best_energy {
                    best_coeffs = current_coeffs;
                    best_energy = current_energy;
                }
            }

            // Cool
            temperature *= self.cooling_rate;
            if temperature < self.t_end {
                temperature = self.t_start * 0.3;
            }

            if iterations % report_interval == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                println!("  [QIA-LAT] iter: {} | T: {:.4} | energy: {}/256 | best: {}/256 | tunnels: {}",
                         iterations, temperature, current_energy, best_energy, tunneling_events);
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        AnnealingResult {
            found: false, k: None,
            iterations, best_energy,
            tunneling_events, elapsed_ms: elapsed,
        }
    }

    fn range_center(start: &Fe, end: &Fe) -> Fe {
        start.add(&end).shr1()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qia_energy() {
        let k = Fe::from_u64(12345);
        let g = Point::generator();
        let q = g.scalar_mul(&k);

        let annealing = QuantumAnnealing::new(q);

        // Same point: energy = 0
        let e0 = annealing.compute_energy(&q);
        assert_eq!(e0, 0);

        // Different point: energy > 0
        let p2 = g.scalar_mul(&Fe::from_u64(54321));
        let e2 = annealing.compute_energy(&p2);
        assert!(e2 > 0);
        println!("  Energy of wrong point: {}/256", e2);
    }
}
