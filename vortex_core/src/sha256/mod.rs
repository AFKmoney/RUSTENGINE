//! Module SHA-256 — implémentation scalaire + SIMD
//!
//! Ce module fournit :
//! - `sha256_scalar` : implémentation pure Rust (portable, fallback)
//! - `compress_block` : compression d'un bloc 512-bit avec état intermédiaire
//! - Constantes K et logiques de schedule de messages

pub mod scalar;
pub mod midstate;

#[cfg(target_arch = "x86_64")]
pub mod simd;

pub use scalar::{sha256d, sha256_raw, compress_block, Sha256State};
pub use midstate::MidstateCache;
