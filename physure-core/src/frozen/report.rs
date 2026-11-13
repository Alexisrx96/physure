//! What the `phs conf` commands report (spec §5.4), as plain data: [`inspect`] says what is wrong with a frozen file
//! without refusing to look at it. Every function here is pure over the file model and the entries of
//! [`entries`](super::entries): nothing is written, no process-wide state changes, and nothing in a file's content can
//! make them panic (every problem becomes data). The CLI only renders these values and picks the exit code they give.

use std::path::{Path, PathBuf};

use super::engine::{environment_pairs, read_engine, EngineIdentity, EnginePolicy};
use super::entries::{entries, Entry};
use super::load::{engine_file_hash, read_bounded, replay_file, LoadError};
use super::model::{FrozenFile, Pairs};

/// How the file's engine pin compares with the running build.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineState {
    /// The engine block could not be read (`read_engine` failed): the message.
    Unreadable(String),
    /// The running build satisfies the pin under the file's policy; `pinned` is the pinned engine, short form.
    Matches { policy: EnginePolicy, pinned: String },
    /// It does not: the policy and every difference (`field: recorded X, running Y`), in the order `check` gives them.
    Differs { policy: EnginePolicy, differences: Vec<String> },
}

/// One field that differs between two sides, or between a file and the running machine. A key present on one side
/// only has the empty string on the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub key: String,
    pub old: String,
    pub new: String,
}

/// Everything that can be said about one frozen file, sound or not.
#[derive(Debug, Clone)]
pub struct Inspection {
    pub path: PathBuf,
    /// The text could not be read or parsed (size limit, I/O, not UTF-8, not a frozen file): the message. Everything
    /// below is then empty.
    pub unreadable: Option<String>,
    pub file: Option<FrozenFile>,
    /// `FrozenFile::verify()`, one problem per string. Empty: the seal and the chain hold.
    pub problems: Vec<String>,
    pub engine: Option<EngineState>,
    /// `[Environment]` of the file (`old`) against `environment_pairs()` of this machine (`new`): informational, outside
    /// the seal, never part of the exit code.
    pub environment: Vec<FieldChange>,
    /// The sources replayed strictly: the entries of the catalog, or the problems (`origin: problem`). `None` when the
    /// file was not replayed: it is unreadable or **does not verify** (the sources of a file that does not verify are
    /// never run).
    pub catalog: Option<Result<Vec<Entry>, Vec<String>>>,
    /// `UnitRegistry::atomic_weight_edition()` of the replayed registry.
    pub atomic_weight_edition: Option<String>,
}

/// Inspects the frozen file at `path` and never fails: every problem is in the result. `engine_file` is the file that
/// contains the running engine, hashed only under the policy `binary`; `None` there is reported as a difference.
pub fn inspect(path: &Path, engine_file: Option<&Path>) -> Inspection {
    let mut inspection = Inspection {
        path: path.to_path_buf(),
        unreadable: None,
        file: None,
        problems: Vec::new(),
        engine: None,
        environment: Vec::new(),
        catalog: None,
        atomic_weight_edition: None,
    };
    let parsed = read_bounded(path).and_then(|(text, _)| {
        FrozenFile::parse(&text).map_err(|e| LoadError::NotFrozen { path: path.to_path_buf(), message: e.0 })
    });
    let file = match parsed {
        Ok(file) => file,
        Err(e) => {
            inspection.unreadable = Some(e.to_string());
            return inspection;
        }
    };
    inspection.problems = file.verify().iter().map(ToString::to_string).collect();
    inspection.engine = Some(engine_state(file.engine(), engine_file));
    inspection.environment = pair_changes(file.environment(), &environment_pairs());
    if inspection.problems.is_empty() {
        match replay_file(&file) {
            Ok((registry, constants, collected)) => {
                inspection.atomic_weight_edition = Some(registry.atomic_weight_edition());
                inspection.catalog = Some(Ok(entries(&registry, &constants, &collected)));
            }
            Err((origin, problems)) => {
                inspection.catalog = Some(Err(problems.iter().map(|p| format!("{origin}: {p}")).collect()));
            }
        }
    }
    inspection.file = Some(file);
    inspection
}

/// The state of the engine block against the running build.
fn engine_state(block: &Pairs, engine_file: Option<&Path>) -> EngineState {
    let block = match read_engine(block) {
        Ok(block) => block,
        Err(e) => return EngineState::Unreadable(e.0),
    };
    match block.check(&EngineIdentity::current(), || engine_file_hash(engine_file)) {
        Ok(()) => EngineState::Matches { policy: block.policy, pinned: block.identity.short() },
        Err(mismatch) => EngineState::Differs {
            policy: mismatch.policy,
            differences: mismatch
                .differences
                .iter()
                .map(|d| format!("{}: recorded {}, running {}", d.field, d.recorded, d.running))
                .collect(),
        },
    }
}

/// The keys whose values differ between `old` and `new`: the keys of `old` in its order, then those of `new` only.
pub(crate) fn pair_changes(old: &Pairs, new: &Pairs) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    for (key, value) in old.iter() {
        let other = new.get(key).unwrap_or("");
        if value != other {
            changes.push(FieldChange { key: key.to_string(), old: value.to_string(), new: other.to_string() });
        }
    }
    for (key, value) in new.iter() {
        if old.get(key).is_none() {
            changes.push(FieldChange { key: key.to_string(), old: String::new(), new: value.to_string() });
        }
    }
    changes
}

impl Inspection {
    /// The exit code of decision 1 of the Phase 2 plan for *a file on its own*: 0 when sound; 3 when the only
    /// discrepancy is the engine; 1 otherwise (unreadable, a verification problem, an unreadable engine block, a
    /// catalog that does not replay). Environment differences never count.
    pub fn exit_code(&self) -> i32 {
        if self.unreadable.is_some() || !self.problems.is_empty() {
            return 1;
        }
        match (&self.engine, &self.catalog) {
            (Some(EngineState::Unreadable(_)) | None, _) | (_, Some(Err(_)) | None) => 1,
            (Some(EngineState::Differs { .. }), _) => 3,
            (Some(EngineState::Matches { .. }), Some(Ok(_))) => 0,
        }
    }

    /// True only when the file verifies (no problem at all) **and** its stored seal equals `expected`: 64 hexadecimal
    /// characters, compared case-insensitively. The stored seal of a hand-edited file is still the old one, so a
    /// comparison of the seal alone would vouch for content it does not cover.
    pub fn seal_matches(&self, expected: &str) -> bool {
        let well_formed = expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit());
        well_formed
            && self.unreadable.is_none()
            && self.problems.is_empty()
            && self.file.as_ref().is_some_and(|file| file.seal_sha256().eq_ignore_ascii_case(expected))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, &str)]) -> Pairs {
        Pairs::new(items).unwrap()
    }

    #[test]
    fn pair_changes_keep_the_order_of_the_first_side_and_mark_one_sided_keys() {
        let old = pairs(&[("os", "linux"), ("cpus", "4"), ("gone", "x")]);
        let new = pairs(&[("os", "linux"), ("added", "y"), ("cpus", "8")]);
        let change = |key: &str, old: &str, new: &str| FieldChange { key: key.into(), old: old.into(), new: new.into() };
        assert_eq!(
            pair_changes(&old, &new),
            vec![change("cpus", "4", "8"), change("gone", "x", ""), change("added", "", "y")]
        );
        assert!(pair_changes(&old, &old).is_empty());
    }

    #[test]
    fn an_inspection_without_an_engine_or_a_catalog_is_never_sound() {
        let empty = Inspection {
            path: PathBuf::from("x"),
            unreadable: None,
            file: None,
            problems: Vec::new(),
            engine: None,
            environment: Vec::new(),
            catalog: None,
            atomic_weight_edition: None,
        };
        assert_eq!(empty.exit_code(), 1);
        let matches = Some(EngineState::Matches { policy: EnginePolicy::Record, pinned: "p".into() });
        assert_eq!(Inspection { engine: matches.clone(), ..empty.clone() }.exit_code(), 1, "no catalog");
        assert_eq!(Inspection { engine: matches.clone(), catalog: Some(Ok(vec![])), ..empty.clone() }.exit_code(), 0);
        assert_eq!(Inspection { engine: matches, catalog: Some(Err(vec!["p".into()])), ..empty.clone() }.exit_code(), 1);
        let differs = Some(EngineState::Differs { policy: EnginePolicy::Release, differences: vec!["v".into()] });
        assert_eq!(Inspection { engine: differs.clone(), catalog: Some(Ok(vec![])), ..empty.clone() }.exit_code(), 3);
        assert_eq!(Inspection { engine: differs, catalog: Some(Err(vec![])), ..empty }.exit_code(), 1, "engine and catalog");
    }
}
