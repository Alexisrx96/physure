//! The frozen file as data, and its strict renderer and parser (spec §4).

use std::collections::{HashMap, HashSet};
use std::fmt;

use super::canonical::{exact_sha256, is_sha256_hex, source_sha256};
use super::engine::EnginePolicy;
use super::time::is_rfc3339_utc;

pub const FORMAT_VERSION: u32 = 1;
pub const MAGIC: &str = "#physure-frozen 1";
/// The most sources a file may have: a source is replayed over a copy of the registry, so the cost of loading grows
/// with their number (about 0.2 ms each), and a file written to be slow must be refused rather than waited for.
pub const MAX_SOURCES: usize = 256;

/// Why a frozen file could not be built, rendered or parsed. The text names the offending line when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenError(pub String);

impl fmt::Display for FrozenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FrozenError {}

pub type FrozenResult<T> = Result<T, FrozenError>;

fn err<T>(message: impl Into<String>) -> FrozenResult<T> {
    Err(FrozenError(message.into()))
}

/// A name (key, origin, witness name): non-empty, `[A-Za-z0-9_.:/-]`.
pub(crate) fn is_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '-'))
}

fn check_name(what: &str, name: &str) -> FrozenResult<()> {
    if !is_name(name) {
        return err(format!("invalid {what} {name:?}: expected letters, digits and _ . : / -"));
    }
    Ok(())
}

pub(crate) fn check_value(what: &str, value: &str) -> FrozenResult<()> {
    if value.contains('\n') || value.contains('\r') {
        return err(format!("{what} must be one line, found {value:?}"));
    }
    if value != value.trim() {
        return err(format!("{what} must not start or end with whitespace, found {value:?}"));
    }
    Ok(())
}

/// A number written the one canonical way: digits only, no sign, no leading zero except `0` itself.
fn canonical_number(text: &str) -> Option<u32> {
    let canonical = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse().ok()).flatten()
}

/// Why a stored text (source or witness script) cannot be framed, if it cannot.
fn text_framing(what: &str, text: &str) -> FrozenResult<()> {
    if text.contains('\r') {
        return err(format!("{what} contains a bare carriage return, which is not allowed (line endings are LF)"));
    }
    if text.ends_with('\n') {
        return err(format!("{what} ends with a newline: expected the trailing newlines removed"));
    }
    if let Some(line) = text.lines().find(|l| l.starts_with("@@ ")) {
        return err(format!("{what} contains a line that starts with the framing marker `@@ `: {line}"));
    }
    Ok(())
}

/// Text as it is stored: CRLF becomes LF, trailing newlines go; a bare carriage return and a line starting with
/// `@@ ` are refused.
fn normalise_text(what: &str, text: &str) -> FrozenResult<String> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim_end_matches('\n').to_string();
    text_framing(what, &text)?;
    Ok(text)
}

/// `key = value` pairs of one header block (`[Engine]`, `[Environment]`, `[Witness]`): keys are names and unique,
/// values are one line without leading or trailing whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pairs(pub(crate) Vec<(String, String)>);

impl Pairs {
    pub fn new(items: &[(&str, &str)]) -> FrozenResult<Pairs> {
        Pairs::from_vec(items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    pub fn from_vec(items: Vec<(String, String)>) -> FrozenResult<Pairs> {
        let pairs = Pairs(items);
        pairs.check()?;
        Ok(pairs)
    }

    pub(crate) fn check(&self) -> FrozenResult<()> {
        let mut seen = HashSet::new();
        for (key, value) in &self.0 {
            check_name("key", key)?;
            if !seen.insert(key.as_str()) {
                return err(format!("the key {key} appears twice: expected each key once"));
            }
            check_value(&format!("the value of {key}"), value)?;
        }
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_slice(&self) -> &[(String, String)] {
        &self.0
    }
}

/// One piece of configuration text, kept as it was read. Its hash is over the conf canonical form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub(crate) origin: String,
    pub(crate) text: String,
    pub(crate) sha256: String,
}

impl Source {
    pub fn new(origin: &str, text: &str) -> FrozenResult<Source> {
        check_name("source origin", origin)?;
        let text = normalise_text(&format!("source {origin:?}"), text)?;
        let sha256 = source_sha256(&text);
        Ok(Source { origin: origin.to_string(), text, sha256 })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The hash recorded in the file's marker (true or not: see [`FrozenFile::verify`]).
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// A user witness: a script kept inside the file. Its hash is byte-exact (see [`exact_sha256`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessScript {
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) sha256: String,
}

impl WitnessScript {
    pub fn new(name: &str, text: &str) -> FrozenResult<WitnessScript> {
        check_name("witness name", name)?;
        let text = normalise_text(&format!("witness {name:?}"), text)?;
        let sha256 = exact_sha256(&text);
        Ok(WitnessScript { name: name.to_string(), text, sha256 })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The hash recorded in the file's marker (true or not: see [`FrozenFile::verify`]).
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// One entry of the hash-chained history (spec §5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub(crate) n: u32,
    pub(crate) time: String,
    pub(crate) action: String,
    pub(crate) engine: String,
    pub(crate) engine_sha256: String,
    pub(crate) policy: String,
    pub(crate) pinned: String,
    pub(crate) actor: String,
    pub(crate) reason: String,
    pub(crate) old_content_sha256: String,
    pub(crate) new_content_sha256: String,
    pub(crate) diff: String,
    pub(crate) prev_entry_sha256: String,
    pub(crate) entry_sha256: String,
}

impl HistoryEntry {
    pub fn n(&self) -> u32 {
        self.n
    }
    pub fn time(&self) -> &str {
        &self.time
    }
    pub fn action(&self) -> &str {
        &self.action
    }
    /// The engine that made this entry, as the caller wrote it (`EntryInput::engine`): informational text, not
    /// derived by the library. What the file pins is [`policy`](Self::policy) and [`pinned`](Self::pinned).
    pub fn engine(&self) -> &str {
        &self.engine
    }
    pub fn engine_sha256(&self) -> &str {
        &self.engine_sha256
    }
    /// The policy of the `[Engine]` block this entry sealed, derived by the library from the block.
    pub fn policy(&self) -> &str {
        &self.policy
    }
    /// The engine the `[Engine]` block of this entry pins, short form (`version+commit`, twelve characters of the
    /// commit), derived by the library from the block.
    pub fn pinned(&self) -> &str {
        &self.pinned
    }
    pub fn actor(&self) -> &str {
        &self.actor
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn old_content_sha256(&self) -> &str {
        &self.old_content_sha256
    }
    pub fn new_content_sha256(&self) -> &str {
        &self.new_content_sha256
    }
    pub fn diff(&self) -> &str {
        &self.diff
    }
    pub fn prev_entry_sha256(&self) -> &str {
        &self.prev_entry_sha256
    }
    pub fn entry_sha256(&self) -> &str {
        &self.entry_sha256
    }

    fn values(&self) -> [&String; 13] {
        [
            &self.time, &self.action, &self.engine, &self.engine_sha256, &self.policy, &self.pinned, &self.actor, &self.reason,
            &self.old_content_sha256, &self.new_content_sha256, &self.diff, &self.prev_entry_sha256, &self.entry_sha256,
        ]
    }

    /// `policy` names a policy and `pinned` is not empty (both are derived from the engine block; `verify` checks
    /// that the last entry agrees with the current block). The error names the field.
    fn pin_shape(&self) -> Result<(), (&'static str, String)> {
        if let Err(e) = self.policy.parse::<EnginePolicy>() {
            return Err(("policy", format!("policy: {e}")));
        }
        if self.pinned.is_empty() {
            return Err(("pinned", "pinned is empty: expected the pinned engine, like 0.3.1+0123456789ab".into()));
        }
        Ok(())
    }

    /// The fields that must be a SHA-256, and whether each may be empty (only in the first entry, where there is no
    /// old content and no previous entry; `verify` checks that).
    fn hash_fields(&self) -> [(&'static str, &String, bool); 5] {
        [
            ("engine_sha256", &self.engine_sha256, false),
            ("old_content_sha256", &self.old_content_sha256, true),
            ("new_content_sha256", &self.new_content_sha256, false),
            ("prev_entry_sha256", &self.prev_entry_sha256, true),
            ("entry_sha256", &self.entry_sha256, false),
        ]
    }
}

/// A frozen configuration, as data. Built by [`FrozenFile::new`](crate::frozen::FrozenFile::new) or
/// [`FrozenFile::parse`]; read through its getters; changed only through the validated setters, after which it is
/// unsealed until [`FrozenFile::reseal`](crate::frozen::FrozenFile::reseal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenFile {
    pub(crate) format: u32,
    pub(crate) created: String,
    pub(crate) content_sha256: String,
    pub(crate) seal_sha256: String,
    pub(crate) engine: Pairs,
    pub(crate) environment: Pairs,
    pub(crate) witnesses: Pairs,
    pub(crate) history: Vec<HistoryEntry>,
    pub(crate) sources: Vec<Source>,
    pub(crate) witness_scripts: Vec<WitnessScript>,
}

const HISTORY_FIELDS: [&str; 13] = [
    "time", "action", "engine", "engine_sha256", "policy", "pinned", "actor", "reason", "old_content_sha256", "new_content_sha256",
    "diff", "prev_entry_sha256", "entry_sha256",
];

fn push_kv(out: &mut String, key: &str, value: &str) {
    out.push_str(&format!("{key} = {value}\n"));
}

fn push_pairs(out: &mut String, header: &str, pairs: &Pairs) {
    out.push_str(&format!("\n[{header}]\n"));
    for (key, value) in pairs.iter() {
        push_kv(out, key, value);
    }
}

impl FrozenFile {
    pub fn format(&self) -> u32 {
        self.format
    }
    pub fn created(&self) -> &str {
        &self.created
    }
    /// The content hash recorded in the file (true or not: see [`FrozenFile::verify`]).
    pub fn content_sha256(&self) -> &str {
        &self.content_sha256
    }
    /// The seal recorded in the file (true or not: see [`FrozenFile::verify`]).
    pub fn seal_sha256(&self) -> &str {
        &self.seal_sha256
    }
    pub fn engine(&self) -> &Pairs {
        &self.engine
    }
    pub fn environment(&self) -> &Pairs {
        &self.environment
    }
    pub fn witnesses(&self) -> &Pairs {
        &self.witnesses
    }
    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    pub fn witness_scripts(&self) -> &[WitnessScript] {
        &self.witness_scripts
    }

    /// Replaces the text of source `index` (0-based). The recorded hashes are kept, so the file reports the
    /// source as modified until it is resealed.
    pub fn set_source_text(&mut self, index: usize, text: &str) -> FrozenResult<()> {
        let count = self.sources.len();
        let Some(source) = self.sources.get_mut(index) else {
            return err(format!("there is no source at index {index}: expected 0 to {}", count.saturating_sub(1)));
        };
        source.text = normalise_text(&format!("source {:?}", source.origin), text)?;
        Ok(())
    }

    /// Replaces `[Engine]`. The file is unsealed until it is resealed.
    pub fn set_engine(&mut self, engine: Pairs) {
        self.engine = engine;
    }

    /// Replaces `[Environment]` (outside the seal).
    pub fn set_environment(&mut self, environment: Pairs) {
        self.environment = environment;
    }

    /// Replaces `[Witness]`. The file is unsealed until it is resealed.
    pub fn set_witnesses(&mut self, witnesses: Pairs) {
        self.witnesses = witnesses;
    }

    /// Replaces the witness scripts. The file is unsealed until it is resealed.
    pub fn set_witness_scripts(&mut self, witness_scripts: Vec<WitnessScript>) {
        self.witness_scripts = witness_scripts;
    }

    /// Everything that would make the rendered text malformed (names, one-line values, framing, hash shapes),
    /// except the format number, an empty history and the numbering of the entries, which
    /// [`FrozenFile::verify`](crate::frozen::FrozenFile::verify) reports as problems of their own.
    pub(crate) fn framing_problems(&self) -> Vec<String> {
        let mut found = Vec::new();
        let mut note = |result: FrozenResult<()>| {
            if let Err(e) = result {
                found.push(e.0);
            }
        };
        if !is_rfc3339_utc(&self.created) {
            note(err(format!("created is not a UTC time like 2026-11-07T14:03:11Z: {:?}", self.created)));
        }
        for (key, value) in [("content_sha256", &self.content_sha256), ("seal_sha256", &self.seal_sha256)] {
            if !is_sha256_hex(value) {
                note(err(format!("{key} is not a SHA-256 in lowercase hexadecimal: {value:?}")));
            }
        }
        for (header, pairs) in [("Engine", &self.engine), ("Environment", &self.environment), ("Witness", &self.witnesses)] {
            note(pairs.check().map_err(|e| FrozenError(format!("[{header}]: {e}"))));
        }
        for entry in &self.history {
            let n = entry.n;
            for (key, value) in HISTORY_FIELDS.iter().zip(entry.values()) {
                note(check_value(&format!("[History.{n}] {key}"), value));
            }
            if !is_rfc3339_utc(&entry.time) {
                note(err(format!("[History.{n}] time is not a UTC time like 2026-11-07T14:03:11Z: {:?}", entry.time)));
            }
            for (key, value, may_be_empty) in entry.hash_fields() {
                if !(value.is_empty() && may_be_empty) && !is_sha256_hex(value) {
                    note(err(format!("[History.{n}] {key} is not a SHA-256 in lowercase hexadecimal: {value:?}")));
                }
            }
            if let Err((_, e)) = entry.pin_shape() {
                note(err(format!("[History.{n}] {e}")));
            }
        }
        for (index, source) in self.sources.iter().enumerate() {
            note(check_name("source origin", &source.origin));
            note(text_framing(&format!("source {}", index + 1), &source.text));
            if !is_sha256_hex(&source.sha256) {
                note(err(format!("source {} hash is not a SHA-256 in lowercase hexadecimal: {:?}", index + 1, source.sha256)));
            }
        }
        for script in &self.witness_scripts {
            note(check_name("witness name", &script.name));
            note(text_framing(&format!("witness {:?}", script.name), &script.text));
            if !is_sha256_hex(&script.sha256) {
                note(err(format!("witness {:?} hash is not a SHA-256 in lowercase hexadecimal: {:?}", script.name, script.sha256)));
            }
        }
        found
    }

    /// The text of the file. Everything is checked again first, so the text always parses.
    pub fn render(&self) -> FrozenResult<String> {
        if self.format != FORMAT_VERSION {
            return err(format!("unsupported format {}: expected {FORMAT_VERSION}", self.format));
        }
        if self.history.is_empty() {
            return err("the history has no entry: a frozen file always records how it was made");
        }
        for (position, entry) in self.history.iter().enumerate() {
            if entry.n as usize != position + 1 {
                return err(format!("history entry at position {} is numbered {}: expected {}", position + 1, entry.n, position + 1));
            }
        }
        if let Some(first) = self.framing_problems().into_iter().next() {
            return err(first);
        }
        let mut out = String::new();
        out.push_str(MAGIC);
        out.push('\n');
        out.push_str("[Freeze]\n");
        push_kv(&mut out, "format", &self.format.to_string());
        push_kv(&mut out, "created", &self.created);
        push_kv(&mut out, "content_sha256", &self.content_sha256);
        push_kv(&mut out, "seal_sha256", &self.seal_sha256);
        push_pairs(&mut out, "Engine", &self.engine);
        push_pairs(&mut out, "Environment", &self.environment);
        push_pairs(&mut out, "Witness", &self.witnesses);
        for entry in &self.history {
            out.push_str(&format!("\n[History.{}]\n", entry.n));
            for (key, value) in HISTORY_FIELDS.iter().zip(entry.values()) {
                push_kv(&mut out, key, value);
            }
        }
        out.push('\n');
        for (index, source) in self.sources.iter().enumerate() {
            let n = index + 1;
            out.push_str(&format!("@@ source {n} origin={} sha256={}\n", source.origin, source.sha256));
            if !source.text.is_empty() {
                out.push_str(&source.text);
                out.push('\n');
            }
            out.push_str(&format!("@@ end source {n}\n"));
        }
        for script in &self.witness_scripts {
            out.push_str(&format!("@@ witness {} sha256={}\n", script.name, script.sha256));
            if !script.text.is_empty() {
                out.push_str(&script.text);
                out.push('\n');
            }
            out.push_str("@@ end witness\n");
        }
        Ok(out)
    }

    /// Reads a frozen file. Strict: any malformed line is an error that names it. The stored hashes are checked
    /// for shape here and for truth by [`FrozenFile::verify`](crate::frozen::FrozenFile::verify).
    pub fn parse(text: &str) -> FrozenResult<FrozenFile> {
        let lines: Vec<&str> = text.lines().collect();
        if lines.first().map(|l| l.trim_end()) != Some(MAGIC) {
            return err("not a physure frozen configuration: the first line must be `#physure-frozen 1`");
        }
        if let Some(i) = lines.iter().position(|l| l.contains('\r')) {
            return err(format!("line {}: a bare carriage return is not allowed (line endings are LF or CRLF)", i + 1));
        }
        let mut freeze: HashMap<String, (String, usize)> = HashMap::new();
        let mut freeze_line = 1;
        let (mut engine, mut environment, mut witnesses) = (Vec::new(), Vec::new(), Vec::new());
        // per history entry: the line of its section header and its (key, value, line) fields
        let mut entries: Vec<(usize, Vec<(String, String, usize)>)> = Vec::new();
        let mut opened: HashSet<String> = HashSet::new();
        let mut keys_here: HashSet<String> = HashSet::new();
        let mut section = String::new();
        let mut at = 1;
        while at < lines.len() && !lines[at].starts_with("@@ ") {
            let number = at + 1;
            let line = lines[at].trim();
            at += 1;
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                if !matches!(name, "Freeze" | "Engine" | "Environment" | "Witness") {
                    match name.strip_prefix("History.").map(canonical_number) {
                        None => return err(format!("line {number}: unknown section [{name}]")),
                        Some(None) => {
                            return err(format!(
                                "line {number}: a history entry is numbered with digits, no sign and no leading zero; found [{name}]"
                            ))
                        }
                        Some(Some(n)) if n as usize != entries.len() + 1 => {
                            return err(format!(
                                "line {number}: history entries must be numbered 1, 2, 3, ...; expected [History.{}], found [{name}]",
                                entries.len() + 1
                            ))
                        }
                        Some(Some(_)) => entries.push((number, Vec::new())),
                    }
                }
                if !opened.insert(name.to_string()) {
                    return err(format!("line {number}: the section [{name}] is opened a second time: expected each section once"));
                }
                if name == "Freeze" {
                    freeze_line = number;
                }
                keys_here.clear();
                section = name.to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return err(format!("line {number}: expected `key = value`, found {line:?}"));
            };
            let (key, value) = (key.trim().to_string(), value.trim().to_string());
            if !is_name(&key) {
                return err(format!("line {number}: invalid key {key:?}: expected letters, digits and _ . : / -"));
            }
            if section.is_empty() {
                return err(format!("line {number}: `{line}` is before any section"));
            }
            if !keys_here.insert(key.clone()) {
                return err(format!("line {number}: [{section}] defines {key} a second time: expected each key once"));
            }
            match section.as_str() {
                "Freeze" => {
                    if !matches!(key.as_str(), "format" | "created" | "content_sha256" | "seal_sha256") {
                        return err(format!("line {number}: unknown key {key} in [Freeze]"));
                    }
                    freeze.insert(key, (value, number));
                }
                "Engine" => engine.push((key, value)),
                "Environment" => environment.push((key, value)),
                "Witness" => witnesses.push((key, value)),
                _ => {
                    if !HISTORY_FIELDS.contains(&key.as_str()) {
                        return err(format!("line {number}: unknown key {key} in [{section}]"));
                    }
                    entries.last_mut().expect("a History section opened it").1.push((key, value, number));
                }
            }
        }
        let get = |key: &str| -> FrozenResult<(String, usize)> {
            freeze.get(key).cloned().ok_or_else(|| FrozenError(format!("line {freeze_line}: [Freeze] has no {key}")))
        };
        let (format, line) = get("format")?;
        let Some(format) = canonical_number(&format) else {
            return err(format!("line {line}: format is not a number written with digits, no sign and no leading zero: {format:?}"));
        };
        if format != FORMAT_VERSION {
            return err(format!("line {line}: unsupported format {format}: this build reads format {FORMAT_VERSION}"));
        }
        let (created, line) = get("created")?;
        if !is_rfc3339_utc(&created) {
            return err(format!("line {line}: created is not a UTC time like 2026-11-07T14:03:11Z: {created:?}"));
        }
        let mut hashes = Vec::new();
        for key in ["content_sha256", "seal_sha256"] {
            let (value, line) = get(key)?;
            if !is_sha256_hex(&value) {
                return err(format!("line {line}: {key} is not a SHA-256 in lowercase hexadecimal: {value:?}"));
            }
            hashes.push(value);
        }
        let seal_sha256 = hashes.pop().expect("two hashes");
        let content_sha256 = hashes.pop().expect("two hashes");
        if entries.is_empty() {
            return err(format!("line {}: the history has no entry: a frozen file always records how it was made", at.min(lines.len())));
        }
        let mut history = Vec::new();
        for (index, (header_line, fields)) in entries.into_iter().enumerate() {
            history.push(read_entry(index as u32 + 1, header_line, fields)?);
        }
        let mut file = FrozenFile {
            format,
            created,
            content_sha256,
            seal_sha256,
            engine: Pairs(engine),
            environment: Pairs(environment),
            witnesses: Pairs(witnesses),
            history,
            sources: Vec::new(),
            witness_scripts: Vec::new(),
        };
        read_blocks(&lines, at, &mut file)?;
        Ok(file)
    }
}

fn read_entry(n: u32, header_line: usize, mut fields: Vec<(String, String, usize)>) -> FrozenResult<HistoryEntry> {
    let mut lines = HashMap::new();
    let mut take = |key: &str| -> FrozenResult<String> {
        match fields.iter().position(|(k, _, _)| k == key) {
            Some(i) => {
                let (_, value, line) = fields.remove(i);
                lines.insert(key.to_string(), line);
                Ok(value)
            }
            None => err(format!("line {header_line}: [History.{n}] has no {key}")),
        }
    };
    let entry = HistoryEntry {
        n,
        time: take("time")?,
        action: take("action")?,
        engine: take("engine")?,
        engine_sha256: take("engine_sha256")?,
        policy: take("policy")?,
        pinned: take("pinned")?,
        actor: take("actor")?,
        reason: take("reason")?,
        old_content_sha256: take("old_content_sha256")?,
        new_content_sha256: take("new_content_sha256")?,
        diff: take("diff")?,
        prev_entry_sha256: take("prev_entry_sha256")?,
        entry_sha256: take("entry_sha256")?,
    };
    if !is_rfc3339_utc(&entry.time) {
        return err(format!(
            "line {}: [History.{n}] time is not a UTC time like 2026-11-07T14:03:11Z: {:?}",
            lines["time"], entry.time
        ));
    }
    for (key, value, may_be_empty) in entry.hash_fields() {
        if !(value.is_empty() && may_be_empty) && !is_sha256_hex(value) {
            return err(format!("line {}: [History.{n}] {key} is not a SHA-256 in lowercase hexadecimal: {value:?}", lines[key]));
        }
    }
    if let Err((key, e)) = entry.pin_shape() {
        return err(format!("line {}: [History.{n}] {e}", lines[key]));
    }
    Ok(entry)
}

/// The `@@` blocks that follow the header. Markers are matched exactly; inside a block, a line starting with
/// `@@ ` must be that block's end marker.
fn read_blocks(lines: &[&str], mut at: usize, file: &mut FrozenFile) -> FrozenResult<()> {
    while at < lines.len() {
        let number = at + 1;
        let line = lines[at];
        at += 1;
        if line.trim().is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split(' ').collect();
        let (end, kind) = match words.as_slice() {
            ["@@", "source", n, origin, sha] => {
                let Some(index) = canonical_number(n) else {
                    return err(format!("line {number}: bad source marker: the number is written with digits, no sign and no leading zero: {line}"));
                };
                if file.sources.len() >= MAX_SOURCES {
                    return err(format!("line {number}: a frozen configuration has at most {MAX_SOURCES} sources (each one is replayed over a copy of the registry)"));
                }
                if index as usize != file.sources.len() + 1 {
                    return err(format!(
                        "line {number}: sources must be numbered 1, 2, 3, ...; expected {}, found {index}",
                        file.sources.len() + 1
                    ));
                }
                let origin = origin.strip_prefix("origin=").filter(|o| is_name(o));
                let sha = sha.strip_prefix("sha256=").filter(|s| is_sha256_hex(s));
                match (origin, sha) {
                    (Some(origin), Some(sha)) => (format!("@@ end source {index}"), Block::Source(origin.to_string(), sha.to_string())),
                    _ => return err(format!("line {number}: bad source marker (origin=NAME sha256=HEX): {line}")),
                }
            }
            ["@@", "witness", name, sha] => {
                let sha = sha.strip_prefix("sha256=").filter(|s| is_sha256_hex(s));
                match (is_name(name), sha) {
                    (true, Some(sha)) => ("@@ end witness".to_string(), Block::Witness(name.to_string(), sha.to_string())),
                    _ => return err(format!("line {number}: bad witness marker (NAME sha256=HEX): {line}")),
                }
            }
            _ if line.starts_with("@@ ") => return err(format!("line {number}: bad block marker: {line}")),
            _ => return err(format!("line {number}: unexpected line outside a block: {line}")),
        };
        let start = at;
        loop {
            let Some(inner) = lines.get(at) else {
                return err(format!("line {number}: the block has no end marker `{end}`"));
            };
            if *inner == end {
                break;
            }
            if inner.starts_with("@@ ") {
                return err(format!(
                    "line {}: a line inside the block starts with the framing marker `@@ `: expected `{end}`, found {inner:?}",
                    at + 1
                ));
            }
            at += 1;
        }
        // trailing empty lines are not part of a stored text (as in `normalise_text`)
        let text = lines[start..at].join("\n").trim_end_matches('\n').to_string();
        at += 1;
        match kind {
            Block::Source(origin, sha256) => file.sources.push(Source { origin, text, sha256 }),
            Block::Witness(name, sha256) => file.witness_scripts.push(WitnessScript { name, text, sha256 }),
        }
    }
    Ok(())
}

enum Block {
    Source(String, String),
    Witness(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn pairs(items: &[(&str, &str)]) -> Pairs {
        Pairs(items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    fn entry(n: u32) -> HistoryEntry {
        HistoryEntry {
            n,
            time: "2026-11-07T14:03:11Z".into(),
            action: "freeze".into(),
            engine: "0.3.1+9f2c1ab".into(),
            engine_sha256: h('7'),
            policy: "build".into(),
            pinned: "0.3.1+0123456789ab".into(),
            actor: "irvin@host".into(),
            reason: "initial freeze".into(),
            old_content_sha256: String::new(),
            new_content_sha256: h('c'),
            diff: "added 3, removed 0, changed 0".into(),
            prev_entry_sha256: String::new(),
            entry_sha256: h('e'),
        }
    }

    fn sample() -> FrozenFile {
        FrozenFile {
            format: 1,
            created: "2026-11-07T14:03:11Z".into(),
            content_sha256: h('c'),
            seal_sha256: h('d'),
            engine: pairs(&[("version", "0.3.1"), ("policy", "build")]),
            environment: pairs(&[("os", "windows")]),
            witnesses: pairs(&[("builtin.conversion.m_to_ft", "3ff0000000000000")]),
            history: vec![entry(1)],
            sources: vec![
                Source { origin: "embedded".into(), text: "[Units]\nm = 1".into(), sha256: h('a') },
                Source { origin: "overrides".into(), text: String::new(), sha256: h('b') },
            ],
            witness_scripts: vec![WitnessScript { name: "validation.1".into(), text: "1 m => ft".into(), sha256: h('f') }],
        }
    }

    #[test]
    fn it_round_trips() {
        let file = sample();
        let text = file.render().unwrap();
        assert!(text.starts_with("#physure-frozen 1\n[Freeze]\nformat = 1\n"));
        assert_eq!(FrozenFile::parse(&text).unwrap(), file);
        assert_eq!(FrozenFile::parse(&text).unwrap().render().unwrap(), text);
    }

    #[test]
    fn crlf_is_accepted_and_header_comments_are_ignored() {
        let text = sample().render().unwrap().replace('\n', "\r\n");
        assert_eq!(FrozenFile::parse(&text).unwrap(), sample());
        let commented = sample().render().unwrap().replacen("[Engine]\n", "# note\n\n[Engine]\n", 1);
        assert_eq!(FrozenFile::parse(&commented).unwrap(), sample());
    }

    fn parse_err(text: &str) -> String {
        FrozenFile::parse(text).unwrap_err().to_string()
    }

    #[test]
    fn what_is_not_a_frozen_file_is_refused() {
        assert!(parse_err("").contains("physure-frozen"));
        assert!(parse_err("[Units]\nm = 1\n").contains("physure-frozen"));
        assert!(parse_err("#physure-frozen 2\n").contains("physure-frozen"));
    }

    #[test]
    fn the_header_is_strict() {
        let good = sample().render().unwrap();
        let cases = [
            (good.replace("[Engine]", "[Engin]"), "unknown section"),
            (good.replace("version = 0.3.1", "version = 0.3.1\nversion = 0.3.2"), "version a second time"),
            (good.replace("format = 1", "format"), "key = value"),
            (good.replace("format = 1", "formats = 1"), "formats"),
            (good.replace("format = 1\n", ""), "has no format"),
            (good.replace("format = 1", "format = 2"), "unsupported format 2"),
            (good.replace("[History.1]", "[History.2]"), "numbered"),
            (good.replace(&h('c'), &"C".repeat(64)), "sha256"),
            (good.replace("time = 2026-11-07T14:03:11Z", "time = yesterday"), "time"),
            (good.replace("created = 2026-11-07T14:03:11Z", "created = soon"), "created"),
            (good.replace("action = freeze\n", ""), "has no action"),
            (good.replace(&format!("engine_sha256 = {}\n", h('7')), ""), "has no engine_sha256"),
            (good.replace(&format!("engine_sha256 = {}", h('7')), "engine_sha256 ="), "engine_sha256 is not a SHA-256"),
            (good.replace("[Freeze]\n", "orphan = 1\n[Freeze]\n"), "before any section"),
            (good.replace("policy = build", "bad key = build"), "invalid key"),
        ];
        for (text, expected) in &cases {
            let message = parse_err(text);
            assert!(message.contains(expected), "{expected:?} not in {message:?}");
            assert!(message.starts_with("line "), "every parse error names its line: {message:?}");
        }
        let no_history = good.split("\n[History.1]").next().unwrap().to_string() + "\n";
        assert!(parse_err(&no_history).contains("no entry"));
        let no_seal = good.replace(&format!("seal_sha256 = {}\n", h('d')), "");
        assert!(parse_err(&no_seal).contains("has no seal_sha256"));
    }

    #[test]
    fn the_blocks_are_strict() {
        let good = sample().render().unwrap();
        assert!(parse_err(&good.replace("@@ end source 1", "@@ end source 7")).contains("end"));
        assert!(parse_err(&good.replace("@@ source 2 ", "@@ source 3 ")).contains("numbered"));
        assert!(parse_err(&good.replace("origin=embedded", "origin=em bedded")).contains("marker"));
        assert!(parse_err(&good.replace("origin=embedded", "origin=")).contains("marker"));
        assert!(parse_err(&good.replace("@@ witness validation.1", "@@ witness valid@tion")).contains("marker"));
        assert!(parse_err(&good.replace("@@ witness validation.1", "@@ witness  validation.1")).contains("marker"));
        assert!(parse_err(&format!("{good}stray line\n")).contains("unexpected"));
        assert!(parse_err(&good.replace("@@ end witness\n", "")).contains("end"));
        assert!(parse_err(&good.replace("@@ end source 1\n", "@@ end source 1  \n")).contains("framing marker"));
        assert!(parse_err(&good.replace("@@ source 1 origin", "@@ source 1  origin")).contains("marker"));
    }

    #[test]
    fn empty_lines_at_the_end_of_a_block_are_not_part_of_its_text() {
        let good = sample().render().unwrap();
        let padded = good.replace("m = 1\n@@ end source 1", "m = 1\n\n\n@@ end source 1").replace("ft\n@@ end witness", "ft\n\n@@ end witness");
        assert_eq!(FrozenFile::parse(&padded).unwrap(), sample());
        let inner = good.replace("[Units]\nm = 1", "[Units]\n\nm = 1");
        assert_eq!(FrozenFile::parse(&inner).unwrap().sources()[0].text(), "[Units]\n\nm = 1", "inside a text, kept");
    }

    #[test]
    fn a_source_cannot_contain_the_framing_marker() {
        assert!(Source::new("overrides", "m = 1\n@@ end source 2\n").is_err());
        assert!(Source::new("over rides", "m = 1").is_err());
        assert!(Source::new("", "m = 1").is_err());
        assert!(WitnessScript::new("a b", "1 m").is_err());
        assert!(WitnessScript::new("w", "@@ end witness").is_err());
    }

    #[test]
    fn source_text_is_normalised() {
        let a = Source::new("overrides", "m = 1\r\nkg = 2\r\n\r\n").unwrap();
        assert_eq!(a.text(), "m = 1\nkg = 2");
        assert_eq!(a.sha256(), super::super::canonical::source_sha256("m = 1\nkg = 2"));
        assert_eq!(a.origin(), "overrides");
        let w = WitnessScript::new("w", "1 m => ft\r\n").unwrap();
        assert_eq!((w.name(), w.text(), w.sha256()), ("w", "1 m => ft", exact_sha256("1 m => ft").as_str()));
    }

    // ---- Task 8 ----

    #[test]
    fn witness_scripts_are_hashed_byte_exact() {
        let hash = |t: &str| WitnessScript::new("w", t).unwrap().sha256;
        assert_ne!(hash("x = 1 m\n; y = 2 m"), hash("x = 1 m\n; y = 3 m"));
        assert_ne!(hash("fn f(x) =\n    a = x\n    return a"), hash("fn f(x) =\na = x\nreturn a"));
        assert_eq!(hash("x = 1 m\r\ny = 2 m\r\n"), hash("x = 1 m\ny = 2 m"));
        assert_ne!(exact_sha256("x = 1 m\n; y = 2 m"), exact_sha256("x = 1 m\n; y = 3 m"));
        assert_ne!(exact_sha256("s = \"a\n   b\""), exact_sha256("s = \"a\nb\""));
        assert_eq!(exact_sha256("x = 1 m\r\ny = 2 m\r\n"), exact_sha256("x = 1 m\ny = 2 m"));
    }

    #[test]
    fn a_repeated_freeze_key_or_section_is_refused_by_the_parser() {
        let good = sample().render().unwrap();
        let twice_key = good.replace("format = 1\n", "format = 1\ncreated = 2030-01-01T00:00:00Z\n");
        assert!(parse_err(&twice_key).contains("created"), "{}", parse_err(&twice_key));
        let reopened = good.replacen("[Engine]", "[Freeze]\nformat = 1\n\n[Engine]", 1);
        assert!(parse_err(&reopened).contains("[Freeze] is opened a second time"), "{}", parse_err(&reopened));
        let engine_twice = good.replacen("[Environment]", "[Engine]\nother = 1\n\n[Environment]", 1);
        assert!(parse_err(&engine_twice).contains("[Engine] is opened a second time"), "{}", parse_err(&engine_twice));
    }

    #[test]
    fn non_canonical_numbers_are_refused() {
        let good = sample().render().unwrap();
        for (from, to) in [
            ("[History.1]", "[History.+1]"),
            ("[History.1]", "[History.01]"),
            ("@@ source 1 ", "@@ source +1 "),
            ("@@ source 1 ", "@@ source 01 "),
            ("format = 1", "format = +01"),
            ("format = 1", "format = 01"),
        ] {
            let message = parse_err(&good.replace(from, to));
            assert!(message.contains("no leading zero"), "{to}: {message}");
        }
        assert_eq!(canonical_number("0"), Some(0));
        assert_eq!(canonical_number("10"), Some(10));
        assert_eq!(canonical_number(""), None);
        assert_eq!(canonical_number("99999999999"), None);
    }

    #[test]
    fn a_line_starting_with_the_marker_inside_a_block_is_refused_by_the_parser() {
        let good = sample().render().unwrap();
        let inside = good.replace("[Units]\nm = 1", "[Units]\n@@ source 9 origin=x sha256=00\nm = 1");
        assert!(parse_err(&inside).contains("framing marker"));
        let spaced = good.replace("1 m => ft\n", "1 m => ft\n@@ end witness extra\n");
        assert!(parse_err(&spaced).contains("framing marker"));
    }

    #[test]
    fn a_bare_carriage_return_is_refused() {
        assert!(Source::new("overrides", "m = 1\r").is_err());
        assert!(Source::new("overrides", "m = 1\rkg = 2").is_err());
        assert!(WitnessScript::new("w", "1 m\r=> ft").is_err());
        assert!(Source::new("overrides", "m = 1\r\nkg = 2\r\n").is_ok(), "CRLF is a line ending, not a bare CR");
        let good = sample().render().unwrap();
        assert!(parse_err(&good.replace("m = 1\n", "m = 1\r\r\n")).contains("carriage return"));
    }

    #[test]
    fn values_must_be_one_line_and_trimmed() {
        let mut file = sample();
        file.engine.0.push(("note".into(), "two\nlines".into()));
        assert!(file.render().is_err());
        let mut file = sample();
        file.engine.0.push(("note".into(), " padded".into()));
        assert!(file.render().is_err());
        let mut file = sample();
        file.engine.0.push(("bad key".into(), "x".into()));
        assert!(file.render().is_err());
    }

    #[test]
    fn render_checks_everything_parse_would_refuse() {
        let breaks: Vec<(&str, Box<dyn Fn(&mut FrozenFile)>)> = vec![
            ("format", Box::new(|f| f.format = 2)),
            ("empty history", Box::new(|f| f.history.clear())),
            ("numbering", Box::new(|f| f.history[0].n = 2)),
            ("created", Box::new(|f| f.created = "now".into())),
            ("content hash", Box::new(|f| f.content_sha256 = "x".into())),
            ("duplicate key", Box::new(|f| f.engine.0.push(("version".into(), "2".into())))),
            ("entry value", Box::new(|f| f.history[0].actor = "a\nb".into())),
            ("entry time", Box::new(|f| f.history[0].time = "now".into())),
            ("entry hash", Box::new(|f| f.history[0].entry_sha256 = String::new())),
            ("entry engine hash", Box::new(|f| f.history[0].engine_sha256 = String::new())),
            ("origin", Box::new(|f| f.sources[0].origin = "embedded\nfake".into())),
            ("source framing", Box::new(|f| f.sources[0].text = "m = 1\n@@ end source 1".into())),
            ("source trailing newline", Box::new(|f| f.sources[0].text = "m = 1\n".into())),
            ("source cr", Box::new(|f| f.sources[0].text = "m = 1\r".into())),
            ("source hash", Box::new(|f| f.sources[0].sha256 = "x".into())),
            ("witness name", Box::new(|f| f.witness_scripts[0].name = "a b".into())),
            ("witness hash", Box::new(|f| f.witness_scripts[0].sha256 = String::new())),
        ];
        for (what, edit) in breaks {
            let mut file = sample();
            edit(&mut file);
            assert!(file.render().is_err(), "{what}");
        }
    }

    #[test]
    fn pairs_are_validated() {
        assert!(Pairs::new(&[("a", "1"), ("a", "2")]).unwrap_err().to_string().contains("twice"));
        assert!(Pairs::new(&[("a", "1\n2")]).is_err());
        assert!(Pairs::new(&[("a", " 1")]).is_err());
        assert!(Pairs::new(&[("a b", "1")]).is_err());
        assert!(Pairs::new(&[("", "1")]).is_err());
        let p = Pairs::new(&[("a", "1"), ("b", "")]).unwrap();
        assert_eq!((p.len(), p.is_empty(), p.get("a"), p.get("z")), (2, false, Some("1"), None));
        assert_eq!(p.iter().collect::<Vec<_>>(), vec![("a", "1"), ("b", "")]);
        assert_eq!(p.as_slice()[1].0, "b");
        assert!(Pairs::default().is_empty());
        assert_eq!(Pairs::from_vec(vec![("k".into(), "v".into())]).unwrap().get("k"), Some("v"));
    }

    #[test]
    fn a_file_with_more_sources_than_the_limit_is_refused() {
        let mut file = sample();
        while file.sources.len() <= MAX_SOURCES {
            file.sources.push(Source::new("extra", "m = 1").unwrap());
        }
        let message = FrozenFile::parse(&file.render().unwrap()).unwrap_err().to_string();
        assert!(message.contains("at most 256 sources"), "{message}");
    }

    #[test]
    fn the_setters_validate_and_do_not_reseal() {
        let mut file = sample();
        assert!(file.set_source_text(5, "m = 1").unwrap_err().to_string().contains("expected 0 to 1"));
        assert!(file.set_source_text(1, "m = 1\n@@ end source 2").is_err());
        file.set_source_text(1, "kg = 2\r\n").unwrap();
        assert_eq!(file.sources()[1].text(), "kg = 2");
        assert_eq!(file.sources()[1].sha256(), h('b'), "the recorded hash stays until the file is resealed");
        file.set_engine(pairs(&[("version", "9")]));
        file.set_environment(pairs(&[("os", "linux")]));
        file.set_witnesses(Pairs::default());
        file.set_witness_scripts(vec![]);
        assert_eq!(file.engine().get("version"), Some("9"));
        assert_eq!(file.environment().get("os"), Some("linux"));
        assert!(file.witnesses().is_empty() && file.witness_scripts().is_empty());
        assert_eq!((file.format(), file.created(), file.content_sha256(), file.seal_sha256()), (1, "2026-11-07T14:03:11Z", h('c').as_str(), h('d').as_str()));
        let e = &file.history()[0];
        assert_eq!(
            (e.n(), e.time(), e.action(), e.engine(), e.actor(), e.reason(), e.diff()),
            (1, "2026-11-07T14:03:11Z", "freeze", "0.3.1+9f2c1ab", "irvin@host", "initial freeze", "added 3, removed 0, changed 0")
        );
        assert_eq!((e.old_content_sha256(), e.new_content_sha256(), e.prev_entry_sha256(), e.entry_sha256()), ("", h('c').as_str(), "", h('e').as_str()));
        assert_eq!(e.engine_sha256(), h('7'));
    }
}
