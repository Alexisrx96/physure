//! Building, verifying and re-sealing a frozen file (spec §4.1, §5.3).

use std::collections::HashSet;
use std::fmt;

use super::canonical::{canonical_pairs, content_sha256, exact_sha256, source_sha256};
use super::engine::{read_engine, recordable, EngineBlock};
use super::model::{FrozenError, FrozenFile, FrozenResult, HistoryEntry, Pairs, Source, WitnessScript, FORMAT_VERSION};
use super::sha256::sha256_hex;
use super::time::is_rfc3339_utc;

/// The facts of one history entry that the caller knows. The model never reads a clock or the environment.
pub struct EntryInput<'a> {
    pub time: &'a str,
    /// `freeze`, `update` or `seal`.
    pub action: &'a str,
    /// The engine acting, short form (`0.3.1+9f2c1ab`): informational text recorded as given, not checked. What the
    /// file pins is derived by the library from the `[Engine]` block (`policy`, `pinned`, `engine_sha256`).
    pub engine: &'a str,
    /// `user@host`: informational, not authenticated.
    pub actor: &'a str,
    /// Why: 1 to 500 characters on one line.
    pub reason: &'a str,
    /// A one-line summary of what changed.
    pub diff: &'a str,
}

/// What a new file is made of, by name, so that the three blocks of pairs cannot be swapped by position.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrozenParts {
    pub engine: Pairs,
    /// Informational, outside the seal.
    pub environment: Pairs,
    pub witnesses: Pairs,
    /// Source 1 is `embedded`, source 2 is `overrides`; further sources follow with unique origins.
    pub sources: Vec<Source>,
    pub witness_scripts: Vec<WitnessScript>,
}

fn one_line(what: &str, value: &str) -> FrozenResult<()> {
    if value.contains('\n') || value.contains('\r') || value != value.trim() {
        return Err(FrozenError(format!("{what} must be one line without leading or trailing whitespace, found {value:?}")));
    }
    Ok(())
}

const MAX_REASON: usize = 500;

impl EntryInput<'_> {
    fn validate(&self) -> FrozenResult<()> {
        if !matches!(self.action, "freeze" | "update" | "seal") {
            return Err(FrozenError(format!("action must be freeze, update or seal, not {:?}", self.action)));
        }
        if !is_rfc3339_utc(self.time) {
            return Err(FrozenError(format!("time must be a UTC time like 2026-11-07T14:03:11Z, not {:?}", self.time)));
        }
        let length = self.reason.chars().count();
        if length == 0 || length > MAX_REASON {
            return Err(FrozenError(format!("the reason must be 1 to {MAX_REASON} characters, found {length}")));
        }
        for (what, value) in [("the reason", self.reason), ("the engine", self.engine), ("the actor", self.actor), ("the diff", self.diff)] {
            one_line(what, value)?;
        }
        Ok(())
    }
}

/// What `verify` found wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Format { found: u32 },
    /// The file could not be written as text (a name, a value, a marker or a hash has the wrong shape).
    Framing { found: String },
    EmptyHistory,
    SourceOrigin { index: usize, expected: String, found: String },
    RepeatedOrigin { origin: String },
    RepeatedWitness { name: String },
    WitnessKeys { expected: Vec<String>, found: Vec<String> },
    SourceHash { index: usize, origin: String, expected: String, found: String },
    ContentHash { expected: String, found: String },
    WitnessScriptHash { name: String, expected: String, found: String },
    HistoryNumber { position: usize, found: u32 },
    HistoryPrev { n: u32, expected: String, found: String },
    HistoryEntryHash { n: u32, expected: String, found: String },
    HistoryAction { n: u32, expected: String, found: String },
    HistoryContinuity { n: u32, expected: String, found: String },
    HistoryReason { n: u32, found: usize },
    Created { expected: String, found: String },
    HistoryTip { expected: String, found: String },
    /// The last history entry does not record the current `[Engine]` block: the block was changed without a
    /// reseal, or the entry's `engine_sha256` was altered. `expected` is the hash of the block as it is now.
    HistoryEngine { expected: String, found: String },
    /// The last history entry's `policy` or `pinned` (`field`) is not what the current `[Engine]` block says.
    HistoryPin { field: String, expected: String, found: String },
    /// The `[Engine]` block cannot be read (`read_engine`: a missing or unknown key, or an invalid value), or could
    /// not have been recorded (`build`/`binary` without a known commit, `binary` without a hash).
    Engine { message: String },
    Seal { expected: String, found: String },
    /// History entry `n` is earlier than the entry before it, or, for the first, than `created` (`previous`). The
    /// history is chronological; equal times are allowed.
    HistoryTime { n: u32, previous: String, found: String },
}

fn shown(hash: &str) -> &str {
    if hash.is_empty() {
        "nothing"
    } else {
        hash
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Format { found } => write!(f, "unsupported format: expected {FORMAT_VERSION}, found {found}"),
            Problem::Framing { found } => write!(f, "the file cannot be written as a frozen file: expected valid names, values and markers, found: {found}"),
            Problem::EmptyHistory => write!(f, "the history is empty: expected at least the freeze entry, found nothing"),
            Problem::SourceOrigin { index, expected, found } => {
                write!(f, "source {index} has the wrong origin: expected {expected}, found {found}")
            }
            Problem::RepeatedOrigin { origin } => {
                write!(f, "the origin {origin} is used by more than one source: expected each origin once, found it again")
            }
            Problem::RepeatedWitness { name } => {
                write!(f, "the witness {name} appears more than once: expected each witness name once, found it again")
            }
            Problem::WitnessKeys { expected, found } => write!(
                f,
                "the user.* keys of [Witness] must name the witness scripts, one each: expected [{}], found [{}]",
                expected.join(", "),
                found.join(", ")
            ),
            Problem::SourceHash { index, origin, expected, found } => {
                write!(f, "source {index} ({origin}) was modified: expected {expected}, found {found}")
            }
            Problem::ContentHash { expected, found } => {
                write!(f, "the content hash does not match the sources: expected {expected}, found {found}")
            }
            Problem::WitnessScriptHash { name, expected, found } => {
                write!(f, "witness {name} was modified: expected {expected}, found {found}")
            }
            Problem::HistoryNumber { position, found } => {
                write!(f, "history entry at position {position} is misnumbered: expected {position}, found {found}")
            }
            Problem::HistoryPrev { n, expected, found } => {
                write!(f, "History.{n} does not follow the previous entry: expected {}, found {}", shown(expected), shown(found))
            }
            Problem::HistoryEntryHash { n, expected, found } => {
                write!(f, "History.{n} was modified: expected {expected}, found {found}")
            }
            Problem::HistoryAction { n, expected, found } => {
                write!(f, "History.{n} has the wrong action: expected {expected}, found {found:?}")
            }
            Problem::HistoryContinuity { n, expected, found } => write!(
                f,
                "History.{n} does not start from the content the previous entry left: expected old content {}, found {}",
                shown(expected),
                shown(found)
            ),
            Problem::HistoryReason { n, found } => {
                write!(f, "History.{n} has a reason of the wrong length: expected 1 to {MAX_REASON} characters, found {found}")
            }
            Problem::Created { expected, found } => {
                write!(f, "created is not the time of the first history entry: expected {expected}, found {found}")
            }
            Problem::HistoryTip { expected, found } => {
                write!(f, "the last history entry does not record the current content: expected {expected}, found {found}")
            }
            Problem::HistoryEngine { expected, found } => write!(
                f,
                "the last history entry does not record the current [Engine] block (changed without a reseal, or the entry was altered): expected engine_sha256 {expected}, found {found}"
            ),
            Problem::HistoryPin { field, expected, found } => write!(
                f,
                "the last history entry does not record what the current [Engine] block pins: expected {field} {expected}, found {found}"
            ),
            Problem::Engine { message } => {
                write!(f, "the [Engine] block cannot be read or could not have been recorded: expected a valid engine block whose policy can be checked, found: {message}")
            }
            Problem::Seal { expected, found } => write!(
                f,
                "the seal does not match the format, creation time, content, engine, witnesses, scripts and history: expected {expected}, found {found}"
            ),
            Problem::HistoryTime { n, previous, found } => write!(
                f,
                "History.{n} is earlier than the entry before it (or than created, for the first): expected a time at or after {previous}, found {found}"
            ),
        }
    }
}

/// The text an entry's hash is computed over (spec §5.3): labelled lines in a fixed order, every value one line.
/// `engine_sha256`, `policy` and `pinned` sit right after `engine`, so a change of the engine block, and of what
/// it pins, is a change of the log.
fn entry_sha256(entry: &HistoryEntry) -> String {
    let text = format!(
        "n={}\ntime={}\naction={}\nengine={}\nengine_sha256={}\npolicy={}\npinned={}\nactor={}\nreason={}\nold_content_sha256={}\nnew_content_sha256={}\ndiff={}\nprev_entry_sha256={}",
        entry.n, entry.time, entry.action, entry.engine, entry.engine_sha256, entry.policy, entry.pinned, entry.actor,
        entry.reason, entry.old_content_sha256, entry.new_content_sha256, entry.diff, entry.prev_entry_sha256
    );
    sha256_hex(text.as_bytes())
}

/// `engine_sha256`: the SHA-256 of the canonical pairs of `[Engine]`, the same value in the seal and in each
/// history entry.
fn engine_sha256(engine: &Pairs) -> String {
    sha256_hex(canonical_pairs(engine.as_slice()).as_bytes())
}

/// The seal text (spec §4.1): labelled lines, each part a fixed-length hash or a one-line scalar, so no boundary
/// can move. Witness scripts are hashed byte-exact, in file order.
fn seal_text(file: &FrozenFile, content: &str, last_entry: &str) -> String {
    let scripts: String =
        file.witness_scripts.iter().map(|s| format!("{}\n{}\n", s.name, exact_sha256(&s.text))).collect();
    format!(
        "physure-frozen-seal-1\nformat:{}\ncreated:{}\ncontent:{content}\nengine:{}\nwitnesses:{}\nscripts:{}\nhistory:{last_entry}\n",
        file.format,
        file.created,
        engine_sha256(&file.engine),
        sha256_hex(canonical_pairs(file.witnesses.as_slice()).as_bytes()),
        sha256_hex(scripts.as_bytes()),
    )
}

fn seal_sha256(file: &FrozenFile, content: &str, last_entry: &str) -> String {
    sha256_hex(seal_text(file, content, last_entry).as_bytes())
}

fn refuse(problems: &[Problem]) -> FrozenError {
    FrozenError(problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))
}

impl FrozenFile {
    /// The content hash computed from the texts of the sources as they are now.
    pub fn computed_content_sha256(&self) -> String {
        let hashes: Vec<(String, String)> =
            self.sources.iter().map(|s| (s.origin.clone(), source_sha256(&s.text))).collect();
        content_sha256(&hashes)
    }

    /// A new frozen file with its first history entry (`freeze`). All hashes are computed; the structure rules
    /// (origins, witness keys, `created` = the entry's time) are checked.
    pub fn new(created: &str, parts: FrozenParts, first: EntryInput<'_>) -> FrozenResult<FrozenFile> {
        if first.action != "freeze" {
            return Err(FrozenError(format!("the first history entry of a file is a freeze, not {:?}", first.action)));
        }
        first.validate()?;
        if !is_rfc3339_utc(created) {
            return Err(FrozenError(format!("created must be a UTC time like 2026-11-07T14:03:11Z, not {created:?}")));
        }
        if created != first.time {
            return Err(FrozenError(format!(
                "created must be the time of the first history entry: expected {}, found {created}",
                first.time
            )));
        }
        let FrozenParts { engine, environment, witnesses, sources, witness_scripts } = parts;
        let mut file = FrozenFile {
            format: FORMAT_VERSION,
            created: created.to_string(),
            content_sha256: String::new(),
            seal_sha256: String::new(),
            engine,
            environment,
            witnesses,
            history: Vec::new(),
            sources,
            witness_scripts,
        };
        file.append_entry(first);
        let problems = file.verify();
        if !problems.is_empty() {
            return Err(refuse(&problems));
        }
        Ok(file)
    }

    /// Accepts the current state of the sources, the engine and the witnesses (after an edit by hand or an
    /// update): recomputes every hash, appends a chained history entry whose old content hash is the one stored
    /// before, and seals again. Refused, leaving the file as it was, when the action is `freeze` or when the
    /// result would not verify (a broken history or a structure rule).
    pub fn reseal(&mut self, entry: EntryInput<'_>) -> FrozenResult<()> {
        if entry.action == "freeze" {
            return Err(FrozenError("a reseal records an update or a seal, not a freeze: a file has one freeze, its first entry".into()));
        }
        entry.validate()?;
        let mut next = self.clone();
        next.append_entry(entry);
        let problems = next.verify();
        if !problems.is_empty() {
            return Err(refuse(&problems));
        }
        *self = next;
        Ok(())
    }

    fn append_entry(&mut self, input: EntryInput<'_>) {
        for source in &mut self.sources {
            source.sha256 = source_sha256(&source.text);
        }
        for script in &mut self.witness_scripts {
            script.sha256 = exact_sha256(&script.text);
        }
        let old = std::mem::take(&mut self.content_sha256);
        self.content_sha256 = self.computed_content_sha256();
        let prev = self.history.last().map(|e| e.entry_sha256.clone()).unwrap_or_default();
        // derived from the block, never from the caller; an unreadable block leaves them empty, and `verify`
        // (which `new` and `reseal` run) refuses the result
        let (policy, pinned) = read_engine(&self.engine)
            .map(|block| (block.policy.as_str().to_string(), block.identity.short()))
            .unwrap_or_default();
        let mut entry = HistoryEntry {
            n: self.history.len() as u32 + 1,
            time: input.time.to_string(),
            action: input.action.to_string(),
            engine: input.engine.to_string(),
            engine_sha256: engine_sha256(&self.engine),
            policy,
            pinned,
            actor: input.actor.to_string(),
            reason: input.reason.to_string(),
            old_content_sha256: old,
            new_content_sha256: self.content_sha256.clone(),
            diff: input.diff.to_string(),
            prev_entry_sha256: prev,
            entry_sha256: String::new(),
        };
        entry.entry_sha256 = entry_sha256(&entry);
        let tip = entry.entry_sha256.clone();
        self.history.push(entry);
        self.seal_sha256 = seal_sha256(self, &self.content_sha256.clone(), &tip);
    }

    fn structure_problems(&self, problems: &mut Vec<Problem>) {
        for (index, expected) in ["embedded", "overrides"].into_iter().enumerate() {
            let found = self.sources.get(index).map_or("no source", |s| s.origin.as_str());
            if found != expected {
                problems.push(Problem::SourceOrigin { index: index + 1, expected: expected.into(), found: found.into() });
            }
        }
        let mut origins = HashSet::new();
        for source in &self.sources {
            if !origins.insert(source.origin.as_str()) {
                problems.push(Problem::RepeatedOrigin { origin: source.origin.clone() });
            }
        }
        let mut names = HashSet::new();
        for script in &self.witness_scripts {
            if !names.insert(script.name.as_str()) {
                problems.push(Problem::RepeatedWitness { name: script.name.clone() });
            }
        }
        let mut expected: Vec<String> = self.witness_scripts.iter().map(|s| format!("user.{}", s.name)).collect();
        let mut found: Vec<String> =
            self.witnesses.iter().map(|(k, _)| k).filter(|k| k.starts_with("user.")).map(String::from).collect();
        expected.sort();
        found.sort();
        if expected != found {
            problems.push(Problem::WitnessKeys { expected, found });
        }
    }

    /// Everything that is wrong with the file, not only the first thing. Empty means the file is intact.
    pub fn verify(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.format != FORMAT_VERSION {
            problems.push(Problem::Format { found: self.format });
        }
        problems.extend(self.framing_problems().into_iter().map(|found| Problem::Framing { found }));
        // readable, and something that could have been recorded (a policy that can be checked)
        let block = read_engine(&self.engine).and_then(|block| {
            recordable(&block.identity, block.policy, block.binary_sha256.as_deref())
                .map(|()| block)
                .map_err(|e| FrozenError(format!("[Engine] {e}")))
        });
        let block: Option<EngineBlock> = match block {
            Ok(block) => Some(block),
            Err(e) => {
                problems.push(Problem::Engine { message: e.0 });
                None
            }
        };
        self.structure_problems(&mut problems);
        for (i, source) in self.sources.iter().enumerate() {
            let computed = source_sha256(&source.text);
            if computed != source.sha256 {
                problems.push(Problem::SourceHash {
                    index: i + 1,
                    origin: source.origin.clone(),
                    expected: source.sha256.clone(),
                    found: computed,
                });
            }
        }
        let content = self.computed_content_sha256();
        if content != self.content_sha256 {
            problems.push(Problem::ContentHash { expected: self.content_sha256.clone(), found: content.clone() });
        }
        for script in &self.witness_scripts {
            let computed = exact_sha256(&script.text);
            if computed != script.sha256 {
                problems.push(Problem::WitnessScriptHash {
                    name: script.name.clone(),
                    expected: script.sha256.clone(),
                    found: computed,
                });
            }
        }
        let mut previous = String::new();
        let mut previous_content = String::new();
        // fixed-width RFC 3339 UTC times compare as text exactly as they compare as times
        let mut previous_time = &self.created;
        for (position, entry) in self.history.iter().enumerate() {
            if is_rfc3339_utc(previous_time) && is_rfc3339_utc(&entry.time) && entry.time < *previous_time {
                problems.push(Problem::HistoryTime { n: entry.n, previous: previous_time.clone(), found: entry.time.clone() });
            }
            previous_time = &entry.time;
            if entry.n as usize != position + 1 {
                problems.push(Problem::HistoryNumber { position: position + 1, found: entry.n });
            }
            if entry.prev_entry_sha256 != previous {
                problems.push(Problem::HistoryPrev { n: entry.n, expected: previous.clone(), found: entry.prev_entry_sha256.clone() });
            }
            let computed = entry_sha256(entry);
            if computed != entry.entry_sha256 {
                problems.push(Problem::HistoryEntryHash { n: entry.n, expected: entry.entry_sha256.clone(), found: computed });
            }
            let (action_ok, expected_action) = if position == 0 {
                (entry.action == "freeze", "freeze")
            } else {
                (matches!(entry.action.as_str(), "update" | "seal"), "update or seal")
            };
            if !action_ok {
                problems.push(Problem::HistoryAction { n: entry.n, expected: expected_action.into(), found: entry.action.clone() });
            }
            if entry.old_content_sha256 != previous_content {
                problems.push(Problem::HistoryContinuity {
                    n: entry.n,
                    expected: previous_content.clone(),
                    found: entry.old_content_sha256.clone(),
                });
            }
            let length = entry.reason.chars().count();
            if length == 0 || length > MAX_REASON {
                problems.push(Problem::HistoryReason { n: entry.n, found: length });
            }
            previous = entry.entry_sha256.clone();
            previous_content = entry.new_content_sha256.clone();
        }
        let (Some(first), Some(last)) = (self.history.first(), self.history.last()) else {
            problems.push(Problem::EmptyHistory);
            return problems;
        };
        if self.created != first.time {
            problems.push(Problem::Created { expected: first.time.clone(), found: self.created.clone() });
        }
        if last.new_content_sha256 != self.content_sha256 {
            problems.push(Problem::HistoryTip { expected: self.content_sha256.clone(), found: last.new_content_sha256.clone() });
        }
        let engine = engine_sha256(&self.engine);
        if last.engine_sha256 != engine {
            problems.push(Problem::HistoryEngine { expected: engine, found: last.engine_sha256.clone() });
        }
        if let Some(block) = &block {
            for (field, expected, found) in
                [("policy", block.policy.as_str().to_string(), &last.policy), ("pinned", block.identity.short(), &last.pinned)]
            {
                if *found != expected {
                    problems.push(Problem::HistoryPin { field: field.into(), expected, found: found.clone() });
                }
            }
        }
        let seal = seal_sha256(self, &content, &last.entry_sha256);
        if seal != self.seal_sha256 {
            problems.push(Problem::Seal { expected: self.seal_sha256.clone(), found: seal });
        }
        problems
    }

    /// Whether the file is exactly as it was last sealed except for the text of source 2, the overrides block, whose
    /// canonical form changed: what `seal` may accept (Task 9, F19). A rule on the list of problems alone is not
    /// enough (an edit of source 1 with its marker recomputed, or a source appended with a correct marker, give the
    /// same problems as an edit of the overrides), so this proves it positively. True exactly when:
    ///
    /// * `verify` reports nothing but the stale marker of source 2, the content hash and the seal (so the history
    ///   chain, the structure, every other marker and the engine block are intact);
    /// * the seal recomputed over the **stored** content hash equals the stored seal (format, `created`, engine,
    ///   witnesses, scripts, the stored content hash and the whole history are as sealed);
    /// * the content hash recomputed from every source's text, except source 2 whose **stored marker** is used,
    ///   equals the stored content hash (every other source, the count, order and origins of the sources, and the
    ///   marker of source 2 are as sealed);
    /// * the canonical text of source 2 no longer matches its marker.
    pub fn only_the_overrides_changed(&self) -> bool {
        let allowed = |p: &Problem| matches!(p, Problem::SourceHash { index: 2, .. } | Problem::ContentHash { .. } | Problem::Seal { .. });
        if !self.verify().iter().all(allowed) {
            return false;
        }
        let (Some(last), Some(overrides)) = (self.history.last(), self.sources.get(1)) else {
            return false;
        };
        if seal_sha256(self, &self.content_sha256, &last.entry_sha256) != self.seal_sha256 {
            return false;
        }
        let hashes: Vec<(String, String)> = self
            .sources
            .iter()
            .enumerate()
            .map(|(index, s)| (s.origin.clone(), if index == 1 { s.sha256.clone() } else { source_sha256(&s.text) }))
            .collect();
        if content_sha256(&hashes) != self.content_sha256 {
            return false;
        }
        source_sha256(&overrides.text) != overrides.sha256
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frozen::canonical::is_sha256_hex;
    use crate::frozen::engine::{engine_pairs, EngineIdentity, EnginePolicy};
    use crate::frozen::model::{FrozenFile, Source, WitnessScript};

    fn pairs(items: &[(&str, &str)]) -> Pairs {
        Pairs::new(items).unwrap()
    }

    /// A valid `[Engine]` block with fixed fields (the fixtures must not depend on the build that runs them).
    fn engine_block(policy: EnginePolicy) -> Pairs {
        let identity = EngineIdentity {
            version: "0.3.1".into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            profile: "release".into(),
            target: "x86_64-pc-windows-msvc".into(),
            features: "html-viewer".into(),
            rustc: "rustc 1.97.1 (8bab26f4f 2026-07-14)".into(),
            flags: "opt-level=3;debug-assertions=off;panic=unwind;rustflags=00;target-features=00".into(),
        };
        engine_pairs(&identity, policy, None).unwrap()
    }

    fn engine() -> Pairs {
        engine_block(EnginePolicy::Release)
    }

    /// The SHA-256 of a block's canonical pairs, computed here independently of `append_entry`.
    fn block_sha256(block: &Pairs) -> String {
        sha256_hex(canonical_pairs(block.as_slice()).as_bytes())
    }

    fn input<'a>(action: &'a str, reason: &'a str, time: &'a str) -> EntryInput<'a> {
        EntryInput { time, action, engine: "0.3.1+9f2c1ab", actor: "irvin@host", reason, diff: "added 2" }
    }

    fn fresh() -> FrozenFile {
        fresh_with(engine())
    }

    fn fresh_with(engine: Pairs) -> FrozenFile {
        FrozenFile::new(
            "2026-11-07T14:03:11Z",
            FrozenParts {
                engine,
                environment: pairs(&[("os", "windows")]),
                witnesses: pairs(&[("builtin.conversion", "3ff0000000000000"), ("user.validation.1", "1a2b")]),
                sources: vec![Source::new("embedded", "[Units]\nm = 1\nkg = 2").unwrap(), Source::new("overrides", "# mine\n").unwrap()],
                witness_scripts: vec![WitnessScript::new("validation.1", "1 m => ft").unwrap()],
            },
            input("freeze", "initial freeze", "2026-11-07T14:03:11Z"),
        )
        .unwrap()
    }

    fn problems(file: &FrozenFile) -> Vec<String> {
        file.verify().iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_new_file_verifies_and_survives_a_trip_through_text() {
        let file = fresh();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        assert_eq!(file.history.len(), 1);
        assert_eq!(file.history[0].prev_entry_sha256, "");
        assert_eq!(file.history[0].old_content_sha256, "");
        assert_eq!(file.history[0].new_content_sha256, file.content_sha256);
        let again = FrozenFile::parse(&file.render().unwrap()).unwrap();
        assert!(problems(&again).is_empty());
        assert_eq!(again, file);
    }

    #[test]
    fn the_same_inputs_give_the_same_file() {
        assert_eq!(fresh().render().unwrap(), fresh().render().unwrap());
    }

    fn reparsed_with(edit: impl Fn(&str) -> String) -> FrozenFile {
        FrozenFile::parse(&edit(&fresh().render().unwrap())).unwrap()
    }

    #[test]
    fn editing_a_value_of_a_source_breaks_the_source_the_content_and_the_seal() {
        let file = reparsed_with(|t| t.replace("kg = 2", "kg = 3"));
        let p = problems(&file);
        assert!(p.iter().any(|m| m.contains("source 1") && m.contains("embedded")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("content")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("seal")), "{p:?}");
    }

    #[test]
    fn editing_a_comment_changes_nothing() {
        let file = reparsed_with(|t| t.replace("# mine", "# somebody else's"));
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
    }

    #[test]
    fn editing_the_engine_or_a_witness_breaks_the_seal() {
        for edit in [("version = 0.3.1", "version = 0.3.2"), ("3ff0000000000000", "3ff0000000000001")] {
            let p = problems(&reparsed_with(|t| t.replace(edit.0, edit.1)));
            assert!(p.iter().any(|m| m.contains("seal")), "{edit:?}: {p:?}");
        }
    }

    #[test]
    fn editing_the_environment_is_allowed() {
        assert!(problems(&reparsed_with(|t| t.replace("os = windows", "os = linux"))).is_empty());
    }

    #[test]
    fn editing_a_witness_script_is_detected() {
        let p = problems(&reparsed_with(|t| t.replace("1 m => ft", "1 m => yd")));
        assert!(p.iter().any(|m| m.contains("witness") && m.contains("validation.1")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("seal")), "the script is sealed: {p:?}");
    }

    #[test]
    fn a_history_entry_cannot_be_edited() {
        let p = problems(&reparsed_with(|t| t.replace("initial freeze", "nothing to see")));
        assert!(p.iter().any(|m| m.contains("History.1")), "{p:?}");
    }

    fn two_entries() -> FrozenFile {
        let mut file = fresh();
        file.set_source_text(1, "kg = 5").unwrap();
        file.reseal(input("seal", "raised the kilogram", "2026-11-08T09:00:00Z")).unwrap();
        file
    }

    #[test]
    fn resealing_after_an_edit_appends_a_chained_entry() {
        let before = fresh();
        let file = two_entries();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        assert_eq!(file.history.len(), 2);
        let (first, second) = (&file.history[0], &file.history[1]);
        assert_eq!(second.n, 2);
        assert_eq!(second.prev_entry_sha256, first.entry_sha256);
        assert_eq!(second.old_content_sha256, before.content_sha256);
        assert_eq!(second.new_content_sha256, file.content_sha256);
        assert_ne!(file.content_sha256, before.content_sha256);
        assert_ne!(file.seal_sha256, before.seal_sha256);
        assert_eq!(second.action, "seal");
        assert_eq!(first, &before.history[0], "earlier entries are untouched");
    }

    #[test]
    fn a_file_edited_by_hand_can_be_resealed_and_then_verifies() {
        let mut file = reparsed_with(|t| t.replace("kg = 2", "kg = 3"));
        assert!(!problems(&file).is_empty());
        file.reseal(input("seal", "accepted my edit", "2026-11-08T09:00:00Z")).unwrap();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        assert_eq!(file.history.len(), 2);
        assert_ne!(file.history[1].old_content_sha256, file.history[1].new_content_sha256);
    }

    #[test]
    fn deleting_reordering_or_forging_the_history_is_detected() {
        let text = two_entries().render().unwrap();
        let file = FrozenFile::parse(&text).unwrap();

        let mut dropped = file.clone();
        dropped.history.remove(0);
        for (i, e) in dropped.history.iter_mut().enumerate() {
            e.n = i as u32 + 1;
        }
        assert!(!problems(&dropped).is_empty(), "dropping the first entry");

        let mut swapped = file.clone();
        swapped.history.swap(0, 1);
        for (i, e) in swapped.history.iter_mut().enumerate() {
            e.n = i as u32 + 1;
        }
        assert!(!problems(&swapped).is_empty(), "swapping the entries");

        let mut truncated = file.clone();
        truncated.history.pop();
        assert!(!problems(&truncated).is_empty(), "removing the last entry leaves the content ahead of the log");

        let mut forged = file.clone();
        forged.history[1].reason = "something else".into();
        let p = problems(&forged);
        assert!(p.iter().any(|m| m.contains("History.2")), "{p:?}");

        let mut emptied = file.clone();
        emptied.history.clear();
        assert!(problems(&emptied).iter().any(|m| m.contains("history is empty")));
    }

    #[test]
    fn the_entry_input_is_validated() {
        let mut file = fresh();
        for bad in [
            input("frobnicate", "r", "2026-11-08T09:00:00Z"),
            input("update", "", "2026-11-08T09:00:00Z"),
            input("update", "two\nlines", "2026-11-08T09:00:00Z"),
            input("update", "r", "yesterday"),
            input("freeze", "r", "2026-11-08T09:00:00Z"),
        ] {
            assert!(file.reseal(bad).is_err());
        }
        assert_eq!(file, fresh(), "a refused reseal leaves the file as it was");
        assert!(file.reseal(input("update", &"x".repeat(501), "2026-11-08T09:00:00Z")).is_err());
        assert!(file.reseal(input("update", &"x".repeat(500), "2026-11-08T09:00:00Z")).is_ok());
        let parts = || FrozenParts {
            engine: engine(),
            sources: vec![Source::new("embedded", "").unwrap(), Source::new("overrides", "").unwrap()],
            ..FrozenParts::default()
        };
        assert!(FrozenFile::new("2026-11-07T14:03:11Z", parts(), input("update", "r", "2026-11-07T14:03:11Z")).is_err(), "the first entry is a freeze");
        assert!(FrozenFile::new("yesterday", parts(), input("freeze", "r", "2026-11-07T14:03:11Z")).is_err());
        assert!(FrozenFile::new("2026-11-07T14:03:11Z", parts(), input("freeze", "", "2026-11-07T14:03:11Z")).is_err());
        assert!(FrozenFile::new("2026-11-07T14:03:11Z", parts(), input("freeze", "r", "2026-11-07T14:03:11Z")).is_ok());
    }

    #[test]
    fn problems_say_what_was_expected_and_what_was_found() {
        let p = problems(&reparsed_with(|t| t.replace("kg = 2", "kg = 3")));
        let source = p.iter().find(|m| m.contains("source 1")).unwrap();
        assert!(source.contains("expected") && source.contains("found"), "{source}");
        let all = [
            Problem::Format { found: 2 },
            Problem::Framing { found: "x".into() },
            Problem::EmptyHistory,
            Problem::SourceOrigin { index: 1, expected: "embedded".into(), found: "mine".into() },
            Problem::RepeatedOrigin { origin: "x".into() },
            Problem::RepeatedWitness { name: "w".into() },
            Problem::WitnessKeys { expected: vec!["user.a".into()], found: vec![] },
            Problem::SourceHash { index: 1, origin: "o".into(), expected: "a".into(), found: "b".into() },
            Problem::ContentHash { expected: "a".into(), found: "b".into() },
            Problem::WitnessScriptHash { name: "w".into(), expected: "a".into(), found: "b".into() },
            Problem::HistoryNumber { position: 1, found: 2 },
            Problem::HistoryPrev { n: 1, expected: String::new(), found: "b".into() },
            Problem::HistoryEntryHash { n: 1, expected: "a".into(), found: "b".into() },
            Problem::HistoryAction { n: 1, expected: "freeze".into(), found: "seal".into() },
            Problem::HistoryContinuity { n: 1, expected: String::new(), found: "b".into() },
            Problem::HistoryReason { n: 1, found: 0 },
            Problem::Created { expected: "a".into(), found: "b".into() },
            Problem::HistoryTip { expected: "a".into(), found: "b".into() },
            Problem::HistoryEngine { expected: "a".into(), found: "b".into() },
            Problem::HistoryPin { field: "policy".into(), expected: "build".into(), found: "record".into() },
            Problem::Engine { message: "[Engine] has no commit".into() },
            Problem::Seal { expected: "a".into(), found: "b".into() },
            Problem::HistoryTime { n: 2, previous: "b".into(), found: "a".into() },
        ];
        for problem in all {
            let text = problem.to_string();
            assert!(text.contains("expected") && text.contains("found"), "{text}");
        }
    }

    // ---- Task 8 ----

    /// The seal of a file with these raw blocks and an otherwise fixed content and history. Since 1b an `[Engine]`
    /// block with arbitrary keys can no longer be sealed (verify reads it), so the seal formula is applied directly:
    /// the history entry is held fixed, and any difference comes from how the seal frames the two blocks.
    fn seal_with_blocks(engine: &[(&str, &str)], witnesses: &[(&str, &str)]) -> String {
        let mut file = FrozenFile::new(
            "2026-11-07T14:03:11Z",
            FrozenParts {
                engine: self::engine(),
                sources: vec![Source::new("embedded", "[Units]\nm = 1").unwrap(), Source::new("overrides", "").unwrap()],
                ..FrozenParts::default()
            },
            input("freeze", "initial freeze", "2026-11-07T14:03:11Z"),
        )
        .unwrap();
        file.engine = pairs(engine);
        file.witnesses = pairs(witnesses);
        let (content, last) = (file.content_sha256.clone(), file.history[0].entry_sha256.clone());
        seal_sha256(&file, &content, &last)
    }

    #[test]
    fn moving_a_pair_between_engine_and_witnesses_changes_the_seal() {
        let a = seal_with_blocks(&[("a", "1")], &[("b", "2"), ("c", "3")]);
        let b = seal_with_blocks(&[("a", "1"), ("b", "2")], &[("c", "3")]);
        assert_ne!(a, b);
        let c = seal_with_blocks(&[], &[("a", "1")]);
        let d = seal_with_blocks(&[("a", "1")], &[]);
        assert_ne!(c, d);
    }

    #[test]
    fn editing_a_witness_script_and_its_marker_hash_is_detected() {
        let text = fresh().render().unwrap();
        let old = exact_sha256("1 m => ft");
        let new = exact_sha256("1 m => yd");
        assert!(text.contains(&old));
        let file = FrozenFile::parse(&text.replace("1 m => ft", "1 m => yd").replace(&old, &new)).unwrap();
        let p = problems(&file);
        assert!(p.iter().any(|m| m.contains("seal")), "{p:?}");
    }

    #[test]
    fn deleting_or_adding_a_witness_block_is_detected() {
        let text = fresh().render().unwrap();
        let cut = text.find("@@ witness").unwrap();
        let p = problems(&FrozenFile::parse(&text[..cut]).unwrap());
        assert!(p.iter().any(|m| m.contains("seal")), "deleted: {p:?}");
        // deleting the block and its key together still breaks the seal
        let both = text[..cut].replace("user.validation.1 = 1a2b\n", "");
        let p = problems(&FrozenFile::parse(&both).unwrap());
        assert!(p.iter().any(|m| m.contains("seal")), "deleted with its key: {p:?}");
        let added = format!("{text}@@ witness extra sha256={}\n2 m => ft\n@@ end witness\n", exact_sha256("2 m => ft"));
        let p = problems(&FrozenFile::parse(&added).unwrap());
        assert!(p.iter().any(|m| m.contains("seal")), "added: {p:?}");
    }

    #[test]
    fn created_is_sealed_and_must_equal_the_first_entry_time() {
        let text = fresh().render().unwrap();
        let p = problems(&FrozenFile::parse(&text.replace("created = 2026-11-07T14:03:11Z", "created = 1999-01-01T00:00:00Z")).unwrap());
        assert!(p.iter().any(|m| m.contains("seal")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("created is not the time of the first history entry")), "{p:?}");
        let other = FrozenFile::new(
            "2020-01-01T00:00:00Z",
            FrozenParts {
                sources: vec![Source::new("embedded", "").unwrap(), Source::new("overrides", "").unwrap()],
                ..FrozenParts::default()
            },
            input("freeze", "r", "2026-11-07T14:03:11Z"),
        );
        assert!(other.unwrap_err().to_string().contains("expected 2026-11-07T14:03:11Z, found 2020-01-01T00:00:00Z"));
        // the seal covers created on its own: a file whose created and first time agree but differ from the
        // sealed ones (rechained) still does not reproduce the recorded seal
        let mut moved = fresh();
        let sealed = moved.seal_sha256.clone();
        moved.created = "2026-11-07T14:03:12Z".into();
        moved.history[0].time = "2026-11-07T14:03:12Z".into();
        rechain(&mut moved);
        assert!(problems(&moved).is_empty());
        assert_ne!(moved.seal_sha256, sealed);
    }

    #[test]
    fn origins_are_enforced() {
        let make = |origins: &[&str], scripts: &[&str]| {
            FrozenFile::new(
                "2026-11-07T14:03:11Z",
                FrozenParts {
                    engine: engine(),
                    sources: origins.iter().map(|o| Source::new(o, "m = 1").unwrap()).collect(),
                    witness_scripts: scripts.iter().map(|n| WitnessScript::new(n, "1 m").unwrap()).collect(),
                    witnesses: Pairs::from_vec(scripts.iter().map(|n| (format!("user.{n}"), "x".to_string())).collect())
                        .unwrap_or_default(),
                    ..FrozenParts::default()
                },
                input("freeze", "r", "2026-11-07T14:03:11Z"),
            )
        };
        for (origins, expected) in [
            (&["embedded", "overrides", "embedded"][..], "origin embedded is used by more than one source"),
            (&["mine", "overrides"][..], "source 1 has the wrong origin: expected embedded, found mine"),
            (&["embedded", "mine"][..], "source 2 has the wrong origin: expected overrides, found mine"),
            (&["embedded", "overrides", "x", "x"][..], "origin x is used by more than one source"),
            (&["embedded"][..], "expected overrides, found no source"),
            (&[][..], "expected embedded, found no source"),
        ] {
            let message = make(origins, &[]).unwrap_err().to_string();
            assert!(message.contains(expected), "{origins:?}: {message}");
        }
        assert!(make(&["embedded", "overrides", "system"], &[]).is_ok());
        let repeated = make(&["embedded", "overrides"], &["w", "w"]);
        assert!(repeated.unwrap_err().to_string().contains("witness w appears more than once"));
        // the same files reached by parsing text: verify reports them
        let text = fresh().render().unwrap();
        for (from, to, expected) in [
            ("origin=overrides", "origin=mine", "source 2 has the wrong origin"),
            ("origin=embedded", "origin=overrides", "the origin overrides is used by more than one source"),
            ("origin=overrides", "origin=embedded", "the origin embedded is used by more than one source"),
        ] {
            let mut file = FrozenFile::parse(&text.replace(from, to)).unwrap();
            assert!(problems(&file).iter().any(|m| m.contains(expected)), "{to}: {:?}", problems(&file));
            assert!(file.reseal(input("seal", "r", "2026-11-08T09:00:00Z")).is_err(), "a reseal cannot accept it");
        }
        let twice = format!("{text}@@ witness validation.1 sha256={}\n1 m => ft\n@@ end witness\n", exact_sha256("1 m => ft"));
        let p = problems(&FrozenFile::parse(&twice).unwrap());
        assert!(p.iter().any(|m| m.contains("witness validation.1 appears more than once")), "{p:?}");
    }

    #[test]
    fn witness_keys_and_scripts_must_agree() {
        let text = fresh().render().unwrap();
        // a script without its key
        let p = problems(&FrozenFile::parse(&text.replace("user.validation.1 = 1a2b\n", "")).unwrap());
        assert!(p.iter().any(|m| m.contains("expected [user.validation.1], found []")), "{p:?}");
        // a key without its script
        let cut = text.find("@@ witness").unwrap();
        let p = problems(&FrozenFile::parse(&text[..cut]).unwrap());
        assert!(p.iter().any(|m| m.contains("expected [], found [user.validation.1]")), "{p:?}");
        // a key naming another script
        let p = problems(&FrozenFile::parse(&text.replace("user.validation.1 =", "user.validation.2 =")).unwrap());
        assert!(p.iter().any(|m| m.contains("expected [user.validation.1], found [user.validation.2]")), "{p:?}");
        // built in memory: refused
        let made = FrozenFile::new(
            "2026-11-07T14:03:11Z",
            FrozenParts {
                engine: engine(),
                sources: vec![Source::new("embedded", "").unwrap(), Source::new("overrides", "").unwrap()],
                witness_scripts: vec![WitnessScript::new("a", "1 m").unwrap()],
                witnesses: pairs(&[("builtin.x", "1")]),
                ..FrozenParts::default()
            },
            input("freeze", "r", "2026-11-07T14:03:11Z"),
        );
        assert!(made.unwrap_err().to_string().contains("expected [user.a], found []"));
        // set_witness_scripts without the key: the reseal refuses
        let mut file = fresh();
        file.set_witness_scripts(vec![WitnessScript::new("validation.1", "1 m => ft").unwrap(), WitnessScript::new("b", "2 m").unwrap()]);
        assert!(file.reseal(input("seal", "r", "2026-11-08T09:00:00Z")).is_err());
        let mut keys = file.witnesses().as_slice().to_vec();
        keys.push(("user.b".into(), "ff".into()));
        file.set_witnesses(Pairs::from_vec(keys).unwrap());
        file.reseal(input("seal", "added b", "2026-11-08T09:00:00Z")).unwrap();
        assert!(problems(&file).is_empty());
    }

    /// Recomputes every entry hash, link and the seal: a structurally wrong file whose hashes all agree.
    fn rechain(file: &mut FrozenFile) {
        let mut prev = String::new();
        for entry in &mut file.history {
            entry.prev_entry_sha256 = prev.clone();
            entry.entry_sha256 = entry_sha256(entry);
            prev = entry.entry_sha256.clone();
        }
        file.content_sha256 = file.computed_content_sha256();
        let content = file.content_sha256.clone();
        file.seal_sha256 = seal_sha256(file, &content, &prev);
    }

    #[test]
    fn the_history_must_be_continuous_and_start_with_a_freeze() {
        let base = two_entries();
        assert!(problems(&base).is_empty());
        let edits: Vec<(&str, &str, Box<dyn Fn(&mut FrozenFile)>)> = vec![
            ("first action", "History.1 has the wrong action: expected freeze", Box::new(|f| f.history[0].action = "update".into())),
            ("first old hash", "History.1 does not start from", Box::new(|f| f.history[0].old_content_sha256 = "a".repeat(64))),
            ("continuity", "History.2 does not start from", Box::new(|f| f.history[1].old_content_sha256 = "a".repeat(64))),
            ("later action", "History.2 has the wrong action: expected update or seal", Box::new(|f| f.history[1].action = "freeze".into())),
            ("bad action", "History.2 has the wrong action", Box::new(|f| f.history[1].action = "frobnicate".into())),
            ("empty reason", "found 0", Box::new(|f| f.history[1].reason = String::new())),
            ("long reason", "found 501", Box::new(|f| f.history[1].reason = "x".repeat(501))),
        ];
        for (what, expected, edit) in edits {
            let mut file = base.clone();
            edit(&mut file);
            rechain(&mut file);
            let p = problems(&file);
            assert!(p.iter().any(|m| m.contains(expected)), "{what}: {p:?}");
        }
    }

    /// What the seal is meant to protect: everything but `[Environment]` and formatting (pair order, comments and
    /// blank lines, and in sources whatever the conf canonical form drops).
    fn meaning(file: &FrozenFile) -> impl PartialEq + std::fmt::Debug {
        let sorted = |p: &Pairs| {
            let mut v = p.as_slice().to_vec();
            v.sort();
            v
        };
        (
            file.format,
            file.created.clone(),
            sorted(&file.engine),
            sorted(&file.witnesses),
            file.sources.iter().map(|s| (s.origin.clone(), crate::frozen::canonical::canonical_text(&s.text))).collect::<Vec<_>>(),
            file.witness_scripts.iter().map(|s| (s.name.clone(), s.text.clone())).collect::<Vec<_>>(),
            file.history.clone(),
        )
    }

    #[test]
    fn every_one_byte_edit_is_refused_detected_or_meaningless() {
        let original = two_entries();
        let text = original.render().unwrap();
        let want = meaning(&original);
        let mut tried = 0;
        for at in 0..text.len() {
            for replacement in ["", "x", "0", "f", " ", "\n", "#", ";", "@", "=", "\n\n"] {
                let edited = format!("{}{replacement}{}", &text[..at], &text[at + 1..]);
                if edited == text {
                    continue;
                }
                tried += 1;
                let Ok(file) = FrozenFile::parse(&edited) else { continue };
                if file.verify().is_empty() {
                    assert_eq!(meaning(&file), want, "byte {at} -> {replacement:?} changed the meaning unnoticed");
                }
            }
        }
        assert!(tried > 10_000);
    }

    /// The seal of one small file, computed by hand outside Rust (Python's hashlib) from the seal text written in
    /// the spec §4.1 and the entry text of §5.3 (with `engine_sha256`, `policy` and `pinned` right after
    /// `engine`). If this fails, the code and the spec disagree.
    #[test]
    fn the_seal_matches_an_independent_computation() {
        let file = FrozenFile::new(
            "2026-11-07T14:03:11Z",
            FrozenParts {
                engine: engine(),
                environment: pairs(&[("os", "windows")]),
                witnesses: pairs(&[("user.v1", "abc")]),
                sources: vec![Source::new("embedded", "[Units]\nm = 1").unwrap(), Source::new("overrides", "").unwrap()],
                witness_scripts: vec![WitnessScript::new("v1", "x = 1 m\nx => ft").unwrap()],
            },
            EntryInput {
                time: "2026-11-07T14:03:11Z",
                action: "freeze",
                engine: "0.3.1+9f2c1ab",
                actor: "irvin@host",
                reason: "initial freeze",
                diff: "added 1",
            },
        )
        .unwrap();
        assert_eq!(file.content_sha256(), INDEPENDENT_CONTENT);
        assert_eq!(block_sha256(file.engine()), INDEPENDENT_ENGINE);
        assert_eq!(file.history()[0].entry_sha256(), INDEPENDENT_ENTRY);
        assert_eq!(file.seal_sha256(), INDEPENDENT_SEAL);
    }

    const INDEPENDENT_CONTENT: &str = "9c0d6a0d38b9562d9227f69c582e48040958a2417745956a6cfc9f5150ca77b7";
    const INDEPENDENT_ENGINE: &str = "a3727508d1f58d898990018132fa83fcb072dcb8e4a25e38c5a776a8572177b6";
    const INDEPENDENT_ENTRY: &str = "5d3ce7e958d3ad893dcf0d5175e0f66a65cbe862c5d5eafc37364a1f6b762690";
    const INDEPENDENT_SEAL: &str = "9fad9250d221770925eb42f8a3c337681d79319edb1f95502f7bc59e4ae03ced";

    // ---- Phase 1b, Task 5: the history records the engine block; verify reads it ----

    fn has(file: &FrozenFile, wanted: impl Fn(&Problem) -> bool) -> bool {
        file.verify().iter().any(wanted)
    }

    #[test]
    fn a_new_file_records_the_engine_block_in_its_first_entry() {
        let file = fresh();
        assert_eq!(file.history()[0].engine_sha256(), block_sha256(file.engine()));
        assert!(is_sha256_hex(file.history()[0].engine_sha256()));
    }

    #[test]
    fn changing_the_engine_and_resealing_shows_in_the_log() {
        let mut file = fresh_with(engine_block(EnginePolicy::Build));
        file.set_engine(engine_block(EnginePolicy::Record));
        // the caller's free-text diff says nothing about the engine: the log must show it anyway
        file.reseal(EntryInput {
            time: "2026-11-08T09:00:00Z",
            action: "seal",
            engine: "0.3.1+0123456789ab",
            actor: "irvin@host",
            reason: "weaken",
            diff: "n/a",
        })
        .unwrap();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        let (first, second) = (&file.history()[0], &file.history()[1]);
        assert_eq!(first.new_content_sha256(), second.new_content_sha256(), "only the engine changed");
        assert_ne!(first.engine_sha256(), second.engine_sha256());
        assert_eq!(first.engine_sha256(), block_sha256(&engine_block(EnginePolicy::Build)));
        assert_eq!(second.engine_sha256(), block_sha256(&engine_block(EnginePolicy::Record)));
        // hiding the weakening by dropping its entry and recomputing every hash and the seal: the log betrays it
        let mut hidden = file.clone();
        hidden.history.pop();
        rechain(&mut hidden);
        assert_eq!(
            hidden.verify(),
            vec![
                Problem::HistoryEngine {
                    expected: block_sha256(&engine_block(EnginePolicy::Record)),
                    found: block_sha256(&engine_block(EnginePolicy::Build)),
                },
                Problem::HistoryPin { field: "policy".into(), expected: "record".into(), found: "build".into() },
            ]
        );
    }

    #[test]
    fn resealing_without_changing_the_engine_keeps_the_hash() {
        let file = two_entries();
        assert!(problems(&file).is_empty());
        let (first, second) = (&file.history()[0], &file.history()[1]);
        assert_ne!(first.new_content_sha256(), second.new_content_sha256(), "the overrides changed");
        assert_eq!(first.engine_sha256(), second.engine_sha256());
        assert_eq!(second.engine_sha256(), block_sha256(file.engine()));
    }

    #[test]
    fn the_last_entry_must_record_the_current_engine_block() {
        // the block replaced without resealing
        let mut file = fresh();
        let recorded = file.history[0].engine_sha256.clone();
        file.engine = engine_block(EnginePolicy::Record);
        let current = block_sha256(&file.engine);
        assert!(
            has(&file, |p| *p == Problem::HistoryEngine { expected: current.clone(), found: recorded.clone() }),
            "{:?}",
            problems(&file)
        );
        assert!(has(&file, |p| matches!(p, Problem::Seal { .. })), "{:?}", problems(&file));
        // the same, by someone who recomputes every hash and the seal but leaves the log alone: the log betrays it
        rechain(&mut file);
        assert_eq!(
            file.verify(),
            vec![
                Problem::HistoryEngine { expected: current, found: recorded },
                Problem::HistoryPin { field: "policy".into(), expected: "record".into(), found: "release".into() },
            ]
        );

        // the other way: the last entry's engine_sha256 forged in the text
        let original = fresh();
        let text = original.render().unwrap();
        let line = format!("engine_sha256 = {}", original.history[0].engine_sha256);
        assert!(text.contains(&line));
        let forged = FrozenFile::parse(&text.replace(&line, &format!("engine_sha256 = {}", "0".repeat(64)))).unwrap();
        assert!(has(&forged, |p| matches!(p, Problem::HistoryEntryHash { n: 1, .. })), "{:?}", problems(&forged));
        assert!(has(&forged, |p| matches!(p, Problem::HistoryEngine { .. })), "{:?}", problems(&forged));
        // and forged with every hash recomputed: still caught
        let mut rehashed = original.clone();
        rehashed.history[0].engine_sha256 = "0".repeat(64);
        rechain(&mut rehashed);
        assert_eq!(
            rehashed.verify(),
            vec![Problem::HistoryEngine { expected: block_sha256(&rehashed.engine), found: "0".repeat(64) }]
        );
    }

    #[test]
    fn an_unreadable_engine_block_is_a_problem() {
        let toy = pairs(&[("version", "0.3.1"), ("policy", "build")]);
        let mut file = fresh();
        file.engine = toy.clone();
        let named = |p: &Problem| matches!(p, Problem::Engine { message } if message.contains("commit"));
        assert!(has(&file, named), "{:?}", problems(&file));
        // fully rehashed, the block is still refused, and the problem survives a trip through text
        file.history[0].engine_sha256 = block_sha256(&toy);
        rechain(&mut file);
        assert_eq!(file.verify().len(), 1, "{:?}", problems(&file));
        assert!(has(&FrozenFile::parse(&file.render().unwrap()).unwrap(), named));
        // it can be neither made nor resealed
        let made = FrozenFile::new(
            "2026-11-07T14:03:11Z",
            FrozenParts {
                engine: toy.clone(),
                sources: vec![Source::new("embedded", "").unwrap(), Source::new("overrides", "").unwrap()],
                ..FrozenParts::default()
            },
            input("freeze", "r", "2026-11-07T14:03:11Z"),
        );
        assert!(made.unwrap_err().to_string().contains("[Engine]"));
        let mut sealed = fresh();
        sealed.set_engine(toy);
        assert!(sealed.reseal(input("seal", "r", "2026-11-08T09:00:00Z")).is_err());
        assert_eq!(sealed.history.len(), 1, "a refused reseal leaves the history as it was");
        // an unknown key in a block that is otherwise valid
        let mut knob = engine().as_slice().to_vec();
        knob.push(("enforce".into(), "never".into()));
        let mut file = fresh();
        file.engine = Pairs::from_vec(knob).unwrap();
        assert!(has(&file, |p| matches!(p, Problem::Engine { message } if message.contains("enforce"))));
    }

    // ---- Phase 1b, Task 8: the block must be recordable; the log names the policy and the pinned engine ----

    /// A block `engine_pairs` would refuse, written by hand: `commit` and `policy` replaced in a valid block.
    fn raw_block(commit: &str, policy: &str) -> Pairs {
        let base = engine_block(EnginePolicy::Release);
        Pairs::from_vec(
            base.as_slice()
                .iter()
                .map(|(k, v)| match k.as_str() {
                    "commit" => (k.clone(), commit.to_string()),
                    "policy" => (k.clone(), policy.to_string()),
                    _ => (k.clone(), v.clone()),
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn a_block_that_could_not_have_been_recorded_is_a_problem() {
        let unknown_build = raw_block("unknown", "build");
        let unknown_binary = raw_block("unknown", "binary");
        let binary_without_hash = raw_block("0123456789abcdef0123456789abcdef01234567", "binary");
        for (what, block, needle) in [
            ("build with an unknown commit", &unknown_build, "commit"),
            ("binary with an unknown commit", &unknown_binary, "commit"),
            ("binary without a hash", &binary_without_hash, "binary_sha256"),
        ] {
            // readable, so only the new rule can refuse it
            assert!(read_engine(block).is_ok(), "{what}");
            let made = FrozenFile::new(
                "2026-11-07T14:03:11Z",
                FrozenParts {
                    engine: block.clone(),
                    sources: vec![Source::new("embedded", "").unwrap(), Source::new("overrides", "").unwrap()],
                    ..FrozenParts::default()
                },
                input("freeze", "r", "2026-11-07T14:03:11Z"),
            );
            assert!(made.is_err(), "{what}: made");
            let mut sealed = fresh();
            sealed.set_engine(block.clone());
            assert!(sealed.reseal(input("seal", "routine", "2026-11-08T09:00:00Z")).is_err(), "{what}: resealed");
            assert_eq!(sealed.history.len(), 1);
            // written by hand with every hash recomputed: verify still reports it, and only it
            let mut forged = fresh();
            forged.engine = block.clone();
            forged.history[0].engine_sha256 = block_sha256(block);
            forged.history[0].policy = block.get("policy").unwrap().to_string();
            forged.history[0].pinned = read_engine(block).unwrap().identity.short();
            rechain(&mut forged);
            let p = forged.verify();
            assert_eq!(p.len(), 1, "{what}: {p:?}");
            assert!(matches!(&p[0], Problem::Engine { message } if message.contains(needle)), "{what}: {p:?}");
        }
    }

    #[test]
    fn each_entry_records_the_policy_and_the_pinned_engine_of_its_block() {
        let mut file = fresh_with(engine_block(EnginePolicy::Build));
        // weakened with an innocuous reason and diff, and a made-up acting engine
        file.set_engine(engine_block(EnginePolicy::Record));
        file.reseal(EntryInput {
            time: "2026-11-08T09:00:00Z",
            action: "seal",
            engine: "9.9.9+ffffffffffff",
            actor: "a@h",
            reason: "routine reseal",
            diff: "no changes",
        })
        .unwrap();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        let (first, second) = (&file.history()[0], &file.history()[1]);
        assert_eq!((first.policy(), first.pinned()), ("build", "0.3.1+0123456789ab"));
        assert_eq!((second.policy(), second.pinned()), ("record", "0.3.1+0123456789ab"));
        assert_eq!(second.engine(), "9.9.9+ffffffffffff", "the acting engine stays what the caller wrote");
        // rendered right after engine_sha256, and parsed back
        let text = file.render().unwrap();
        let expected = format!("engine_sha256 = {}\npolicy = record\npinned = 0.3.1+0123456789ab\nactor = a@h\n", second.engine_sha256());
        assert!(text.contains(&expected), "{text}");
        assert_eq!(FrozenFile::parse(&text).unwrap(), file);
        // inside the entry hash: editing either in the text breaks that entry
        for (from, to) in [
            ("policy = record\npinned", "policy = build\npinned"),
            ("pinned = 0.3.1+0123456789ab\nactor = a@h", "pinned = 0.3.1+fedcba987654\nactor = a@h"),
        ] {
            let edited = FrozenFile::parse(&text.replacen(from, to, 1)).unwrap();
            assert!(has(&edited, |p| matches!(p, Problem::HistoryEntryHash { n: 2, .. })), "{to}: {:?}", problems(&edited));
        }
        // the block decides what the entry says: a rewritten entry with every hash recomputed is still caught
        let mut moved = file.clone();
        moved.history[1].policy = "build".into();
        rechain(&mut moved);
        assert_eq!(moved.verify(), vec![Problem::HistoryPin { field: "policy".into(), expected: "record".into(), found: "build".into() }]);
        let mut moved = file.clone();
        moved.history[1].pinned = "0.3.1+fedcba987654".into();
        rechain(&mut moved);
        assert_eq!(
            moved.verify(),
            vec![Problem::HistoryPin { field: "pinned".into(), expected: "0.3.1+0123456789ab".into(), found: "0.3.1+fedcba987654".into() }]
        );
    }

    #[test]
    fn the_policy_and_pinned_fields_are_parsed_strictly() {
        let text = two_entries().render().unwrap();
        // edits inside [History.2] only ([Engine] has a `policy` line of its own)
        let at = text.find("[History.2]").unwrap();
        let (head, entry) = text.split_at(at);
        let edited = |from: &str, to: &str| format!("{head}{}", entry.replacen(from, to, 1));
        for key in ["policy", "pinned"] {
            let line = entry.lines().find(|l| l.starts_with(&format!("{key} = "))).unwrap().to_string() + "\n";
            let message = FrozenFile::parse(&edited(&line, "")).unwrap_err().to_string();
            assert!(message.starts_with("line ") && message.contains(&format!("[History.2] has no {key}")), "{message}");
            let message = FrozenFile::parse(&edited(&line, &format!("{key} =\n"))).unwrap_err().to_string();
            assert!(message.starts_with("line ") && message.contains(&format!("[History.2] {key}")), "{message}");
        }
        let message = FrozenFile::parse(&edited("policy = release", "policy = exact")).unwrap_err().to_string();
        assert!(message.starts_with("line ") && message.contains("exact"), "{message}");
        // the same shapes, built in memory, cannot be rendered
        for (key, value) in [("policy", ""), ("policy", "exact"), ("pinned", "")] {
            let mut file = two_entries();
            if key == "policy" {
                file.history[1].policy = value.into();
            } else {
                file.history[1].pinned = value.into();
            }
            assert!(file.render().unwrap_err().to_string().contains(key), "{key} = {value:?}");
            assert!(has(&file, |p| matches!(p, Problem::Framing { found } if found.contains(key))), "{key} = {value:?}");
        }
    }

    // ---- Phase 1c, Task 4: the history is chronological (residual risk 2 of spec §13) ----

    #[test]
    fn an_entry_earlier_than_the_previous_one_is_a_problem() {
        let mut file = two_entries();
        let (t1, t2) = (file.history[0].time.clone(), file.history[1].time.clone());
        assert!(t1 < t2);
        file.history[0].time = t2.clone();
        file.history[1].time = t1.clone();
        file.created = t2.clone();
        // every hash, link and the seal recomputed: only the time rule can catch it
        rechain(&mut file);
        assert_eq!(file.verify(), vec![Problem::HistoryTime { n: 2, previous: t2, found: t1 }]);
    }

    #[test]
    fn an_entry_earlier_than_created_is_a_problem() {
        let mut file = fresh();
        file.created = "2026-11-07T14:03:12Z".into();
        rechain(&mut file);
        let p = file.verify();
        assert!(
            p.contains(&Problem::HistoryTime { n: 1, previous: "2026-11-07T14:03:12Z".into(), found: "2026-11-07T14:03:11Z".into() }),
            "{p:?}"
        );
    }

    #[test]
    fn equal_times_are_allowed_and_a_reseal_cannot_go_back_in_time() {
        let mut file = fresh();
        file.set_source_text(1, "kg = 5").unwrap();
        file.reseal(input("seal", "same second", "2026-11-07T14:03:11Z")).unwrap();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        let mut file = fresh();
        file.set_source_text(1, "kg = 5").unwrap();
        let message = file.reseal(input("seal", "back", "2026-11-07T14:03:10Z")).unwrap_err().to_string();
        assert!(message.contains("History.2") && message.contains("earlier"), "{message}");
        assert_eq!(file.history.len(), 1);
    }

    // ---- Phase 1c, Task 9 F19: `seal` proves positively that only the overrides changed ----

    fn overrides_edited() -> String {
        fresh().render().unwrap().replace("# mine\n", "# mine\nzz = 1.0, L\n")
    }

    fn only_overrides(text: &str) -> bool {
        FrozenFile::parse(text).unwrap().only_the_overrides_changed()
    }

    #[test]
    fn an_edit_of_the_overrides_alone_is_recognised() {
        assert!(only_overrides(&overrides_edited()));
        // the environment is outside the seal
        assert!(only_overrides(&overrides_edited().replace("os = windows", "os = linux")));
        // and it is exactly what verify would complain about
        let file = FrozenFile::parse(&overrides_edited()).unwrap();
        assert!(file.verify().iter().all(|p| matches!(p, Problem::SourceHash { index: 2, .. } | Problem::ContentHash { .. } | Problem::Seal { .. })));
    }

    #[test]
    fn a_clean_file_or_an_edit_of_comments_is_not_an_edit_of_the_overrides() {
        let text = fresh().render().unwrap();
        assert!(!only_overrides(&text));
        let comments = text.replace("# mine\n", "# mine, reworded\n   \n; more\n");
        assert!(FrozenFile::parse(&comments).unwrap().verify().is_empty());
        assert!(!only_overrides(&comments));
    }

    #[test]
    fn an_edit_of_anything_else_is_not_an_edit_of_the_overrides() {
        let base = overrides_edited();
        let original = fresh();
        let s1 = original.sources[0].sha256.clone();
        let s2 = original.sources[1].sha256.clone();
        let new_s1 = source_sha256("[Units]\nm = 1\nkg = 3");
        let new_s2 = source_sha256("# mine\nzz = 1.0, L");
        let extra = format!("@@ source 3 origin=evil sha256={}\nkg = 9\n@@ end source 3\n", source_sha256("kg = 9"));
        let at = base.find("@@ witness").unwrap();
        let appended = format!("{}{extra}{}", &base[..at], &base[at..]);
        let content = original.content_sha256.clone();
        let cases: Vec<(&str, String)> = vec![
            // the measured unsound cases of the review: [ContentHash, Seal] and [Seal] only
            ("source 1 edited with its marker recomputed", base.replace("kg = 2", "kg = 3").replace(&s1, &new_s1)),
            ("a third source appended with a correct marker", appended),
            ("a [Witness] value", base.replace("3ff0000000000000", "3ff0000000000001")),
            ("a witness script", base.replace("1 m => ft", "1 m => yd")),
            ("a history field", base.replace("reason = initial freeze", "reason = something else")),
            ("the engine block", base.replace("version = 0.3.1", "version = 0.3.2")),
            ("the stored content hash", base.replace(&format!("content_sha256 = {content}"), &format!("content_sha256 = {}", "0".repeat(64)))),
            ("the marker of the overrides", base.replace(&s2, &new_s2)),
            ("created", base.replace("created = 2026-11-07T14:03:11Z", "created = 2026-11-07T14:03:12Z")),
            ("source 1 edited without its marker", base.replace("kg = 2", "kg = 3")),
        ];
        for (what, text) in cases {
            assert_ne!(text, base, "{what}: the edit did not apply");
            assert!(!only_overrides(&text), "{what}");
        }
        // the witness block deleted together with its key
        let cut = base.find("@@ witness").unwrap();
        let deleted = base[..cut].replace("user.validation.1 = 1a2b\n", "");
        assert!(!only_overrides(&deleted), "witness deleted");
        // a history entry rewritten with every hash and the seal recomputed
        let mut forged = FrozenFile::parse(&base).unwrap();
        forged.history[0].reason = "rewritten".into();
        rechain(&mut forged);
        assert!(!forged.only_the_overrides_changed(), "history rechained");
        // a resealed file whose overrides were then edited again: still recognised
        let mut resealed = FrozenFile::parse(&base).unwrap();
        resealed.reseal(input("seal", "accepted", "2026-11-08T09:00:00Z")).unwrap();
        let again = resealed.render().unwrap().replace("zz = 1.0, L\n", "zz = 2.0, L\n");
        assert!(only_overrides(&again));
    }

    #[test]
    fn the_render_parse_round_trip_keeps_engine_sha256_and_the_independent_seal_check_is_updated() {
        let mut file = fresh_with(engine_block(EnginePolicy::Build));
        file.set_engine(engine_block(EnginePolicy::Release));
        file.reseal(input("update", "relaxed the pin", "2026-11-08T09:00:00Z")).unwrap();
        let text = file.render().unwrap();
        for entry in file.history() {
            assert!(text.contains(&format!("engine_sha256 = {}\n", entry.engine_sha256())));
        }
        let again = FrozenFile::parse(&text).unwrap();
        assert_eq!(again, file);
        assert!(again.verify().is_empty());
        // the field sits right after `engine` in each entry
        assert!(text.contains(&format!("engine = 0.3.1+9f2c1ab\nengine_sha256 = {}\n", file.history()[1].engine_sha256())));
        // missing or malformed: a parse error that names the line
        let line = format!("engine_sha256 = {}\n", file.history()[1].engine_sha256());
        let missing = FrozenFile::parse(&text.replace(&line, "")).unwrap_err().to_string();
        assert!(missing.starts_with("line ") && missing.contains("[History.2] has no engine_sha256"), "{missing}");
        for bad in ["", "xyz", &"A".repeat(64)] {
            let message = FrozenFile::parse(&text.replace(&line, &format!("engine_sha256 = {bad}\n"))).unwrap_err().to_string();
            assert!(message.starts_with("line ") && message.contains("engine_sha256 is not a SHA-256"), "{bad:?}: {message}");
        }
    }
}
