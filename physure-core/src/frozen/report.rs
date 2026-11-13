//! What the `phs conf` commands report (spec §5.4), as plain data: [`inspect`] says what is wrong with a frozen file
//! without refusing to look at it. Every function here is pure over the file model and the entries of
//! [`entries`](super::entries): nothing is written, no process-wide state changes, and nothing in a file's content can
//! make them panic (every problem becomes data). The CLI only renders these values and picks the exit code they give.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::engine::{environment_pairs, read_engine, EngineIdentity, EnginePolicy};
use super::entries::{diff, entries, summary, Change, Entry};
use super::load::{engine_file_hash, read_bounded, replay_file, replay_file_with, LoadError};
use super::model::{FrozenFile, HistoryEntry, Pairs};
use super::seal::Problem;
use crate::units::conf::{read_leniently_collected, replay_source, ProcessSettings, DEFAULT_PHYSURE_CONF};
use crate::units::UnitRegistry;

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
/// `phs conf update` shows the `[Engine]` pairs a plan changes with it (`plan.old.engine()` against `plan.file.engine()`).
pub fn pair_changes(old: &Pairs, new: &Pairs) -> Vec<FieldChange> {
    changes_of(old.as_slice(), new.as_slice())
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

/// One side of a comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    /// A frozen file on disk. It need not verify; if it does not, its problems are reported and its sources are not
    /// replayed (so the comparison has no data for it and is never identical).
    File(PathBuf),
    /// What the running build gives *unfrozen* in `cwd`: the embedded catalog plus `cwd/physure.conf` if any, read
    /// as the runtime reads it ([`unfrozen_entries`]: leniently, with warnings). The engine is the running build.
    Unfrozen { cwd: PathBuf },
}

/// The label of the unfrozen side.
pub const UNFROZEN_LABEL: &str = "the installed build, unfrozen";

/// The semantic difference of two sides: their catalogs, their engines and their environments.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The path of a file side, or [`UNFROZEN_LABEL`].
    pub a: String,
    pub b: String,
    /// The difference of the catalogs ([`diff`](super::entries::diff), old = a, new = b). Empty when a file side does
    /// not verify (it is not replayed).
    pub data: Vec<Change>,
    /// The `[Engine]` pairs that differ, in the key order of side a (then b). With an unfrozen side only the identity
    /// fields of `EngineIdentity::current()` are compared (version, commit, profile, target, features, rustc, flags):
    /// the running build has no policy and is not hashed.
    pub engine: Vec<FieldChange>,
    /// The `[Environment]` pairs that differ (the running machine's for an unfrozen side). Informational.
    pub environment: Vec<FieldChange>,
    /// Why side a (a file) cannot be trusted: its verification problems, or that its sources do not replay. Always
    /// empty for an unfrozen side.
    pub a_problems: Vec<String>,
    pub b_problems: Vec<String>,
    /// What the reading of an unfrozen side's `physure.conf` warns about (lines the runtime ignores, a strict reading
    /// that would refuse the file or give another catalog).
    pub warnings: Vec<String>,
}

impl Comparison {
    /// No difference in data, engine or environment, and both file sides verify and replay.
    pub fn is_identical(&self) -> bool {
        self.data.is_empty()
            && self.engine.is_empty()
            && self.environment.is_empty()
            && self.a_problems.is_empty()
            && self.b_problems.is_empty()
    }
}

/// A side, read.
struct SideRead {
    label: String,
    /// `None` when a file side was not replayed.
    entries: Option<Vec<Entry>>,
    /// `None` for the unfrozen side: the running build.
    engine: Option<Pairs>,
    environment: Pairs,
    problems: Vec<String>,
    warnings: Vec<String>,
}

fn read_side(side: &Side) -> Result<SideRead, String> {
    match side {
        Side::File(path) => {
            let (text, _) = read_bounded(path).map_err(|e| e.to_string())?;
            let file = FrozenFile::parse(&text)
                .map_err(|e| LoadError::NotFrozen { path: path.clone(), message: e.0 }.to_string())?;
            let mut problems: Vec<String> = file.verify().iter().map(ToString::to_string).collect();
            let mut entries_of_file = None;
            if problems.is_empty() {
                match replay_file(&file) {
                    Ok((registry, constants, collected)) => entries_of_file = Some(entries(&registry, &constants, &collected)),
                    Err((origin, found)) => problems.extend(found.iter().map(|p| format!("source {origin} does not replay: {p}"))),
                }
            }
            Ok(SideRead {
                label: path.display().to_string(),
                entries: entries_of_file,
                engine: Some(file.engine().clone()),
                environment: file.environment().clone(),
                problems,
                warnings: Vec::new(),
            })
        }
        Side::Unfrozen { cwd } => {
            let (found, warnings) = unfrozen_entries(cwd)?;
            Ok(SideRead {
                label: UNFROZEN_LABEL.to_string(),
                entries: Some(found),
                engine: None,
                environment: environment_pairs(),
                problems: Vec::new(),
                warnings,
            })
        }
    }
}

/// The identity fields of an engine block, which is all the running build has.
const IDENTITY_KEYS: [&str; 7] = ["version", "commit", "profile", "target", "features", "rustc", "flags"];

/// The identity of the running build as pairs.
fn running_identity() -> Vec<(String, String)> {
    let EngineIdentity { version, commit, profile, target, features, rustc, flags } = EngineIdentity::current();
    IDENTITY_KEYS.iter().map(|k| k.to_string()).zip([version, commit, profile, target, features, rustc, flags]).collect()
}

/// The `[Engine]` differences of two sides (see [`Comparison::engine`]).
fn engine_changes(a: Option<&Pairs>, b: Option<&Pairs>) -> Vec<FieldChange> {
    let identity_only = |pairs: &Pairs| -> Vec<(String, String)> {
        pairs.iter().filter(|(k, _)| IDENTITY_KEYS.contains(k)).map(|(k, v)| (k.to_string(), v.to_string())).collect()
    };
    let (old, new) = match (a, b) {
        (Some(a), Some(b)) => (a.as_slice().to_vec(), b.as_slice().to_vec()),
        (Some(a), None) => (identity_only(a), running_identity()),
        (None, Some(b)) => (running_identity(), identity_only(b)),
        (None, None) => return Vec::new(),
    };
    changes_of(&old, &new)
}

/// [`pair_changes`] over plain lists.
fn changes_of(old: &[(String, String)], new: &[(String, String)]) -> Vec<FieldChange> {
    let get = |pairs: &[(String, String)], key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let mut changes = Vec::new();
    for (key, value) in old {
        let other = get(new, key).unwrap_or_default();
        if *value != other {
            changes.push(FieldChange { key: key.clone(), old: value.clone(), new: other });
        }
    }
    for (key, value) in new {
        if get(old, key).is_none() {
            changes.push(FieldChange { key: key.clone(), old: String::new(), new: value.clone() });
        }
    }
    changes
}

/// The semantic difference of two sides (`phs conf diff`): catalogs, engines, environments. `Err` only when a side
/// cannot be read at all (a file that is missing, too large, not UTF-8 or not a frozen file; a `physure.conf` that is
/// not text); a file that does not verify is reported in the comparison. The engine file is not needed: no engine
/// check is made, the blocks are compared as written (kept in the signature for the CLI's symmetry with `inspect`).
pub fn compare(a: &Side, b: &Side, _engine_file: Option<&Path>) -> Result<Comparison, String> {
    let (a, b) = (read_side(a)?, read_side(b)?);
    let data = match (&a.entries, &b.entries) {
        (Some(old), Some(new)) => diff(old, new),
        _ => Vec::new(),
    };
    let engine = engine_changes(a.engine.as_ref(), b.engine.as_ref());
    let environment = pair_changes(&a.environment, &b.environment);
    let mut warnings = a.warnings;
    for warning in b.warnings {
        // the same directory on both sides warns once
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
    Ok(Comparison { a: a.label, b: b.label, data, engine, environment, a_problems: a.problems, b_problems: b.problems, warnings })
}

/// The catalog the running build gives *unfrozen* in `cwd`, as the runtime builds it: the embedded catalog, then
/// `cwd/physure.conf` if it is a file, both read leniently (a line that does not read is skipped), with the settings
/// collected, never applied. The warnings: each line the runtime ignores, and, when the strict reading that `phs conf
/// freeze` uses would refuse the file or build another catalog, that too. `Err` names a `physure.conf` that exists but
/// cannot be read as text (the runtime would silently use the embedded catalog alone).
pub fn unfrozen_entries(cwd: &Path) -> Result<(Vec<Entry>, Vec<String>), String> {
    let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
    let mut warnings: Vec<String> =
        read_leniently_collected(DEFAULT_PHYSURE_CONF, &mut registry, &mut constants, &mut collected, true)
            .into_iter()
            .map(|p| format!("the embedded catalog: {p}; the line is ignored"))
            .collect();
    let path = cwd.join("physure.conf");
    if !path.is_file() {
        return Ok((entries(&registry, &constants, &collected), warnings));
    }
    let shown = path.display();
    let text = fs::read_to_string(&path).map_err(|e| {
        format!("{shown}: expected a readable UTF-8 text, found: {e} (the runtime ignores such a file and uses the embedded catalog)")
    })?;
    let lenient_problems = read_leniently_collected(&text, &mut registry, &mut constants, &mut collected, false);
    warnings.extend(lenient_problems.iter().map(|p| format!("{shown}: {p}; the line is ignored")));
    let lenient = entries(&registry, &constants, &collected);
    let (mut strict_registry, mut strict_constants, mut strict_collected) =
        (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
    let strict = replay_source(&mut strict_registry, &mut strict_constants, &mut strict_collected, DEFAULT_PHYSURE_CONF, true)
        .and_then(|()| replay_source(&mut strict_registry, &mut strict_constants, &mut strict_collected, &text, false));
    match strict {
        Err(problems) => warnings.push(format!(
            "{shown}: the strict reading (what `phs conf freeze` records) refuses it: {}",
            problems.join("; ")
        )),
        Ok(()) => {
            let changes = diff(&lenient, &entries(&strict_registry, &strict_constants, &strict_collected));
            if !changes.is_empty() {
                warnings.push(format!(
                    "{shown}: the strict reading (what `phs conf freeze` records) builds another catalog: {}",
                    summary(&changes)
                ));
            }
        }
    }
    Ok((lenient, warnings))
}

/// The entries of the catalog a file's sources build, from memory (the replay of `load`): what `phs conf update` and
/// `seal` show the difference of, between a plan's `old` and its `file`, without reading the file again. Replays the
/// file as it is: the caller decides whether a file that does not verify may be replayed. `Err` is the origin of the
/// source that does not replay (or "the sources together") and its problems.
pub fn catalog_entries(file: &FrozenFile) -> Result<Vec<Entry>, (String, Vec<String>)> {
    replay_file(file).map(|(registry, constants, collected)| entries(&registry, &constants, &collected))
}

/// What the overrides (source 2) contribute to the catalog, from one replay of the file: the difference of the
/// catalog the sources build without them and with them (old = without), which is what `phs conf seal` shows. The
/// file keeps only the marker of the last sealed overrides, not their text, so a before/after of a hand edit cannot
/// be made from the file alone; what the edited overrides contribute now can.
///
/// Taken between the catalog after source 1 and after source 2 of the one replay, so for the files `phs conf freeze`
/// makes (two sources) it is exactly that difference; a further source (none is made today) is not attributed to the
/// overrides, and its redefinition of an entry the overrides set is not reflected. Replays the file as it is (the
/// caller decides whether it may be replayed); `Err` is the origin of the source that does not replay (or "the
/// sources together") and its problems, as for [`catalog_entries`].
pub fn overrides_contribution(file: &FrozenFile) -> Result<Vec<Change>, (String, Vec<String>)> {
    if file.sources().len() < 2 {
        return Err(("overrides".to_string(), vec!["the file has no source 2 (overrides)".to_string()]));
    }
    let (mut without, mut with) = (Vec::new(), Vec::new());
    replay_file_with(file, |index, registry, constants, collected| match index {
        0 => without = entries(registry, constants, collected),
        1 => with = entries(registry, constants, collected),
        _ => {}
    })?;
    Ok(diff(&without, &with))
}

/// One entry of a file's history, as `phs conf history` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub n: u32,
    pub time: String,
    pub action: String,
    pub actor: String,
    pub reason: String,
    /// The engine that acted (informational, as recorded).
    pub engine: String,
    /// The policy and the pinned engine of the `[Engine]` block this entry sealed.
    pub policy: String,
    pub pinned: String,
    pub diff: String,
    /// `engine_sha256` differs from the previous entry's (the first entry: false). Residual risk 1b-12 of §13: a moved
    /// `binary_sha256` or a changed profile/target/features/rustc/flags shows only as this, since the history keeps the
    /// hash of the block, not the block.
    pub engine_block_changed: bool,
    /// `policy` or `pinned` differs from the previous entry's, in words (`policy: release -> record`, then
    /// `pinned: a -> b`, joined by `; `), else empty.
    pub pin_change: String,
    /// The file's own checks do not vouch for this row: it is at or after the first entry whose chain link fails
    /// (its number, its link to the previous entry, its own hash, its action, its continuity, its reason, its time),
    /// or the stored seal does not cover the stored parts (then every row). The row shows what the file says.
    pub unverified: bool,
}

/// The first history entry (by number) whose link of the chain fails, if any.
fn first_broken_link(file: &FrozenFile) -> Option<u32> {
    file.verify()
        .iter()
        .filter_map(|problem| match problem {
            Problem::HistoryNumber { position, .. } => u32::try_from(*position).ok(),
            Problem::HistoryPrev { n, .. }
            | Problem::HistoryEntryHash { n, .. }
            | Problem::HistoryAction { n, .. }
            | Problem::HistoryContinuity { n, .. }
            | Problem::HistoryReason { n, .. }
            | Problem::HistoryTime { n, .. } => Some(*n),
            _ => None,
        })
        .min()
}

/// The history of `file` as rows, oldest first, whether the file verifies or not (see [`HistoryRow::unverified`]).
pub fn history_rows(file: &FrozenFile) -> Vec<HistoryRow> {
    let sealed = file.seal_covers_the_stored_parts();
    let broken = first_broken_link(file);
    let mut rows = Vec::with_capacity(file.history().len());
    let mut previous: Option<&HistoryEntry> = None;
    for (position, entry) in file.history().iter().enumerate() {
        let mut pin_change = Vec::new();
        if let Some(before) = previous {
            for (what, old, new) in [("policy", before.policy(), entry.policy()), ("pinned", before.pinned(), entry.pinned())] {
                if old != new {
                    pin_change.push(format!("{what}: {old} -> {new}"));
                }
            }
        }
        // by position, which a misnumbered entry cannot move; the numbers of the problems are the entries' own, and a
        // misnumbered entry is reported at its position too, so the first broken link is never placed too late
        let n = u32::try_from(position + 1).unwrap_or(u32::MAX);
        rows.push(HistoryRow {
            n: entry.n(),
            time: entry.time().to_string(),
            action: entry.action().to_string(),
            actor: entry.actor().to_string(),
            reason: entry.reason().to_string(),
            engine: entry.engine().to_string(),
            policy: entry.policy().to_string(),
            pinned: entry.pinned().to_string(),
            diff: entry.diff().to_string(),
            engine_block_changed: previous.is_some_and(|before| before.engine_sha256() != entry.engine_sha256()),
            pin_change: pin_change.join("; "),
            unverified: !sealed || broken.is_some_and(|first| n >= first),
        });
        previous = Some(entry);
    }
    rows
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
    fn a_file_without_overrides_has_no_contribution_to_show() {
        use crate::frozen::engine::engine_pairs;
        use crate::frozen::model::Source;
        use crate::frozen::seal::{EntryInput, FrozenParts};
        let mut file = FrozenFile::new(
            "2026-11-08T10:00:00Z",
            FrozenParts {
                engine: engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap(),
                sources: vec![
                    Source::new("embedded", crate::units::conf::DEFAULT_PHYSURE_CONF).unwrap(),
                    Source::new("overrides", "").unwrap(),
                ],
                ..FrozenParts::default()
            },
            EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "t", actor: "t@h", reason: "t", diff: "d" },
        )
        .unwrap();
        assert!(overrides_contribution(&file).unwrap().is_empty());
        file.sources.truncate(1);
        let (source, problems) = overrides_contribution(&file).unwrap_err();
        assert_eq!(source, "overrides");
        assert!(problems[0].contains("no source 2"), "{problems:?}");
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
