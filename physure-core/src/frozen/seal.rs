//! Building, verifying and re-sealing a frozen file (spec §4.1, §5.3).

use std::fmt;

use super::canonical::{canonical_pairs, content_sha256, source_sha256};
use super::model::{FrozenError, FrozenFile, FrozenResult, HistoryEntry, Source, WitnessScript, FORMAT_VERSION};
use super::sha256::sha256_hex;
use super::time::is_rfc3339_utc;

/// The facts of one history entry that the caller knows. The model never reads a clock or the environment.
pub struct EntryInput<'a> {
    pub time: &'a str,
    /// `freeze`, `update` or `seal`.
    pub action: &'a str,
    /// The running engine, short form (`0.3.1+9f2c1ab`).
    pub engine: &'a str,
    /// `user@host`: informational, not authenticated.
    pub actor: &'a str,
    /// Why: 1 to 500 characters on one line.
    pub reason: &'a str,
    /// A one-line summary of what changed.
    pub diff: &'a str,
}

fn one_line(what: &str, value: &str) -> FrozenResult<()> {
    if value.contains('\n') || value.contains('\r') || value != value.trim() {
        return Err(FrozenError(format!("{what} must be one line without leading or trailing whitespace")));
    }
    Ok(())
}

impl EntryInput<'_> {
    fn validate(&self) -> FrozenResult<()> {
        if !matches!(self.action, "freeze" | "update" | "seal") {
            return Err(FrozenError(format!("action must be freeze, update or seal, not {:?}", self.action)));
        }
        if !is_rfc3339_utc(self.time) {
            return Err(FrozenError(format!("time must be a UTC time like 2026-11-07T14:03:11Z, not {:?}", self.time)));
        }
        let length = self.reason.chars().count();
        if length == 0 || length > 500 {
            return Err(FrozenError("the reason must be 1 to 500 characters".into()));
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
    EmptyHistory,
    SourceHash { index: usize, origin: String, expected: String, found: String },
    ContentHash { expected: String, found: String },
    WitnessScriptHash { name: String, expected: String, found: String },
    HistoryNumber { position: usize, found: u32 },
    HistoryPrev { n: u32, expected: String, found: String },
    HistoryEntryHash { n: u32, expected: String, found: String },
    HistoryTip { expected: String, found: String },
    Seal { expected: String, found: String },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Format { found } => write!(f, "unsupported format {found}"),
            Problem::EmptyHistory => write!(f, "the history is empty"),
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
                write!(f, "history entry at position {position} is numbered {found}")
            }
            Problem::HistoryPrev { n, expected, found } => {
                write!(f, "History.{n} does not follow the previous entry: expected {expected}, found {found}")
            }
            Problem::HistoryEntryHash { n, expected, found } => {
                write!(f, "History.{n} was modified: expected {expected}, found {found}")
            }
            Problem::HistoryTip { expected, found } => {
                write!(f, "the last history entry does not record the current content: expected {expected}, found {found}")
            }
            Problem::Seal { expected, found } => {
                write!(f, "the seal does not match the content, engine, witnesses and history: expected {expected}, found {found}")
            }
        }
    }
}

fn entry_sha256(entry: &HistoryEntry) -> String {
    let text = format!(
        "n={}\ntime={}\naction={}\nengine={}\nactor={}\nreason={}\nold_content_sha256={}\nnew_content_sha256={}\ndiff={}\nprev_entry_sha256={}",
        entry.n, entry.time, entry.action, entry.engine, entry.actor, entry.reason, entry.old_content_sha256,
        entry.new_content_sha256, entry.diff, entry.prev_entry_sha256
    );
    sha256_hex(text.as_bytes())
}

fn seal_sha256(file: &FrozenFile, content: &str, last_entry: &str) -> String {
    let text = format!(
        "physure-frozen-v1\n{content}\n{}\n{}\n{last_entry}",
        canonical_pairs(&file.engine),
        canonical_pairs(&file.witnesses)
    );
    sha256_hex(text.as_bytes())
}

impl FrozenFile {
    /// The content hash computed from the texts of the sources as they are now.
    pub fn computed_content_sha256(&self) -> String {
        let hashes: Vec<(String, String)> =
            self.sources.iter().map(|s| (s.origin.clone(), source_sha256(&s.text))).collect();
        content_sha256(&hashes)
    }

    /// A new frozen file with its first history entry (`freeze`). All hashes are computed.
    pub fn new(
        created: &str,
        engine: Vec<(String, String)>,
        environment: Vec<(String, String)>,
        witnesses: Vec<(String, String)>,
        sources: Vec<Source>,
        witness_scripts: Vec<WitnessScript>,
        first: EntryInput<'_>,
    ) -> FrozenResult<FrozenFile> {
        if first.action != "freeze" {
            return Err(FrozenError("the first history entry of a file is a freeze".into()));
        }
        first.validate()?;
        if !is_rfc3339_utc(created) {
            return Err(FrozenError(format!("created must be a UTC time like 2026-11-07T14:03:11Z, not {created:?}")));
        }
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
        Ok(file)
    }

    /// Accepts the current state of the sources, the engine and the witnesses (after an edit by hand or an
    /// update): recomputes every hash, appends a chained history entry whose old content hash is the one stored
    /// before, and seals again.
    pub fn reseal(&mut self, entry: EntryInput<'_>) -> FrozenResult<()> {
        entry.validate()?;
        self.append_entry(entry);
        Ok(())
    }

    fn append_entry(&mut self, input: EntryInput<'_>) {
        for source in &mut self.sources {
            source.sha256 = source_sha256(&source.text);
        }
        for script in &mut self.witness_scripts {
            script.sha256 = source_sha256(&script.text);
        }
        let old = std::mem::take(&mut self.content_sha256);
        self.content_sha256 = self.computed_content_sha256();
        let prev = self.history.last().map(|e| e.entry_sha256.clone()).unwrap_or_default();
        let mut entry = HistoryEntry {
            n: self.history.len() as u32 + 1,
            time: input.time.to_string(),
            action: input.action.to_string(),
            engine: input.engine.to_string(),
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

    /// Everything that is wrong with the file, not only the first thing. Empty means the file is intact.
    pub fn verify(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.format != FORMAT_VERSION {
            problems.push(Problem::Format { found: self.format });
        }
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
            let computed = source_sha256(&script.text);
            if computed != script.sha256 {
                problems.push(Problem::WitnessScriptHash {
                    name: script.name.clone(),
                    expected: script.sha256.clone(),
                    found: computed,
                });
            }
        }
        let mut previous = String::new();
        for (position, entry) in self.history.iter().enumerate() {
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
            previous = entry.entry_sha256.clone();
        }
        let Some(last) = self.history.last() else {
            problems.push(Problem::EmptyHistory);
            return problems;
        };
        if last.new_content_sha256 != self.content_sha256 {
            problems.push(Problem::HistoryTip { expected: self.content_sha256.clone(), found: last.new_content_sha256.clone() });
        }
        let seal = seal_sha256(self, &content, &last.entry_sha256);
        if seal != self.seal_sha256 {
            problems.push(Problem::Seal { expected: self.seal_sha256.clone(), found: seal });
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frozen::model::{FrozenFile, Source, WitnessScript};

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn input<'a>(action: &'a str, reason: &'a str, time: &'a str) -> EntryInput<'a> {
        EntryInput { time, action, engine: "0.3.1+9f2c1ab", actor: "irvin@host", reason, diff: "added 2" }
    }

    fn fresh() -> FrozenFile {
        FrozenFile::new(
            "2026-11-07T14:03:11Z",
            pairs(&[("version", "0.3.1"), ("policy", "build")]),
            pairs(&[("os", "windows")]),
            pairs(&[("builtin.conversion", "3ff0000000000000")]),
            vec![Source::new("embedded", "[Units]\nm = 1\nkg = 2").unwrap(), Source::new("overrides", "# mine\n").unwrap()],
            vec![WitnessScript::new("validation.1", "1 m => ft").unwrap()],
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
    }

    #[test]
    fn a_history_entry_cannot_be_edited() {
        let p = problems(&reparsed_with(|t| t.replace("initial freeze", "nothing to see")));
        assert!(p.iter().any(|m| m.contains("History.1")), "{p:?}");
    }

    fn two_entries() -> FrozenFile {
        let mut file = fresh();
        file.sources[1] = Source::new("overrides", "kg = 5").unwrap();
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
    }

    #[test]
    fn the_entry_input_is_validated() {
        let mut file = fresh();
        for bad in [
            input("frobnicate", "r", "2026-11-08T09:00:00Z"),
            input("update", "", "2026-11-08T09:00:00Z"),
            input("update", "two\nlines", "2026-11-08T09:00:00Z"),
            input("update", "r", "yesterday"),
        ] {
            assert!(file.reseal(bad).is_err());
        }
        assert!(file.reseal(input("update", &"x".repeat(501), "2026-11-08T09:00:00Z")).is_err());
        assert!(file.reseal(input("update", &"x".repeat(500), "2026-11-08T09:00:00Z")).is_ok());
        assert!(FrozenFile::new("2026-11-07T14:03:11Z", vec![], vec![], vec![], vec![], vec![],
                                input("update", "r", "2026-11-07T14:03:11Z")).is_err(), "the first entry is a freeze");
    }

    #[test]
    fn problems_say_what_was_expected_and_what_was_found() {
        let p = problems(&reparsed_with(|t| t.replace("kg = 2", "kg = 3")));
        let source = p.iter().find(|m| m.contains("source 1")).unwrap();
        assert!(source.contains("expected") && source.contains("found"), "{source}");
    }
}
