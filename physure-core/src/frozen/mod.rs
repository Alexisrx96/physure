//! Freezing the configuration: the data model of a sealed `physure.frozen.conf` and the hashes that seal it.
//!
//! See `docs/superpowers/specs/2026-11-07-conf-freeze-design.md`. This module is pure: it reads no clock, no
//! environment and no file, and it never touches the unit registry.

pub mod sha256;
