//! The frozen file as data, and its strict renderer and parser (spec §4).

use std::fmt;

use super::canonical::{is_sha256_hex, source_sha256};
use super::time::is_rfc3339_utc;

pub const FORMAT_VERSION: u32 = 1;
pub const MAGIC: &str = "#physure-frozen 1";

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
fn is_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '-'))
}

fn check_value(what: &str, value: &str) -> FrozenResult<()> {
    if value.contains('\n') || value.contains('\r') {
        return err(format!("{what} must be one line"));
    }
    if value != value.trim() {
        return err(format!("{what} must not start or end with whitespace"));
    }
    Ok(())
}

/// Source text as it is stored: CRLF becomes LF, trailing newlines go, and no line may start with `@@ `.
fn normalise_text(what: &str, text: &str) -> FrozenResult<String> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim_end_matches('\n').to_string();
    if let Some(line) = text.lines().find(|l| l.starts_with("@@ ")) {
        return err(format!("{what} contains a line that starts with the framing marker `@@ `: {line}"));
    }
    Ok(text)
}

/// One piece of configuration text, kept as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub origin: String,
    pub text: String,
    pub sha256: String,
}

impl Source {
    pub fn new(origin: &str, text: &str) -> FrozenResult<Source> {
        if !is_name(origin) {
            return err(format!("invalid source origin {origin:?}: use letters, digits and _ . : / -"));
        }
        let text = normalise_text(&format!("source {origin:?}"), text)?;
        let sha256 = source_sha256(&text);
        Ok(Source { origin: origin.to_string(), text, sha256 })
    }
}

/// A user witness: a script kept inside the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessScript {
    pub name: String,
    pub text: String,
    pub sha256: String,
}

impl WitnessScript {
    pub fn new(name: &str, text: &str) -> FrozenResult<WitnessScript> {
        if !is_name(name) {
            return err(format!("invalid witness name {name:?}: use letters, digits and _ . : / -"));
        }
        let text = normalise_text(&format!("witness {name:?}"), text)?;
        let sha256 = source_sha256(&text);
        Ok(WitnessScript { name: name.to_string(), text, sha256 })
    }
}

/// One entry of the hash-chained history (spec §5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub n: u32,
    pub time: String,
    pub action: String,
    pub engine: String,
    pub actor: String,
    pub reason: String,
    pub old_content_sha256: String,
    pub new_content_sha256: String,
    pub diff: String,
    pub prev_entry_sha256: String,
    pub entry_sha256: String,
}

/// A frozen configuration, as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenFile {
    pub format: u32,
    pub created: String,
    pub content_sha256: String,
    pub seal_sha256: String,
    pub engine: Vec<(String, String)>,
    pub environment: Vec<(String, String)>,
    pub witnesses: Vec<(String, String)>,
    pub history: Vec<HistoryEntry>,
    pub sources: Vec<Source>,
    pub witness_scripts: Vec<WitnessScript>,
}

const HISTORY_FIELDS: [&str; 10] = [
    "time", "action", "engine", "actor", "reason", "old_content_sha256", "new_content_sha256", "diff",
    "prev_entry_sha256", "entry_sha256",
];

fn push_kv(out: &mut String, key: &str, value: &str) -> FrozenResult<()> {
    if !is_name(key) {
        return err(format!("invalid key {key:?}"));
    }
    check_value(&format!("the value of {key}"), value)?;
    out.push_str(&format!("{key} = {value}\n"));
    Ok(())
}

fn push_pairs(out: &mut String, header: &str, pairs: &[(String, String)]) -> FrozenResult<()> {
    out.push_str(&format!("\n[{header}]\n"));
    for (key, value) in pairs {
        push_kv(out, key, value)?;
    }
    Ok(())
}

fn push_unique(pairs: &mut Vec<(String, String)>, section: &str, key: String, value: String) -> FrozenResult<()> {
    if pairs.iter().any(|(k, _)| *k == key) {
        return err(format!("[{section}] defines {key} twice"));
    }
    pairs.push((key, value));
    Ok(())
}

impl FrozenFile {
    /// The text of the file. Every value is checked (one line, trimmed, valid names).
    pub fn render(&self) -> FrozenResult<String> {
        let mut out = String::new();
        out.push_str(MAGIC);
        out.push('\n');
        out.push_str("[Freeze]\n");
        push_kv(&mut out, "format", &self.format.to_string())?;
        push_kv(&mut out, "created", &self.created)?;
        push_kv(&mut out, "content_sha256", &self.content_sha256)?;
        push_kv(&mut out, "seal_sha256", &self.seal_sha256)?;
        push_pairs(&mut out, "Engine", &self.engine)?;
        push_pairs(&mut out, "Environment", &self.environment)?;
        push_pairs(&mut out, "Witness", &self.witnesses)?;
        for entry in &self.history {
            out.push_str(&format!("\n[History.{}]\n", entry.n));
            let fields = [
                &entry.time, &entry.action, &entry.engine, &entry.actor, &entry.reason, &entry.old_content_sha256,
                &entry.new_content_sha256, &entry.diff, &entry.prev_entry_sha256, &entry.entry_sha256,
            ];
            for (key, value) in HISTORY_FIELDS.iter().zip(fields) {
                push_kv(&mut out, key, value)?;
            }
        }
        out.push('\n');
        for (index, source) in self.sources.iter().enumerate() {
            if !is_name(&source.origin) {
                return err(format!("invalid source origin {:?}", source.origin));
            }
            let n = index + 1;
            out.push_str(&format!("@@ source {n} origin={} sha256={}\n", source.origin, source.sha256));
            if !source.text.is_empty() {
                out.push_str(&source.text);
                out.push('\n');
            }
            out.push_str(&format!("@@ end source {n}\n"));
        }
        for script in &self.witness_scripts {
            if !is_name(&script.name) {
                return err(format!("invalid witness name {:?}", script.name));
            }
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
    /// for shape here and for truth by [`FrozenFile::verify`].
    pub fn parse(text: &str) -> FrozenResult<FrozenFile> {
        let lines: Vec<&str> = text.lines().collect();
        if lines.first().map(|l| l.trim_end()) != Some(MAGIC) {
            return err("not a physure frozen configuration: the first line must be `#physure-frozen 1`");
        }
        let mut file = FrozenFile {
            format: 0,
            created: String::new(),
            content_sha256: String::new(),
            seal_sha256: String::new(),
            engine: Vec::new(),
            environment: Vec::new(),
            witnesses: Vec::new(),
            history: Vec::new(),
            sources: Vec::new(),
            witness_scripts: Vec::new(),
        };
        let mut entries: Vec<Vec<(String, String)>> = Vec::new();
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
                    let n = name.strip_prefix("History.").and_then(|d| d.parse::<usize>().ok());
                    match n {
                        None => return err(format!("line {number}: unknown section [{name}]")),
                        Some(n) if n != entries.len() + 1 => {
                            return err(format!("line {number}: history entries must be numbered 1, 2, 3, ...; found [{name}]"))
                        }
                        Some(_) => entries.push(Vec::new()),
                    }
                }
                section = name.to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return err(format!("line {number}: expected `key = value`: {line}"));
            };
            let (key, value) = (key.trim().to_string(), value.trim().to_string());
            if !is_name(&key) {
                return err(format!("line {number}: invalid key {key:?}"));
            }
            match section.as_str() {
                "" => return err(format!("line {number}: `{line}` is before any section")),
                "Freeze" => match key.as_str() {
                    "format" => file.format = value.parse().map_err(|_| FrozenError(format!("line {number}: format is not a number")))?,
                    "created" => file.created = value,
                    "content_sha256" => file.content_sha256 = value,
                    "seal_sha256" => file.seal_sha256 = value,
                    other => return err(format!("line {number}: unknown key {other} in [Freeze]")),
                },
                "Engine" => push_unique(&mut file.engine, "Engine", key, value)?,
                "Environment" => push_unique(&mut file.environment, "Environment", key, value)?,
                "Witness" => push_unique(&mut file.witnesses, "Witness", key, value)?,
                _ => {
                    let fields = entries.last_mut().expect("a History section opened it");
                    if !HISTORY_FIELDS.contains(&key.as_str()) {
                        return err(format!("line {number}: unknown key {key} in [{section}]"));
                    }
                    push_unique(fields, &section, key, value)?;
                }
            }
        }
        if file.format != FORMAT_VERSION {
            return err(format!("unsupported format {} (this build reads format {FORMAT_VERSION})", file.format));
        }
        if !is_rfc3339_utc(&file.created) {
            return err(format!("created is not a UTC time like 2026-11-07T14:03:11Z: {:?}", file.created));
        }
        for (key, value) in [("content_sha256", &file.content_sha256), ("seal_sha256", &file.seal_sha256)] {
            if !is_sha256_hex(value) {
                return err(format!("{key} is not a SHA-256 in lowercase hexadecimal: {value:?}"));
            }
        }
        for (index, fields) in entries.into_iter().enumerate() {
            file.history.push(read_entry(index as u32 + 1, fields)?);
        }
        if file.history.is_empty() {
            return err("the history has no entry: a frozen file always records how it was made");
        }
        read_blocks(&lines, at, &mut file)?;
        Ok(file)
    }
}

fn read_entry(n: u32, mut fields: Vec<(String, String)>) -> FrozenResult<HistoryEntry> {
    let mut take = |key: &str| -> FrozenResult<String> {
        match fields.iter().position(|(k, _)| k == key) {
            Some(i) => Ok(fields.remove(i).1),
            None => err(format!("[History.{n}] has no {key}")),
        }
    };
    let entry = HistoryEntry {
        n,
        time: take("time")?,
        action: take("action")?,
        engine: take("engine")?,
        actor: take("actor")?,
        reason: take("reason")?,
        old_content_sha256: take("old_content_sha256")?,
        new_content_sha256: take("new_content_sha256")?,
        diff: take("diff")?,
        prev_entry_sha256: take("prev_entry_sha256")?,
        entry_sha256: take("entry_sha256")?,
    };
    if !is_rfc3339_utc(&entry.time) {
        return err(format!("[History.{n}] time is not a UTC time like 2026-11-07T14:03:11Z: {:?}", entry.time));
    }
    for (key, value, may_be_empty) in [
        ("old_content_sha256", &entry.old_content_sha256, true),
        ("new_content_sha256", &entry.new_content_sha256, false),
        ("prev_entry_sha256", &entry.prev_entry_sha256, true),
        ("entry_sha256", &entry.entry_sha256, false),
    ] {
        if !(value.is_empty() && may_be_empty) && !is_sha256_hex(value) {
            return err(format!("[History.{n}] {key} is not a SHA-256 in lowercase hexadecimal: {value:?}"));
        }
    }
    Ok(entry)
}

/// The `@@` blocks that follow the header.
fn read_blocks(lines: &[&str], mut at: usize, file: &mut FrozenFile) -> FrozenResult<()> {
    while at < lines.len() {
        let number = at + 1;
        let line = lines[at].trim_end();
        at += 1;
        if line.trim().is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split(' ').collect();
        let (end, kind) = match words.as_slice() {
            ["@@", "source", n, origin, sha] => {
                let index: usize = n.parse().map_err(|_| FrozenError(format!("line {number}: bad source marker")))?;
                if index != file.sources.len() + 1 {
                    return err(format!("line {number}: sources must be numbered 1, 2, 3, ...; found {index}"));
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
            _ if line.starts_with("@@ source ") || line.starts_with("@@ witness ") => {
                return err(format!("line {number}: bad block marker: {line}"))
            }
            _ => return err(format!("line {number}: unexpected line outside a block: {line}")),
        };
        let start = at;
        while at < lines.len() && lines[at].trim_end() != end {
            at += 1;
        }
        if at >= lines.len() {
            return err(format!("line {number}: the block has no end marker `{end}`"));
        }
        let text = lines[start..at].join("\n");
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

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn entry(n: u32) -> HistoryEntry {
        HistoryEntry {
            n,
            time: "2026-11-07T14:03:11Z".into(),
            action: "freeze".into(),
            engine: "0.3.1+9f2c1ab".into(),
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
        assert!(parse_err(&good.replace("[Engine]", "[Engin]")).contains("unknown section"));
        assert!(parse_err(&good.replace("version = 0.3.1", "version = 0.3.1\nversion = 0.3.2")).contains("version"));
        assert!(parse_err(&good.replace("format = 1", "format")).contains("key = value"));
        assert!(parse_err(&good.replace("format = 1", "formats = 1")).contains("formats"));
        assert!(parse_err(&good.replace("[History.1]", "[History.2]")).contains("numbered"));
        assert!(parse_err(&good.replace(&h('c'), &"C".repeat(64))).contains("sha256"));
        assert!(parse_err(&good.replace("time = 2026-11-07T14:03:11Z", "time = yesterday")).contains("time"));
    }

    #[test]
    fn the_blocks_are_strict() {
        let good = sample().render().unwrap();
        assert!(parse_err(&good.replace("@@ end source 1", "@@ end source 7")).contains("end"));
        assert!(parse_err(&good.replace("@@ source 2 ", "@@ source 3 ")).contains("numbered"));
        assert!(parse_err(&good.replace("origin=embedded", "origin=em bedded")).contains("marker"));
        assert!(parse_err(&format!("{good}stray line\n")).contains("unexpected"));
        assert!(parse_err(&good.replace("@@ end witness\n", "")).contains("end"));
    }

    #[test]
    fn a_source_cannot_contain_the_framing_marker() {
        assert!(Source::new("overrides", "m = 1\n@@ end source 2\n").is_err());
        assert!(Source::new("over rides", "m = 1").is_err());
        assert!(Source::new("", "m = 1").is_err());
        assert!(WitnessScript::new("a b", "1 m").is_err());
    }

    #[test]
    fn source_text_is_normalised() {
        let a = Source::new("overrides", "m = 1\r\nkg = 2\r\n\r\n").unwrap();
        assert_eq!(a.text, "m = 1\nkg = 2");
        assert_eq!(a.sha256, super::super::canonical::source_sha256("m = 1\nkg = 2"));
    }

    #[test]
    fn values_must_be_one_line_and_trimmed() {
        let mut file = sample();
        file.engine.push(("note".into(), "two\nlines".into()));
        assert!(file.render().is_err());
        let mut file = sample();
        file.engine.push(("note".into(), " padded".into()));
        assert!(file.render().is_err());
        let mut file = sample();
        file.engine.push(("bad key".into(), "x".into()));
        assert!(file.render().is_err());
    }
}
