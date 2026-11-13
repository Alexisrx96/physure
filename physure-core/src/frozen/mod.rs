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
pub mod active;
pub use active::{
    active, ensure_valid, refuse_unwired, reset_cache, set_engine_file, set_explicit_conf, Active, ActiveError, Resolution,
    ResolveError, Why,
};
pub mod load;
pub use load::{load_file, FileStamp, FrozenCatalog, LoadError, LoadOptions, MAX_FILE_BYTES};
pub mod ops;
pub use ops::{
    actor_from_env, freeze, now_unix, plan_seal, plan_update, write_atomic, FreezeOptions, OpsError, SealPlan, UpdatePlan, EMPTY_OVERRIDES,
};
pub mod entries;
pub use entries::{diff, embedded_entries, entries, summary, Change, Entry};
pub mod report;
pub use report::{
    catalog_entries, compare, history_rows, inspect, unfrozen_entries, Comparison, EngineState, FieldChange, HistoryRow, Inspection, Side,
    UNFROZEN_LABEL,
};
pub mod origin;
pub use origin::{entries_with_origin, section_and_key, select, Origin};
