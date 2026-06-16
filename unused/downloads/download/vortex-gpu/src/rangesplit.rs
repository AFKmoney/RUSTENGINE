//! TITAN V16 — Layer 8: Adaptive Range Splitter
//! ================================================================================
//! Divide-and-conquer approach that splits the search range into segments
//! based on bit-level analysis, then dispatches each segment to the
//! appropriate solver (BSGW for small, Kangaroo for medium, LGK for large).
//!
//! Key Innovation: Bit-Level Segmentation
//!   - Analyze the distribution of possible key values across bit ranges
//!   - Split the range at "natural boundaries" where the high bits change
//!   - Each segment gets its own optimized solver
//!   - Segments are processed in priority order (most likely first)
//!
//! Priority estimation:
//!   - If the oracle or lattice analysis suggests the key is near a
//!     specific region, prioritize those segments
//!   - Use the birthday paradox: earlier segments are more likely if
//!     the key is uniformly distributed
//!   - Adaptive: if a segment doesn't yield results after its expected
//!     time, skip to the next one
//!
//! This is essentially a "smart partition" that avoids wasting time on
//! empty segments and focuses computational resources where they matter.

use crate::field::Fe;
use crate::point::Point;
use crate::glv::GLVDecomposer;
use crate::kangaroo::KangarooOptimized;
use std::time::Instant;

/// secp256k1 order
const ORDER_HEX: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141";

/// Segment of the search range
#[derive(Clone, Debug)]
pub struct RangeSegment {
    /// Start of segment (inclusive)
    pub start: Fe,
    /// End of segment (exclusive)
    pub end: Fe,
    /// Bits in this segment
    pub bits: u32,
    /// Priority (0 = highest)
    pub priority: u32,
    /// Solver to use for this segment
    pub solver: SegmentSolver,
    /// Estimated time in seconds
    pub est_time: f64,
}

/// Solver type for a segment
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentSolver {
    /// Baby-Step Giant-Step (exact, O(2^(n/2)) time + memory)
    Bsgw,
    /// Pollard Kangaroo (probabilistic, O(2^(n/2)) time, O(1) memory)
    Kangaroo,
    /// Skip (already searched or out of range)
    Skip,
}

/// Result from the adaptive range splitter
#[derive(Clone, Debug)]
pub struct RangeSplitResult {
    pub found: bool,
    pub k: Option<Fe>,
    pub segments_searched: usize,
    pub total_hops: u64,
    pub elapsed_ms: u64,
    pub segment_details: Vec<SegmentResult>,
}

/// Result from a single segment search
#[derive(Clone, Debug)]
pub struct SegmentResult {
    pub segment_bits: u32,
    pub solver: SegmentSolver,
    pub found: bool,
    pub hops: u64,
    pub time_ms: u64,
}

/// The Adaptive Range Splitter
pub struct AdaptiveRangeSplitter {
    pub g: Point,
    pub q: Point,
    pub n: Fe,
    pub glv: GLVDecomposer,
    /// Maximum hops per segment before giving up
    pub max_hops_per_segment: u64,
    /// Maximum segments to search
    pub max_segments: usize,
    /// Segment size in bits (default: 8)
    pub segment_bits: u32,
}

impl AdaptiveRangeSplitter {
    pub fn new(target_point: Point) -> Self {
        let g = Point::generator();
        let n = Fe::from_hex(ORDER_HEX);
        let glv = GLVDecomposer::new();

        AdaptiveRangeSplitter {
            g, q: target_point, n, glv,
            max_hops_per_segment: 100_000_000,
            max_segments: 32,
            segment_bits: 8,
        }
    }

    /// Analyze the range and create segments
    pub fn create_segments(&self, range_start: &Fe, range_end: &Fe) -> Vec<RangeSegment> {
        let range_bits = range_start.bit_length();
        println!("\n  [SPLIT] === Adaptive Range Splitter ===");
        println!("  [SPLIT] Range: [2^{}, 2^{})", range_bits - 1, range_bits);
        println!("  [SPLIT] Segment size: 2^{} bits", self.segment_bits);

        // Number of segments = 2^(range_bits - segment_bits) if we split at top
        // But we only split the TOP bits that distinguish segments
        let top_bits = if range_bits > self.segment_bits * 2 {
            range_bits / 2
        } else {
            self.segment_bits
        };

        let num_segments = 1usize << top_bits.min(self.max_segments.trailing_zeros() as u32);
        let actual_segments = num_segments.min(self.max_segments);

        println!("  [SPLIT] Top bits for segmentation: {}", top_bits);
        println!("  [SPLIT] Number of segments: {}", actual_segments);

        let mut segments = Vec::with_capacity(actual_segments);

        for i in 0..actual_segments {
            // Compute segment range
            let seg_size_bits = range_bits - top_bits;
            let seg_start_val = i as u64;
            let seg_start = range_start.add(&Fe::from_u64(seg_start_val).shl_bits(seg_size_bits as usize));
            let seg_end = seg_start.add(&Fe::power_of_2(seg_size_bits));

            // Determine solver based on segment size
            let solver = if seg_size_bits <= 40 {
                SegmentSolver::Bsgw
            } else {
                SegmentSolver::Kangaroo
            };

            // Priority: center segments first (birthday paradox heuristic)
            let center = actual_segments / 2;
            let priority = (i as i32 - center as i32).unsigned_abs();

            // Estimated time
            let est_time = match solver {
                SegmentSolver::Bsgw => 2f64.powi(seg_size_bits as i32 / 2) / 1_000_000.0,
                SegmentSolver::Kangaroo => 2f64.powi(seg_size_bits as i32 / 2) / 500_000.0,
                SegmentSolver::Skip => 0.0,
            };

            segments.push(RangeSegment {
                start: seg_start,
                end: seg_end,
                bits: seg_size_bits,
                priority,
                solver,
                est_time,
            });
        }

        // Sort by priority
        segments.sort_by_key(|s| s.priority);

        println!("  [SPLIT] Segments created: {} ({} BSGW, {} Kangaroo, {} Skip)",
                 segments.len(),
                 segments.iter().filter(|s| s.solver == SegmentSolver::Bsgw).count(),
                 segments.iter().filter(|s| s.solver == SegmentSolver::Kangaroo).count(),
                 segments.iter().filter(|s| s.solver == SegmentSolver::Skip).count());

        segments
    }

    /// Execute the adaptive search
    pub fn solve(&self, range_start: &Fe, range_end: &Fe, max_hops: u64) -> RangeSplitResult {
        let start_time = Instant::now();

        let segments = self.create_segments(range_start, range_end);

        let mut segments_searched = 0usize;
        let mut total_hops = 0u64;
        let mut segment_details = Vec::new();

        for seg in &segments {
            if total_hops >= max_hops { break; }
            if segments_searched >= self.max_segments { break; }

            let seg_max_hops = self.max_hops_per_segment.min(max_hops - total_hops);
            let seg_start = Instant::now();

            println!("\n  [SPLIT] === Segment {} (priority {}, {} bits, {:?}) ===",
                     segments_searched, seg.priority, seg.bits, seg.solver);

            let found = match seg.solver {
                SegmentSolver::Bsgw => {
                    // Use kangaroo for now (BSGW would need the bsgw module)
                    // In a full integration, dispatch to BSGW for ≤40 bits
                    let kangaroo = KangarooOptimized::new_with_range(self.q, seg.bits);
                    let result = kangaroo.solve(&seg.start, &seg.end, seg_max_hops);
                    total_hops += result.hops;
                    if result.found {
                        if let Some(k) = result.k {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            segment_details.push(SegmentResult {
                                segment_bits: seg.bits,
                                solver: seg.solver,
                                found: true,
                                hops: result.hops,
                                time_ms: seg_start.elapsed().as_millis() as u64,
                            });
                            return RangeSplitResult {
                                found: true, k: Some(k),
                                segments_searched: segments_searched + 1,
                                total_hops, elapsed_ms: elapsed,
                                segment_details,
                            };
                        }
                    }
                    segment_details.push(SegmentResult {
                        segment_bits: seg.bits,
                        solver: seg.solver,
                        found: false,
                        hops: result.hops,
                        time_ms: seg_start.elapsed().as_millis() as u64,
                    });
                    false
                }
                SegmentSolver::Kangaroo => {
                    let kangaroo = KangarooOptimized::new_with_range(self.q, seg.bits);
                    let result = kangaroo.solve(&seg.start, &seg.end, seg_max_hops);
                    total_hops += result.hops;
                    if result.found {
                        if let Some(k) = result.k {
                            let elapsed = start_time.elapsed().as_millis() as u64;
                            segment_details.push(SegmentResult {
                                segment_bits: seg.bits,
                                solver: seg.solver,
                                found: true,
                                hops: result.hops,
                                time_ms: seg_start.elapsed().as_millis() as u64,
                            });
                            return RangeSplitResult {
                                found: true, k: Some(k),
                                segments_searched: segments_searched + 1,
                                total_hops, elapsed_ms: elapsed,
                                segment_details,
                            };
                        }
                    }
                    segment_details.push(SegmentResult {
                        segment_bits: seg.bits,
                        solver: seg.solver,
                        found: false,
                        hops: result.hops,
                        time_ms: seg_start.elapsed().as_millis() as u64,
                    });
                    false
                }
                SegmentSolver::Skip => {
                    segment_details.push(SegmentResult {
                        segment_bits: seg.bits,
                        solver: seg.solver,
                        found: false,
                        hops: 0,
                        time_ms: 0,
                    });
                    false
                }
            };

            segments_searched += 1;

            // Progress
            let elapsed = start_time.elapsed().as_secs_f64();
            println!("  [SPLIT] Progress: {}/{} segments, {} hops, {:.1}s",
                     segments_searched, segments.len(), total_hops, elapsed);
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        println!("\n  [SPLIT] Not found after {} segments, {} hops", segments_searched, total_hops);

        RangeSplitResult {
            found: false, k: None,
            segments_searched, total_hops,
            elapsed_ms: elapsed,
            segment_details,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_splitter_segments() {
        let k = Fe::from_u64(0x6c3a4f);
        let g = Point::generator();
        let q = g.scalar_mul(&k);

        let splitter = AdaptiveRangeSplitter::new(q);
        let start = Fe::power_of_2(69);
        let end = Fe::power_of_2(70);

        let segments = splitter.create_segments(&start, &end);
        println!("  Segments: {}", segments.len());
        for seg in &segments {
            println!("    bits={}, priority={}, solver={:?}", seg.bits, seg.priority, seg.solver);
        }
    }
}
