//! The two steps of `freeze`, `update` and `seal` when witnesses are evaluated (Phase 3 plan, corrections C1, C2,
//! C7 and C11): `prepare_*` reads the file **once** and builds the new file without its witnesses (a *draft*);
//! [`Draft::probe`] is a sealed, validated throwaway copy of it that a host loads as the explicit configuration to
//! evaluate the witnesses under the file it is about to write (never under the unfrozen catalog); `finish` sets
//! `[Witness]` and seals **once**, so the history gains exactly one entry, whose `diff` line already holds the
//! witness part. The one-step [`freeze`], [`plan_update`] and [`plan_seal`] are these steps finished with the
//! recorded witnesses unchanged (the behaviour of Phase 2).
//!
//! The core decides nothing about witness policy: the caller compares (`witness::compare` against
//! `draft.old.witnesses()`), decides whether a change may be accepted, and passes the re-recorded witnesses
//! ([`rerecord`]) or `None` to `finish`.
//!
//! # The order a host follows
//!
//! * **freeze:** [`prepare_freeze`] (refuses `AlreadyFrozen`, `WouldOverwrite`, an engine policy this build cannot
//!   record and overrides that do not replay, all before anything is evaluated); write [`FreezeDraft::probe`] to a
//!   throwaway file, set it as the explicit configuration, reset the cache, evaluate the battery and the user scripts
//!   (`WitnessScript::new(name, raw)?.text()`, decision C13), restore the configuration; [`FreezeDraft::finish`] with
//!   the rows and the scripts (an `Err` row refuses, naming the case); write the file (`write_atomic`, no replace).
//! * **update:** [`prepare_update`] (`None`: nothing to do, the witnesses are not consulted, decision S9); evaluate
//!   under [`Draft::probe`] as for freeze; `witness::compare(draft.old.witnesses(), &rows)`; the host's policy
//!   (`--accept-witness-changes`) decides; [`Draft::finish`] with `None` when nothing is re-recorded or with
//!   `Some(&rerecord(draft.old.witnesses(), &rows)?)`; before writing, check both the stamp (`FileStamp`) and
//!   [`bytes_unchanged_since`] with [`Plan::read_sha256`].
//! * **seal:** [`prepare_seal`] (it returns a draft only after the positive proof that only source 2 changed); then
//!   the host's **engine check of `draft.old`** (the probe carries the same `[Engine]` block, so a probe of a file for
//!   another engine would not load: the check comes first, exit 3, before anything is evaluated); then the probe,
//!   the comparison and `finish` as for update.

use std::io::Read;

use super::*;
use crate::frozen::canonical::canonical_pairs;
use crate::frozen::load::MAX_FILE_BYTES;
use crate::frozen::model::WitnessScript;
use crate::frozen::sha256::sha256_hex;
use crate::frozen::witness::{compare, format_value, WitnessStatus, WitnessValue};

/// At most this many keys are named per kind in the history `diff` line; the rest are counted (`+N more`).
pub const DRIFT_NAMES: usize = 10;

/// What re-recording the witnesses changes, against the witnesses the file recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessDrift {
    /// Recorded keys whose running value differs, in recorded order.
    pub changed: Vec<String>,
    /// Running keys the file did not record, in running order.
    pub new: Vec<String>,
    /// Recorded keys the running rows do not have (they are dropped), in recorded order.
    pub missing: Vec<String>,
    /// The SHA-256 (lowercase hex) of the old `[Witness]` block, hashed canonically (as the seal hashes it).
    pub old_block_sha256: String,
}

impl WitnessDrift {
    /// Nothing changed, nothing is new, nothing is missing.
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.new.is_empty() && self.missing.is_empty()
    }

    /// The witness part of the history `diff` line: `witnesses: N changed, M new, K missing`, the changed and the
    /// missing keys (the first [`DRIFT_NAMES`] of each, then `+N more`), and the first 12 hex digits of
    /// [`old_block_sha256`](Self::old_block_sha256).
    pub fn line(&self) -> String {
        fn names(keys: &[String]) -> String {
            let mut text = keys.iter().take(DRIFT_NAMES).map(String::as_str).collect::<Vec<_>>().join(", ");
            if keys.len() > DRIFT_NAMES {
                text.push_str(&format!(" +{} more", keys.len() - DRIFT_NAMES));
            }
            text
        }
        let mut line = format!("witnesses: {} changed, {} new, {} missing", self.changed.len(), self.new.len(), self.missing.len());
        for (what, keys) in [("changed", &self.changed), ("missing", &self.missing)] {
            if !keys.is_empty() {
                line.push_str(&format!("; {what}: {}", names(keys)));
            }
        }
        line.push_str(&format!("; old [Witness] sha256 {}", &self.old_block_sha256[..12.min(self.old_block_sha256.len())]));
        line
    }
}

/// Witnesses ready to be recorded: the new `[Witness]` pairs and what they change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rerecorded {
    pub witnesses: Pairs,
    pub drift: WitnessDrift,
}

/// The new `[Witness]` block from freshly computed rows (all of them re-recorded, in the order given: the battery in
/// id order, then the user witnesses in file order, decision S2; a `user.*` row is the caller's
/// `Ok(WitnessValue::Hash(..))`), and its drift against `recorded`. Scripts are not touched: a file keeps its own.
/// A `WitnessValue::Failed` row is a value (decision S6); an `Err` row (a panic, a limit) cannot be recorded and
/// refuses, naming the case.
pub fn rerecord(recorded: &Pairs, running: &[(String, Result<WitnessValue, String>)]) -> Result<Rerecorded, OpsError> {
    let witnesses = record_rows(running)?;
    let mut drift = WitnessDrift {
        changed: Vec::new(),
        new: Vec::new(),
        missing: Vec::new(),
        old_block_sha256: sha256_hex(canonical_pairs(recorded.as_slice()).as_bytes()),
    };
    for row in compare(recorded, running).rows {
        match row.status {
            WitnessStatus::Match => {}
            WitnessStatus::Differs { .. } => drift.changed.push(row.key),
            WitnessStatus::New => drift.new.push(row.key),
            WitnessStatus::Missing => drift.missing.push(row.key),
            // a recorded entry that is not a witness (not in a verified file): replaced, or dropped
            WitnessStatus::Error(_) if witnesses.get(&row.key).is_some() => drift.changed.push(row.key),
            WitnessStatus::Error(_) => drift.missing.push(row.key),
        }
    }
    Ok(Rerecorded { witnesses, drift })
}

/// The `[Witness]` pairs of `running`, in its order; refused when a row is an `Err`, naming every such case.
fn record_rows(running: &[(String, Result<WitnessValue, String>)]) -> Result<Pairs, OpsError> {
    let errors: Vec<String> =
        running.iter().filter_map(|(key, row)| row.as_ref().err().map(|e| format!("{key}: {e}"))).collect();
    if !errors.is_empty() {
        return Err(OpsError::Refused(format!(
            "{} witness(es) could not be evaluated, so nothing can be recorded (expected a value or a recorded failure for each): {}",
            errors.len(),
            errors.join("; ")
        )));
    }
    let pairs = running.iter().filter_map(|(key, row)| row.as_ref().ok().map(|value| (key.clone(), format_value(value))));
    Ok(Pairs::from_vec(pairs.collect())?)
}

/// Whether the bytes at `path` are still the ones whose SHA-256 (lowercase hex) is `sha256`: the compare-and-swap
/// of a write (correction C11), next to the length and modification-time check of [`FileStamp`], which an edit of
/// the same length that keeps the modification time does not see. `false` when the file cannot be read.
pub fn bytes_unchanged_since(path: &Path, sha256: &str) -> bool {
    let Ok(file) = File::open(path) else { return false };
    let mut bytes = Vec::new();
    if file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).is_err() {
        return false;
    }
    sha256_hex(&bytes) == sha256
}

/// The first step of `freeze`: the new file without witnesses, validated, and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct FreezeDraft {
    /// Where the file is meant to be written.
    pub out: PathBuf,
    probe: FrozenFile,
    time: String,
    actor: String,
    reason: String,
    engine: String,
    diff: String,
}

impl FreezeDraft {
    /// The new file without witnesses, sealed and validated: to be loaded as the explicit configuration while the
    /// witnesses are evaluated, never written as the result.
    pub fn probe(&self) -> &FrozenFile {
        &self.probe
    }

    /// The new file with `[Witness]` = `running` (keys `builtin.<id>` and `user.<name>`, in the order given) and the
    /// user `scripts`; with no rows and no scripts, the probe itself. One history entry, the freeze; with witnesses,
    /// its `diff` ends with `; witnesses: N built-in, M user`. Refused when a row is an `Err` (naming the case), when
    /// the `user.*` keys do not name the scripts one each, and when a value is not a witness value.
    pub fn finish(
        &self,
        running: &[(String, Result<WitnessValue, String>)],
        scripts: Vec<WitnessScript>,
    ) -> Result<(FrozenFile, PathBuf), OpsError> {
        if running.is_empty() && scripts.is_empty() {
            return Ok((self.probe.clone(), self.out.clone()));
        }
        let witnesses = record_rows(running)?;
        let builtin = witnesses.iter().filter(|(key, _)| key.starts_with("builtin.")).count();
        let diff = format!("{}; witnesses: {builtin} built-in, {} user", self.diff, witnesses.len() - builtin);
        let probe = &self.probe;
        let file = FrozenFile::new(
            &self.time,
            FrozenParts {
                engine: probe.engine().clone(),
                environment: probe.environment().clone(),
                witnesses,
                sources: probe.sources().to_vec(),
                witness_scripts: scripts,
            },
            EntryInput { time: &self.time, action: "freeze", engine: &self.engine, actor: &self.actor, reason: &self.reason, diff: &diff },
        )?;
        validate_text(&file)?;
        Ok((file, self.out.clone()))
    }
}

/// The first step of `freeze`, every refusal of [`freeze`] included, before anything is evaluated.
pub fn prepare_freeze(options: &FreezeOptions<'_>) -> Result<FreezeDraft, OpsError> {
    let conf_var = match std::env::var(ENV_CONF) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => return Err(OpsError::Resolve(ResolveError::NotUnicode(ENV_CONF))),
    };
    let inputs = Inputs { explicit: None, conf_var: conf_var.as_deref(), require_var: None, cwd: options.cwd };
    if let Resolution::Frozen { path, .. } = resolve(&inputs).map_err(OpsError::Resolve)? {
        return Err(OpsError::AlreadyFrozen(path));
    }
    let out = options.cwd.join(options.out.clone().unwrap_or_else(|| PathBuf::from(FROZEN_FILE_NAME)));
    if out.exists() {
        return Err(OpsError::WouldOverwrite(out));
    }
    let local = options.cwd.join("physure.conf");
    let overrides = if local.is_file() { fs::read_to_string(&local).map_err(io_error(&local))? } else { EMPTY_OVERRIDES.to_string() };
    let engine = running_engine(options.policy, options.engine_file)?;
    let time = time_text(options.time_unix)?;
    let sources = vec![Source::new("embedded", DEFAULT_PHYSURE_CONF)?, Source::new("overrides", &overrides)?];
    // the overrides are checked before the file is made, so the problems name the user's own text
    let (embedded_lines, overrides_lines) = (canonical_line_count(DEFAULT_PHYSURE_CONF), canonical_line_count(&overrides));
    let diff = format!("embedded {embedded_lines} lines; overrides {overrides_lines} lines");
    let short = EngineIdentity::current().short();
    let probe = FrozenFile::new(
        &time,
        FrozenParts { engine, environment: environment_pairs(), sources, ..FrozenParts::default() },
        EntryInput { time: &time, action: "freeze", engine: &short, actor: options.actor, reason: options.reason, diff: &diff },
    )?;
    validate(&probe)?;
    Ok(FreezeDraft { out, probe, time, actor: options.actor.to_string(), reason: options.reason.to_string(), engine: short, diff })
}

/// A planned change of a frozen file, sealed: what `update` and `seal` write.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub file: FrozenFile,
    /// The history `diff` line of the new entry, witness part included.
    pub summary: String,
    /// The file as it was read, so the difference the plan makes can be shown without reading it again.
    pub old: FrozenFile,
    /// The stamp of the file, from the handle `old` was read through (residual risk 1c-14).
    pub stamp: FileStamp,
    /// The SHA-256 (lowercase hex) of the bytes `old` was parsed from: [`bytes_unchanged_since`] compares it with
    /// the bytes on disk just before writing.
    pub read_sha256: String,
    /// The witnesses re-recorded, or `None` when they were kept as recorded.
    pub witnesses: Option<WitnessDrift>,
}

/// The first step of `update` and `seal`: the file read once, the new file without its new witnesses.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// The file as it was read.
    pub old: FrozenFile,
    pub stamp: FileStamp,
    /// The SHA-256 (lowercase hex) of the bytes read.
    pub read_sha256: String,
    /// The history `diff` line without the witness part.
    pub summary: String,
    next: FrozenFile,
    probe: FrozenFile,
    action: &'static str,
    time: String,
    actor: String,
    reason: String,
    engine: String,
}

impl Draft {
    /// The new file before any reseal (it does not verify).
    pub fn next(&self) -> &FrozenFile {
        &self.next
    }

    /// The new file sealed with the recorded witnesses and validated: to be loaded as the explicit configuration
    /// while the witnesses are evaluated, never written as the result (it is also exactly what `finish(None)` returns).
    pub fn probe(&self) -> &FrozenFile {
        &self.probe
    }

    /// The plan: with `None` the recorded witnesses are kept and the result is the probe (Phase 2 behaviour, byte
    /// for byte); with `Some`, `[Witness]` becomes `rerecorded.witnesses` and the file is sealed once, the history
    /// `diff` being the summary followed by `; ` and [`WitnessDrift::line`].
    pub fn finish(&self, rerecorded: Option<&Rerecorded>) -> Result<Plan, OpsError> {
        let (file, summary) = match rerecorded {
            None => (self.probe.clone(), self.summary.clone()),
            Some(rerecorded) => {
                let summary = format!("{}; {}", self.summary, rerecorded.drift.line());
                let mut file = self.next.clone();
                file.set_witnesses(rerecorded.witnesses.clone());
                file.reseal(self.entry(&summary))?;
                validate_text(&file)?;
                (file, summary)
            }
        };
        Ok(Plan {
            file,
            summary,
            old: self.old.clone(),
            stamp: self.stamp,
            read_sha256: self.read_sha256.clone(),
            witnesses: rerecorded.map(|r| r.drift.clone()),
        })
    }

    fn entry<'a>(&'a self, diff: &'a str) -> EntryInput<'a> {
        EntryInput { time: &self.time, action: self.action, engine: &self.engine, actor: &self.actor, reason: &self.reason, diff }
    }

    /// The draft of `next` (made from `read`: the file as read, its stamp and the hash of its bytes) whose history
    /// `diff` is `summary`; the probe is `next` sealed and validated.
    fn new(
        action: &'static str,
        read: (FrozenFile, FileStamp, String),
        next: FrozenFile,
        summary: String,
        entry: (u64, &str, &str),
    ) -> Result<Draft, OpsError> {
        let ((old, stamp, read_sha256), (time_unix, actor, reason)) = (read, entry);
        let mut draft = Draft {
            old,
            stamp,
            read_sha256,
            summary,
            probe: next.clone(),
            next,
            action,
            time: time_text(time_unix)?,
            actor: actor.to_string(),
            reason: reason.to_string(),
            engine: EngineIdentity::current().short(),
        };
        let mut probe = draft.next.clone();
        probe.reseal(draft.entry(&draft.summary))?;
        validate(&probe)?;
        draft.probe = probe;
        Ok(draft)
    }
}

/// The first step of [`plan_update`]; `Ok(None)` when there is nothing to update.
pub fn prepare_update(
    path: &Path,
    reason: &str,
    policy: Option<EnginePolicy>,
    engine_file: Option<&Path>,
    time_unix: u64,
    actor: &str,
) -> Result<Option<Draft>, OpsError> {
    let (mut file, stamp, read_sha256) = read_frozen(path)?;
    let old = file.clone();
    let problems = file.verify();
    if !problems.is_empty() {
        return Err(OpsError::Refused(format!(
            "{}: the file was modified (expected it to verify): seal it or restore it first: {}",
            path.display(),
            listed(&problems)
        )));
    }
    // verify read the block; not assumed
    let old_block = read_engine(file.engine())?;
    let new_policy = policy.unwrap_or(old_block.policy);
    let new_engine = running_engine(new_policy, engine_file)?;
    let old_embedded = file.sources()[0].text().to_string();
    let catalog_changed = source_sha256(&old_embedded) != source_sha256(DEFAULT_PHYSURE_CONF);
    let engine_changed = canonical_pairs(file.engine().as_slice()) != canonical_pairs(new_engine.as_slice());
    if !catalog_changed && !engine_changed {
        return Ok(None);
    }
    if catalog_changed {
        file.set_source_text(0, DEFAULT_PHYSURE_CONF)?;
    }
    file.set_engine(new_engine);
    let (added, removed) = line_changes(&old_embedded, DEFAULT_PHYSURE_CONF);
    let summary = format!(
        "embedded: +{added} -{removed} canonical lines; engine: {}; policy: {} -> {}",
        if engine_changed { "changed" } else { "unchanged" },
        old_block.policy.as_str(),
        new_policy.as_str()
    );
    Draft::new("update", (old, stamp, read_sha256), file, summary, (time_unix, actor, reason)).map(Some)
}

/// The first step of [`plan_seal`]; `Ok(None)` when the file verifies. A draft exists only after the proof that
/// only the overrides changed.
pub fn prepare_seal(path: &Path, reason: &str, time_unix: u64, actor: &str) -> Result<Option<Draft>, OpsError> {
    let (file, stamp, read_sha256) = read_frozen(path)?;
    let problems = file.verify();
    if problems.is_empty() {
        return Ok(None);
    }
    if !file.only_the_overrides_changed() {
        return Err(OpsError::Refused(format!(
            "{}: seal accepts a hand edit of the overrides block only (expected every other part as last sealed), found: {}",
            path.display(),
            listed(&problems)
        )));
    }
    let overrides = &file.sources()[1];
    let (old, new) = (overrides.sha256().to_string(), source_sha256(overrides.text()));
    let summary = format!("overrides edited by hand: {} -> {}", &old[..12.min(old.len())], &new[..12]);
    Draft::new("seal", (file.clone(), stamp, read_sha256), file, summary, (time_unix, actor, reason)).map(Some)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fake_identity, frozen_in, older_embedded, options, Dir, T0};
    use super::*;
    use crate::frozen::load::{load_file, replay_file, LoadOptions};

    fn num(x: f64) -> WitnessValue {
        WitnessValue::number(x, 0.0, "m")
    }

    fn rows(items: &[(&str, Result<WitnessValue, &str>)]) -> Vec<(String, Result<WitnessValue, String>)> {
        items.iter().map(|(k, v)| (k.to_string(), v.clone().map_err(str::to_string))).collect()
    }

    /// A file an older library froze, with witnesses.
    fn old_witnessed(witnesses: &[(&str, WitnessValue)]) -> FrozenFile {
        let pairs: Vec<(String, String)> = witnesses.iter().map(|(k, v)| (k.to_string(), format_value(v))).collect();
        FrozenFile::new(
            "2026-11-01T10:00:00Z",
            FrozenParts {
                engine: engine_pairs(&fake_identity(), EnginePolicy::Release, None).unwrap(),
                witnesses: Pairs::from_vec(pairs).unwrap(),
                sources: vec![
                    Source::new("embedded", &older_embedded()).unwrap(),
                    Source::new("overrides", "[Units]\nzz_kept = 2.0, L, [zz_kept]").unwrap(),
                ],
                ..FrozenParts::default()
            },
            EntryInput { time: "2026-11-01T10:00:00Z", action: "freeze", engine: "0.0.1+0123456789ab", actor: "a@b", reason: "old", diff: "d" },
        )
        .unwrap()
    }

    /// Everything but `[Witness]` and the last history entry is the same; the last entries differ only in the diff.
    fn same_but_witnesses(probe: &FrozenFile, file: &FrozenFile) {
        assert_eq!(file.sources(), probe.sources());
        assert_eq!(file.engine(), probe.engine());
        assert_eq!(file.environment(), probe.environment());
        let (a, b) = (replay_file(probe).unwrap(), replay_file(file).unwrap());
        assert!(a.0 == b.0, "the replayed registries differ");
        assert_eq!((a.1, a.2), (b.1, b.2), "the replayed constants or settings differ");
        let (p, f) = (probe.history(), file.history());
        assert_eq!(p.len(), f.len(), "one entry each");
        let last = f.len() - 1;
        assert_eq!(p[..last], f[..last]);
        let (p, f) = (&p[last], &f[last]);
        assert_eq!(
            (p.time(), p.action(), p.engine(), p.engine_sha256(), p.policy(), p.pinned(), p.actor(), p.reason()),
            (f.time(), f.action(), f.engine(), f.engine_sha256(), f.policy(), f.pinned(), f.actor(), f.reason())
        );
        assert_eq!((p.old_content_sha256(), p.new_content_sha256()), (f.old_content_sha256(), f.new_content_sha256()));
        assert!(file.verify().is_empty(), "{:?}", file.verify());
    }

    #[test]
    fn update_probe_and_plan_differ_only_in_the_witnesses_and_the_last_entry() {
        let dir = Dir::new();
        let old = old_witnessed(&[("builtin.a", num(1.0)), ("builtin.c", num(3.0)), ("builtin.gone", num(9.0))]);
        let path = dir.write(FROZEN_FILE_NAME, &old.render().unwrap());
        let draft = prepare_update(&path, "new library", None, None, T0, "t@h").unwrap().unwrap();
        assert_eq!(draft.old, old);
        assert_eq!(draft.read_sha256, sha256_hex(&fs::read(&path).unwrap()));
        let probe = draft.probe().clone();
        assert_eq!(probe.witnesses(), old.witnesses(), "the probe keeps the recorded witnesses");
        assert!(probe.verify().is_empty());
        assert_eq!(probe.history().last().unwrap().diff(), draft.summary);
        let running = rows(&[
            ("builtin.a", Ok(num(1.5))),
            ("builtin.c", Ok(num(3.0))),
            ("builtin.d", Ok(WitnessValue::failed("no Ca in this table\nsecond line"))),
        ]);
        let rerecorded = rerecord(draft.old.witnesses(), &running).unwrap();
        assert_eq!(rerecorded.drift.changed, ["builtin.a"]);
        assert_eq!(rerecorded.drift.new, ["builtin.d"]);
        assert_eq!(rerecorded.drift.missing, ["builtin.gone"]);
        let keys: Vec<&str> = rerecorded.witnesses.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["builtin.a", "builtin.c", "builtin.d"]);
        assert_eq!(rerecorded.witnesses.get("builtin.d"), Some("failed=no Ca in this table"), "a failure is a value");
        let plan = draft.finish(Some(&rerecorded)).unwrap();
        same_but_witnesses(&probe, &plan.file);
        assert_eq!(plan.file.witnesses(), &rerecorded.witnesses);
        assert_eq!(plan.file.history().len(), old.history().len() + 1, "exactly one entry");
        let diff = plan.file.history().last().unwrap().diff();
        assert_eq!(diff, format!("{}; {}", draft.summary, rerecorded.drift.line()));
        assert_eq!(plan.summary, diff);
        assert!(diff.contains("witnesses: 1 changed, 1 new, 1 missing; changed: builtin.a; missing: builtin.gone"), "{diff}");
        assert_eq!(plan.witnesses, Some(rerecorded.drift.clone()));
        assert_eq!((plan.old, plan.stamp, plan.read_sha256), (old, draft.stamp, draft.read_sha256.clone()));
        // the probe and the plan load under the running build
        let probe_path = dir.write("probe.conf", &probe.render().unwrap());
        load_file(&probe_path, LoadOptions::default()).unwrap();
        write_atomic(&path, &plan.file.render().unwrap(), true).unwrap();
        load_file(&path, LoadOptions::default()).unwrap();
    }

    #[test]
    fn finishing_with_the_recorded_witnesses_is_the_phase_2_plan_byte_for_byte() {
        let dir = Dir::new();
        let old = old_witnessed(&[("builtin.a", num(1.0))]);
        let path = dir.write(FROZEN_FILE_NAME, &old.render().unwrap());
        let draft = prepare_update(&path, "new library", None, None, T0, "t@h").unwrap().unwrap();
        let plan = draft.finish(None).unwrap();
        let wrapped = plan_update(&path, "new library", None, None, T0, "t@h").unwrap().unwrap();
        assert_eq!(plan.file.render().unwrap(), wrapped.file.render().unwrap());
        assert_eq!(plan, wrapped);
        assert_eq!(plan.file, *draft.probe());
        assert_eq!(plan.witnesses, None);
        assert_eq!(plan.summary, "embedded: +2 -1 canonical lines; engine: changed; policy: release -> release");
        // seal
        let seal_dir = Dir::new();
        let sealed = frozen_in(&seal_dir);
        let text = fs::read_to_string(&sealed).unwrap().replace(EMPTY_OVERRIDES.trim_end(), "[Constants]\nzz_c = 3 m");
        fs::write(&sealed, &text).unwrap();
        let draft = prepare_seal(&sealed, "mine", T0, "t@h").unwrap().unwrap();
        let plan = draft.finish(None).unwrap();
        let wrapped = plan_seal(&sealed, "mine", T0, "t@h").unwrap().unwrap();
        assert_eq!(plan.file.render().unwrap(), wrapped.file.render().unwrap());
        assert_eq!(plan, wrapped);
        assert_eq!(plan.file.history().len(), 2);
    }

    #[test]
    fn the_diff_line_names_changed_and_missing_keys_bounded_and_the_old_block_hash() {
        let mut recorded: Vec<(String, String)> =
            (0..12).map(|i| (format!("builtin.k{i:02}"), format_value(&num(f64::from(i))))).collect();
        recorded.extend((0..11).map(|i| (format!("builtin.m{i:02}"), format_value(&num(1.0)))));
        let recorded = Pairs::from_vec(recorded).unwrap();
        let running: Vec<(String, Result<WitnessValue, String>)> =
            (0..12).map(|i| (format!("builtin.k{i:02}"), Ok(num(f64::from(i) + 100.0)))).collect();
        let drift = rerecord(&recorded, &running).unwrap().drift;
        let hash = sha256_hex(canonical_pairs(recorded.as_slice()).as_bytes());
        assert_eq!(drift.old_block_sha256, hash);
        let names = |prefix: &str| (0..10).map(|i| format!("builtin.{prefix}{i:02}")).collect::<Vec<_>>().join(", ");
        assert_eq!(
            drift.line(),
            format!(
                "witnesses: 12 changed, 0 new, 11 missing; changed: {} +2 more; missing: {} +1 more; old [Witness] sha256 {}",
                names("k"),
                names("m"),
                &hash[..12]
            )
        );
        assert!(!drift.is_empty());
        // only new keys: no names, still the hash
        let new = rerecord(&Pairs::default(), &rows(&[("builtin.x", Ok(num(1.0)))])).unwrap().drift;
        assert_eq!(new.line(), format!("witnesses: 0 changed, 1 new, 0 missing; old [Witness] sha256 {}", &sha256_hex(b"")[..12]));
        let same = rerecord(&Pairs::new(&[("builtin.x", &format_value(&num(1.0)))]).unwrap(), &rows(&[("builtin.x", Ok(num(1.0)))]));
        assert!(same.unwrap().drift.is_empty());
    }

    #[test]
    fn rerecord_refuses_a_row_that_did_not_evaluate_and_names_it() {
        let running = rows(&[("builtin.a", Ok(num(1.0))), ("builtin.b", Err("panic: boom")), ("builtin.c", Err("limit: loop"))]);
        let e = rerecord(&Pairs::default(), &running).unwrap_err();
        let message = e.to_string();
        assert!(matches!(e, OpsError::Refused(_)), "{e:?}");
        assert!(message.contains("builtin.b: panic: boom") && message.contains("builtin.c: limit: loop"), "{message}");
        assert!(!message.contains("builtin.a"), "{message}");
    }

    #[test]
    fn freeze_records_the_rows_and_scripts_in_one_entry_and_refuses_an_error_row() {
        let dir = Dir::new();
        dir.write("physure.conf", "[Constants]\nzz_c = 3 m\n");
        let draft = prepare_freeze(&options(&dir, EnginePolicy::Release)).unwrap();
        assert_eq!(draft.out, dir.0.join(FROZEN_FILE_NAME));
        let probe = draft.probe().clone();
        assert!(probe.witnesses().is_empty() && probe.witness_scripts().is_empty() && probe.verify().is_empty());
        let probe_path = dir.write("probe.conf", &probe.render().unwrap());
        load_file(&probe_path, LoadOptions::default()).unwrap();
        let hash = "ab".repeat(32);
        let running = rows(&[
            ("builtin.chem.b", Ok(WitnessValue::failed("no Ca"))),
            ("builtin.units.a", Ok(num(0.3048))),
            ("user.mine", Ok(WitnessValue::Hash(hash.clone()))),
        ]);
        let scripts = vec![WitnessScript::new("mine", "x = 1 m\r\n").unwrap()];
        let (file, out) = draft.finish(&running, scripts.clone()).unwrap();
        assert_eq!(out, draft.out);
        same_but_witnesses(&probe, &file);
        let keys: Vec<&str> = file.witnesses().iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["builtin.chem.b", "builtin.units.a", "user.mine"]);
        assert_eq!(file.witnesses().get("user.mine"), Some(format!("sha256:{hash}").as_str()));
        assert_eq!(file.witness_scripts(), scripts.as_slice());
        assert_eq!(file.history().len(), 1);
        assert_eq!(file.history()[0].diff(), format!("{}; witnesses: 2 built-in, 1 user", probe.history()[0].diff()));
        let written = dir.write("witnessed.conf", &file.render().unwrap());
        load_file(&written, LoadOptions::default()).unwrap();
        // no rows, no scripts: the probe, which is what `freeze` returns
        assert_eq!(draft.finish(&[], Vec::new()).unwrap().0, probe);
        assert_eq!(freeze(&options(&dir, EnginePolicy::Release)).unwrap().0, probe);
        // an error row refuses, naming the case
        let mut broken = running.clone();
        broken.insert(1, ("builtin.broken".into(), Err("panic: boom".into())));
        let e = draft.finish(&broken, scripts.clone()).unwrap_err();
        assert!(matches!(&e, OpsError::Refused(m) if m.contains("builtin.broken: panic: boom")), "{e:?}");
        // a user row without its script, and a script without its row
        assert!(draft.finish(&running, Vec::new()).is_err());
        assert!(draft.finish(&running[..2], scripts).is_err());
        assert_eq!(dir.names(), vec!["physure.conf", "probe.conf", "witnessed.conf"], "nothing else was written");
    }

    #[test]
    fn prepare_freeze_refuses_before_anything_is_evaluated() {
        let dir = Dir::new();
        let path = frozen_in(&dir);
        assert_eq!(prepare_freeze(&options(&dir, EnginePolicy::Release)).unwrap_err(), OpsError::AlreadyFrozen(path));
        let other = Dir::new();
        other.write("taken.conf", "x");
        let taken = FreezeOptions { out: Some(PathBuf::from("taken.conf")), ..options(&other, EnginePolicy::Release) };
        assert_eq!(prepare_freeze(&taken).unwrap_err(), OpsError::WouldOverwrite(other.0.join("taken.conf")));
    }

    #[test]
    fn a_plan_carries_the_hash_of_the_bytes_it_read_and_sees_an_edit_the_stamp_cannot() {
        let dir = Dir::new();
        let path = frozen_in(&dir);
        let edited = fs::read_to_string(&path).unwrap().replace(EMPTY_OVERRIDES.trim_end(), "[Constants]\nzz_c = 3 m");
        fs::write(&path, &edited).unwrap();
        let plan = prepare_seal(&path, "mine", T0, "t@h").unwrap().unwrap().finish(None).unwrap();
        assert_eq!(plan.read_sha256, sha256_hex(edited.as_bytes()));
        assert!(bytes_unchanged_since(&path, &plan.read_sha256));
        // the same length, the modification time set back: the stamp is blind to it, the hash is not
        let other = edited.replace("zz_c = 3 m", "zz_c = 4 m");
        assert_eq!(other.len(), edited.len());
        fs::write(&path, &other).unwrap();
        fs::File::options().write(true).open(&path).unwrap().set_modified(plan.stamp.modified.unwrap()).unwrap();
        assert_eq!(FileStamp::of(&fs::metadata(&path).unwrap()), plan.stamp, "the stamp check still exists and is blind here");
        assert!(!bytes_unchanged_since(&path, &plan.read_sha256));
        assert!(!bytes_unchanged_since(&dir.0.join("missing.conf"), &plan.read_sha256));
    }

    #[test]
    fn a_seal_draft_exists_only_after_the_proof_that_only_the_overrides_changed() {
        let dir = Dir::new();
        let path = frozen_in(&dir);
        assert_eq!(prepare_seal(&path, "x", T0, "t@h").unwrap(), None, "a clean file");
        let base = fs::read_to_string(&path).unwrap();
        let mut engine = FrozenFile::parse(&base).unwrap();
        engine.set_engine(engine_pairs(&EngineIdentity::current(), EnginePolicy::Record, None).unwrap());
        fs::write(&path, engine.render().unwrap()).unwrap();
        let e = prepare_seal(&path, "x", T0, "t@h").unwrap_err();
        assert!(matches!(&e, OpsError::Refused(m) if m.contains("overrides block only")), "{e:?}");
        // a hand edit of the overrides: the probe pins the engine of the file as read (the host checks it first)
        fs::write(&path, base.replace(EMPTY_OVERRIDES.trim_end(), "[Constants]\nzz_c = 3 m")).unwrap();
        let draft = prepare_seal(&path, "mine", T0, "t@h").unwrap().unwrap();
        assert_eq!(draft.probe().engine(), draft.old.engine());
        assert!(!draft.next().verify().is_empty(), "the draft's file is not sealed");
        let running = rows(&[("builtin.a", Ok(num(2.0)))]);
        let plan = draft.finish(Some(&rerecord(draft.old.witnesses(), &running).unwrap())).unwrap();
        same_but_witnesses(draft.probe(), &plan.file);
        assert_eq!(plan.file.history().len(), 2);
        assert!(plan.summary.starts_with("overrides edited by hand: ") && plan.summary.contains("; witnesses: 0 changed, 1 new, 0 missing"));
    }
}
