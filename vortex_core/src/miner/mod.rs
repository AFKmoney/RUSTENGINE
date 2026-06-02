//! Module Minage — orchestration du scanner et du scheduler
//!
//! Fournit :
//! - `Miner` : structure principale qui orchestre le scan de nonces
//! - `MiningResult` : résultat d'une session de minage
//! - Intégration du midstate, scanner SIMD, et early termination

pub mod scanner;
pub mod scheduler;
pub mod early_term;

pub use scanner::MiningScanner;
pub use scheduler::NonceScheduler;
pub use early_term::Target;
