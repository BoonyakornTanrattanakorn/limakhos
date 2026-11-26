//! Retrograde WDL/DTZ endgame tablebase generator and prober for Thai Draughts (Makhos 8),
//! calibrated to the TCA 2025 32-ply (16-full-move) King-only draw rule.

pub mod format;
pub mod gen;
pub mod index;
pub mod probe;

pub use format::{TablebaseSlice, TbEntry, Wdl, FORMAT_VERSION, MAGIC};
pub use gen::{generate_all, generate_slice};
pub use index::{SliceIndexer, SliceKey};
pub use probe::{TablebaseSet, TbMoveEval, TbProbeDetails, TbResult};
