//! Freezing the configuration: the data model of a sealed `physure.frozen.conf` and the hashes that seal it.
//!
//! See `docs/superpowers/specs/2026-11-07-conf-freeze-design.md`. The model, the canonical form and the seal are
//! pure: they read no clock, no environment and no file. Only `engine` looks outside: the identity the build script
//! compiled in, [`environment_pairs`] (the machine, informational) and [`sha256_of_file`]. Nothing here touches the
//! unit registry.

pub mod sha256;
pub mod canonical;
pub mod time;
pub mod model;
pub use model::{FrozenError, FrozenFile, FrozenResult, HistoryEntry, Pairs, Source, WitnessScript};
pub mod seal;
pub use seal::{EntryInput, FrozenParts, Problem};
pub mod engine;
pub use engine::{
    check, engine_pairs, environment_pairs, freezable, read_engine, sha256_of_file, Difference, EngineBlock, EngineIdentity,
    EngineMismatch, EnginePolicy,
};
