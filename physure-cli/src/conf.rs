//! `phs conf`: inspect, freeze and verify the frozen configuration (spec §5.4 of
//! `docs/superpowers/specs/2026-11-07-conf-freeze-design.md`, Phase 2 of the conf freeze).
//!
//! Everything that decides lives in the core (`physure_core::frozen::{report, ops}`); this module parses the arguments,
//! calls the core, renders text or one JSON document and picks the exit code: 0 ok, 1 a discrepancy, 2 misuse or a
//! resolution problem or a refused operation, 3 the engine differs from the one the file pins. `main` dispatches
//! `phs conf` before anything else looks at the arguments and before the pre-command check of the configuration in
//! force: these commands exist to look at a file that may be broken or pinned to another engine.

use std::collections::BTreeSet;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process;

use physure_core::frozen::active::resolve_process;
use physure_core::frozen::canonical::canonical_text;
use physure_core::frozen::{
    active, actor_from_env, catalog_entries, compare, diff, embedded_entries, engine_state, ensure_valid, entries_with_origin, freeze, history_rows,
    inspect, now_unix, overrides_contribution, pair_changes, plan_seal, plan_update, section_and_key, select, set_engine_file,
    set_explicit_conf, summary, unfrozen_entries, write_atomic, Change, Comparison, EngineIdentity, EnginePolicy, EngineState, Entry,
    FieldChange, FileStamp, FreezeOptions, FrozenFile, Inspection, OpsError, Resolution, Side, Why,
};
use serde_json::{json, Value};

/// The commands of `phs conf`.
const COMMANDS: [&str; 8] = ["status", "show", "diff", "update", "seal", "history", "freeze", "verify"];

const HELP: &str = "\
phs conf: the frozen configuration (physure.frozen.conf)

USAGE:
    phs conf [status]          Which configuration is in force and why; seal, engine, history, warnings
    phs conf show [SELECTOR] [--origin]
                               The configuration in force as `section.key = value`, sorted: all of it, one section
                               (`constants`) or one key (`constants.molar_gas_constant`); --origin adds the source of
                               each value. A unit's value includes an internal `id` (a hash of this build)
    phs conf diff [A] [B] [--allow-engine-mismatch]
                               What differs between two configurations: data by kind (+ added, - removed,
                               ~ changed old -> new), then engine and environment, with counts. Default: the file in
                               force against what this build gives unfrozen here (`unfrozen` names that side); one
                               argument: that file against it. Exit 0 identical, 1 different or a file that does not
                               verify, 2 nothing to compare; the environment is listed, never a difference
    phs conf update --reason TEXT [--engine-policy binary|build|release|record] [--yes]
                               Adopt what this build brings (its embedded catalog, its engine) into the file in
                               force, keeping the overrides; shows what changes, asks, writes atomically
    phs conf seal --reason TEXT [--yes]
                               Accept a hand edit of the overrides block (source 2) of the file in force: shows the
                               new overrides and what they contribute, asks, re-seals
    phs conf history [FILE]    The history of the file in force (or FILE), problems first
    phs conf freeze --reason TEXT [--engine-policy binary|build|release|record] [--out PATH]
                               Write physure.frozen.conf from what is in force now (default policy: build)
    phs conf verify [FILE] [--expect SEAL] [--allow-engine-mismatch]
                               Check the seal, the hashes, the chain and the engine; compare the seal with SEAL

FLAGS:
    --conf PATH                The frozen file in force (not with freeze, which writes to --out, nor beside a FILE)
    --json                     One JSON document on stdout, with \"schema\": 1 and \"command\"
    --allow-engine-mismatch    status, verify, diff: inspect a file pinned to another engine (the exit code stays 3)
    --yes                      update, seal: do not ask; without it they ask on a terminal and refuse without one
                               (with --json they never ask: --json needs --yes to write)
    -h, --help                 This help

EXIT CODES:
    0 ok (identical, for diff; nothing to do, for update and seal)
    1 a discrepancy (modified file, broken seal, --expect mismatch, differences, a file changed while asking)
    2 misuse, a missing required frozen file, a refused or declined operation, nothing to compare
    3 the engine differs from the pinned one
";

/// The arguments of `phs conf`, after the word `conf`.
#[derive(Debug, Default)]
pub struct ConfArgs {
    /// `status` when none is given.
    pub command: String,
    pub positional: Vec<String>,
    pub conf: Option<PathBuf>,
    pub json: bool,
    pub reason: Option<String>,
    pub policy: Option<EnginePolicy>,
    pub out: Option<PathBuf>,
    pub expect: Option<String>,
    pub yes: bool,
    pub origin: bool,
    pub allow_engine_mismatch: bool,
    pub help: bool,
    /// The flags given, in order, so each command can refuse the ones it does not take.
    given: Vec<&'static str>,
}

const VALUE_FLAGS: [&str; 5] = ["--conf", "--reason", "--engine-policy", "--out", "--expect"];
const SWITCHES: [&str; 6] = ["--json", "--yes", "--origin", "--allow-engine-mismatch", "--help", "-h"];

fn accepted_flags() -> String {
    let mut all: Vec<&str> = VALUE_FLAGS.to_vec();
    all.extend_from_slice(&SWITCHES);
    all.join(", ")
}

/// Parses the arguments that follow `conf`. `Err` is the message, which names the accepted flags.
pub fn parse(args: &[String]) -> Result<ConfArgs, String> {
    let mut parsed = ConfArgs::default();
    let mut command = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_str();
        if let Some(&flag) = VALUE_FLAGS.iter().find(|f| **f == arg) {
            let value = iter
                .next()
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| format!("{flag} expects a value, found none"))?;
            if parsed.given.contains(&flag) {
                return Err(format!("{flag} is given twice; expected it once"));
            }
            parsed.given.push(flag);
            match flag {
                "--conf" => parsed.conf = Some(PathBuf::from(value)),
                "--reason" => parsed.reason = Some(value.clone()),
                "--engine-policy" => parsed.policy = Some(value.parse()?),
                "--out" => parsed.out = Some(PathBuf::from(value)),
                _ => parsed.expect = Some(value.clone()),
            }
        } else if let Some(&switch) = SWITCHES.iter().find(|s| **s == arg) {
            parsed.given.push(switch);
            match switch {
                "--json" => parsed.json = true,
                "--yes" => parsed.yes = true,
                "--origin" => parsed.origin = true,
                "--allow-engine-mismatch" => parsed.allow_engine_mismatch = true,
                _ => parsed.help = true,
            }
        } else if arg.starts_with('-') {
            return Err(format!("unknown flag {arg}; the flags of `phs conf` are: {}", accepted_flags()));
        } else if command.is_none() {
            command = Some(arg.to_string());
        } else {
            parsed.positional.push(arg.to_string());
        }
    }
    parsed.command = command.unwrap_or_else(|| "status".to_string());
    Ok(parsed)
}

/// What a command produced: the text for people, the JSON document, the exit code, and whether the text is an error
/// (stderr) or a report (stdout).
pub struct Outcome {
    pub text: String,
    pub json: Value,
    pub code: i32,
    pub error: bool,
}

/// `{"schema": 1, "command": <command>}` followed by `fields`.
fn document(command: &str, fields: Value) -> Value {
    let mut doc = json!({ "schema": 1, "command": command });
    if let (Some(doc), Value::Object(fields)) = (doc.as_object_mut(), fields) {
        doc.extend(fields);
    }
    doc
}

/// An error: the message on stderr (and in the JSON document as `"error"`), with `code`.
fn failure(command: &str, code: i32, message: impl Into<String>) -> Outcome {
    let message = message.into();
    Outcome { json: document(command, json!({ "error": message, "exit_code": code })), text: message, code, error: true }
}

fn report(command: &str, text: String, fields: Value, code: i32) -> Outcome {
    let mut json = document(command, fields);
    json["exit_code"] = Value::from(code);
    Outcome { text, json, code, error: false }
}

/// `text` with every control character but the line feed written as an escape (`\x1b`, `\x07`, `\u{9b}`): what is
/// printed for people may hold strings from a file received from elsewhere (history, origins, values, problems), and
/// a raw escape sequence there could rewrite the terminal. The JSON document needs no such care (JSON escapes them).
fn escape_controls(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => escaped.push('\n'),
            c if c.is_control() && u32::from(c) < 0x80 => escaped.push_str(&format!("\\x{:02x}", u32::from(c))),
            c if c.is_control() => escaped.push_str(&c.escape_unicode().to_string()),
            c => escaped.push(c),
        }
    }
    escaped
}

/// Writes `text` to `out` and flushes it. A failure to write is not reported, since there is nowhere left to report
/// it: above all a reader that stopped early (`phs conf show | head`, a broken pipe) is a normal end, never a panic,
/// and the exit code stays the command's.
fn emit(mut out: impl Write, text: &str) {
    let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
}

/// [`emit`] to stdout.
fn say(text: &str) {
    emit(std::io::stdout().lock(), text);
}

/// [`emit`] to stderr.
fn say_error(text: &str) {
    emit(std::io::stderr().lock(), text);
}

/// Runs `phs conf ...` (the whole argument list, `phs` and `conf` included), prints its outcome and exits.
pub fn run_conf(args: &[String]) -> ! {
    let rest = args.get(2..).unwrap_or_default();
    let wants_json = rest.iter().any(|a| a == "--json");
    let outcome = match parse(rest) {
        Err(message) => failure("conf", 2, message),
        Ok(parsed) if parsed.help => Outcome { text: HELP.to_string(), json: document("conf", json!({ "help": HELP, "exit_code": 0 })), code: 0, error: false },
        Ok(parsed) => run(&parsed),
    };
    if wants_json && !outcome.json.is_null() {
        say(&format!("{}\n", serde_json::to_string_pretty(&outcome.json).unwrap_or_default()));
        if outcome.error {
            say_error(&format!("phs conf: {}\n", escape_controls(&outcome.text)));
        }
    } else if outcome.error {
        say_error(&format!("phs conf: {}\n", escape_controls(&outcome.text)));
    } else {
        say(&escape_controls(&outcome.text));
    }
    process::exit(outcome.code)
}

/// The flags and the number of positional arguments each command takes.
fn shape(command: &str) -> (&'static [&'static str], usize) {
    match command {
        "status" => (&["--conf", "--json", "--allow-engine-mismatch"], 0),
        "show" => (&["--conf", "--json", "--origin"], 1),
        "diff" => (&["--conf", "--json", "--allow-engine-mismatch"], 2),
        "history" => (&["--conf", "--json"], 1),
        "freeze" => (&["--json", "--reason", "--engine-policy", "--out"], 0),
        "verify" => (&["--conf", "--json", "--expect", "--allow-engine-mismatch"], 1),
        "update" => (&["--conf", "--json", "--reason", "--engine-policy", "--yes"], 0),
        "seal" => (&["--conf", "--json", "--reason", "--yes"], 0),
        _ => (&[], 0),
    }
}

fn run(args: &ConfArgs) -> Outcome {
    let command = args.command.as_str();
    if !COMMANDS.contains(&command) {
        return failure("conf", 2, format!("unknown command {command:?}; the commands are: {}", COMMANDS.join(", ")));
    }
    if command == "freeze" && args.conf.is_some() {
        return failure(
            command,
            2,
            "`phs conf freeze` does not take --conf: it makes a new file from what is in force unfrozen; name where it goes with --out",
        );
    }
    let (flags, positional) = shape(command);
    if let Some(flag) = args.given.iter().find(|f| !flags.contains(f)) {
        return failure(command, 2, format!("`phs conf {command}` does not take {flag}; it takes: {}", flags.join(", ")));
    }
    if args.positional.len() > positional {
        return failure(
            command,
            2,
            format!("`phs conf {command}` takes at most {positional} argument(s), found: {}", args.positional.join(" ")),
        );
    }
    // what is decided without reading anything: misuse comes before any file is read
    if args.conf.is_some() && !args.positional.is_empty() && ["verify", "history", "diff"].contains(&command) {
        return failure(
            command,
            2,
            format!(
                "`phs conf {command}` was given both a FILE ({}) and --conf: which one it acts on would be ambiguous; give one",
                args.positional.join(" ")
            ),
        );
    }
    if ["freeze", "update", "seal"].contains(&command) && args.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
        return failure(command, 2, format!("`phs conf {command}` needs --reason \"...\" (not empty): the history records why"));
    }
    if let Some(control) = args.reason.as_deref().and_then(|r| r.chars().find(|c| c.is_control())) {
        return failure(
            command,
            2,
            format!("--reason must be one line without control characters (the history records it), found the control character {control:?}"),
        );
    }
    if let Some(expected) = &args.expect {
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            return failure(command, 2, format!("--expect takes a seal: 64 hexadecimal characters, found {expected:?}"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        set_engine_file(Some(exe));
    }
    if let Some(path) = &args.conf {
        set_explicit_conf(Some(path.clone()));
    }
    match command {
        "status" => status(args),
        "show" => show(args),
        "diff" => diff_command(args),
        "history" => history(args),
        "freeze" => freeze_command(args),
        "update" => update(args),
        "seal" => seal(args),
        _ => verify(args),
    }
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn why_text(why: &Why) -> &'static str {
    match why {
        Why::Explicit => "--conf",
        Why::Environment => "PHYSURE_CONF",
        Why::Discovered => "discovery",
    }
}

/// The file in force: `Ok(None)` when unfrozen, `Err` an outcome with exit 2 for a resolution problem.
fn in_force(command: &str) -> Result<Option<(PathBuf, Why)>, Outcome> {
    match resolve_process() {
        Ok(Resolution::Frozen { path, why }) => Ok(Some((path, why))),
        Ok(Resolution::Unfrozen) => Ok(None),
        Err(e) => Err(failure(command, 2, e.to_string())),
    }
}

fn changes_json(changes: &[FieldChange]) -> Value {
    Value::Array(changes.iter().map(|c| json!({ "key": c.key, "old": c.old, "new": c.new })).collect())
}

fn engine_json(engine: Option<&EngineState>) -> Value {
    match engine {
        None => Value::Null,
        Some(EngineState::Unreadable(message)) => json!({ "state": "unreadable", "message": message }),
        Some(EngineState::Matches { policy, pinned }) => json!({ "state": "matches", "policy": policy.as_str(), "pinned": pinned }),
        Some(EngineState::Differs { policy, differences }) => {
            json!({ "state": "differs", "policy": policy.as_str(), "differences": differences })
        }
    }
}

fn engine_text(engine: Option<&EngineState>) -> String {
    match engine {
        None => "engine: not checked (the file does not verify)\n".to_string(),
        Some(EngineState::Unreadable(message)) => format!("engine: the [Engine] block does not read: {message}\n"),
        Some(EngineState::Matches { policy, pinned }) => format!("engine: matches (policy {}, pinned {pinned})\n", policy.as_str()),
        Some(EngineState::Differs { policy, differences }) => {
            let mut text = format!("engine: DIFFERS from the one the file pins (policy {}):\n", policy.as_str());
            for difference in differences {
                text.push_str(&format!("  {difference}\n"));
            }
            text
        }
    }
}

/// What a file says about itself, as text and JSON: shared by `status` and `verify`.
fn inspection_report(inspection: &Inspection) -> (String, Value) {
    let mut text = format!("file: {}\n", shown(&inspection.path));
    if let Some(message) = &inspection.unreadable {
        text.push_str(&format!("UNREADABLE: {message}\n"));
        let fields = json!({ "path": shown(&inspection.path), "unreadable": message, "verified": false, "engine_matches": false });
        return (text, fields);
    }
    let file = inspection.file.as_ref();
    let seal = file.map(|f| f.seal_sha256().to_string()).unwrap_or_default();
    let created = file.map(|f| f.created().to_string()).unwrap_or_default();
    let history = file.map_or(0, |f| f.history().len());
    text.push_str(&format!("seal: {seal}\ncreated: {created}\nhistory: {history} entries\n"));
    if inspection.problems.is_empty() {
        text.push_str("verification: the seal, the hashes and the chain hold\n");
    } else {
        text.push_str("verification: the file does not verify (modified since the last seal):\n");
        for problem in &inspection.problems {
            text.push_str(&format!("  {problem}\n"));
        }
    }
    text.push_str(&engine_text(inspection.engine.as_ref()));
    let catalog_problems: Vec<String> = match &inspection.catalog {
        Some(Err(problems)) => problems.clone(),
        _ => Vec::new(),
    };
    match &inspection.catalog {
        Some(Ok(_)) => text.push_str(&format!(
            "atomic-weight edition: {}\n",
            inspection.atomic_weight_edition.as_deref().unwrap_or("unknown")
        )),
        Some(Err(problems)) => {
            text.push_str("catalog: the sources do not replay:\n");
            for problem in problems {
                text.push_str(&format!("  {problem}\n"));
            }
        }
        None => text.push_str("catalog: not replayed (the file does not verify)\n"),
    }
    if !inspection.environment.is_empty() {
        text.push_str("warning: the environment differs from the one recorded (informational, outside the seal):\n");
        for change in &inspection.environment {
            text.push_str(&format!("  {}: recorded {:?}, here {:?}\n", change.key, change.old, change.new));
        }
    }
    let fields = json!({
        "path": shown(&inspection.path),
        "seal": seal,
        "created": created,
        "history_entries": history,
        "verified": inspection.problems.is_empty(),
        "problems": inspection.problems,
        "engine": engine_json(inspection.engine.as_ref()),
        "engine_matches": matches!(inspection.engine, Some(EngineState::Matches { .. })),
        "catalog_problems": catalog_problems,
        "atomic_weight_edition": inspection.atomic_weight_edition,
        "environment": changes_json(&inspection.environment),
        "sources": file.map_or_else(Vec::new, |f| f
            .sources()
            .iter()
            .map(|s| json!({ "origin": s.origin(), "sha256": s.sha256() }))
            .collect::<Vec<_>>()),
    });
    (text, fields)
}

fn engine_file() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// `phs conf` / `phs conf status`.
fn status(args: &ConfArgs) -> Outcome {
    let cwd = std::env::current_dir().ok();
    let local = cwd.as_ref().map(|cwd| cwd.join("physure.conf")).filter(|p| p.is_file());
    let (path, why) = match in_force("status") {
        Err(outcome) => return outcome,
        Ok(None) => return status_unfrozen(cwd.as_deref(), local.as_deref()),
        Ok(Some(found)) => found,
    };
    let inspection = inspect(&path, engine_file().as_deref());
    let code = inspection.exit_code();
    let (body, mut fields) = inspection_report(&inspection);
    let mut text = format!("frozen configuration in force ({})\n{body}", why_text(&why));
    let mut warnings = Vec::new();
    if let Some(local) = &local {
        warnings.push(format!(
            "{} is ignored: the frozen file is in force; to adopt an edit, change the overrides of the frozen file and run `phs conf seal`",
            shown(local)
        ));
    }
    if args.allow_engine_mismatch && code == 3 {
        warnings.push("--allow-engine-mismatch: inspected under another engine; the exit code still says so (3)".to_string());
    }
    for warning in &warnings {
        text.push_str(&format!("warning: {warning}\n"));
    }
    fields["frozen"] = Value::Bool(true);
    fields["why"] = Value::from(why_text(&why));
    fields["warnings"] = json!(warnings);
    report("status", text, fields, code)
}

/// `phs conf status` with no frozen file in force: the embedded catalog, and the working directory's `physure.conf`
/// read as the runtime reads it ([`unfrozen_entries`]), with what that reading warns about. A file the runtime ignores
/// as a whole (not UTF-8 text) is said to be ignored, never "read as an overlay". The exit code stays 0: the runtime
/// works, with the embedded catalog.
fn status_unfrozen(cwd: Option<&Path>, local: Option<&Path>) -> Outcome {
    let mut text = "no frozen configuration in force: the embedded catalog".to_string();
    let mut warnings = Vec::new();
    match (cwd, local) {
        (Some(cwd), Some(local)) => match unfrozen_entries(cwd) {
            Ok((_, found)) => {
                text.push_str(&format!(", with {} read as an overlay of it\n", shown(local)));
                warnings = found;
            }
            Err(message) => {
                text.push_str(&format!(" alone ({} is not used)\n", shown(local)));
                warnings.push(message);
            }
        },
        _ => text.push_str(" (no physure.conf here)\n"),
    }
    for warning in &warnings {
        text.push_str(&format!("warning: {warning}\n"));
    }
    let fields = json!({ "frozen": false, "physure_conf": local.map(shown), "warnings": warnings });
    report("status", text, fields, 0)
}

/// The file a command acts on: the positional FILE, or the file in force (exit 2 when there is none).
fn target(command: &str, args: &ConfArgs) -> Result<PathBuf, Outcome> {
    if let Some(file) = args.positional.first() {
        return Ok(PathBuf::from(file));
    }
    match in_force(command)? {
        Some((path, _)) => Ok(path),
        None => Err(failure(command, 2, format!("no frozen configuration in force; nothing to {command} (name a FILE, or use --conf)"))),
    }
}

/// `phs conf history [FILE]`: the problems of the file first, then its rows; the exit code is the file's soundness.
fn history(args: &ConfArgs) -> Outcome {
    let path = match target("history", args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let inspection = inspect(&path, engine_file().as_deref());
    let code = inspection.exit_code();
    let mut text = format!("file: {}\n", shown(&path));
    if let Some(message) = &inspection.unreadable {
        text.push_str(&format!("UNREADABLE: {message}\n"));
        return report("history", text, json!({ "path": shown(&path), "unreadable": message, "rows": [] }), code);
    }
    if !inspection.problems.is_empty() {
        text.push_str("the file does not verify; rows marked unverified are not vouched for by its chain:\n");
        for problem in &inspection.problems {
            text.push_str(&format!("  {problem}\n"));
        }
    }
    let rows = inspection.file.as_ref().map(history_rows).unwrap_or_default();
    let mut json_rows = Vec::new();
    for row in &rows {
        text.push_str(&format!(
            "{:>3}  {}  {:<6}  {}  engine {}  policy {}  pinned {}{}\n     reason: {}\n     diff: {}\n",
            row.n,
            row.time,
            row.action,
            row.actor,
            row.engine,
            row.policy,
            row.pinned,
            if row.unverified { "  [unverified]" } else { "" },
            row.reason,
            row.diff
        ));
        if row.engine_block_changed {
            text.push_str("     the [Engine] block changed\n");
        }
        if !row.pin_change.is_empty() {
            text.push_str(&format!("     pin: {}\n", row.pin_change));
        }
        json_rows.push(json!({
            "n": row.n, "time": row.time, "action": row.action, "actor": row.actor, "reason": row.reason,
            "engine": row.engine, "policy": row.policy, "pinned": row.pinned, "diff": row.diff,
            "engine_block_changed": row.engine_block_changed, "pin_change": row.pin_change, "unverified": row.unverified,
        }));
    }
    let fields = json!({ "path": shown(&path), "problems": inspection.problems, "rows": json_rows });
    report("history", text, fields, code)
}

/// Catalog entries, each with the origin it is shown with.
type Listing = Vec<(Entry, String)>;

/// The sections a `show` of the catalog groups several kinds under: the kind is shown with the key there.
const POOLED_SECTIONS: [&str; 3] = ["units", "atomic_weights", "settings"];

/// `phs conf show [SELECTOR] [--origin]`: the catalog in force. Unlike the other commands it needs the configuration
/// in force to be usable (decision 2 of the Phase 2 plan), so a broken file is its exit code (1, 2 or 3).
fn show(args: &ConfArgs) -> Outcome {
    if let Err(e) = ensure_valid() {
        return failure("show", e.exit_code(), e.to_string());
    }
    let selector = args.positional.first().map_or("", String::as_str);
    let (listed, warnings, path): (Listing, Vec<String>, Option<PathBuf>) = match active() {
        Err(e) => return failure("show", e.exit_code(), e.to_string()),
        Ok(Some(active)) => match entries_with_origin(&active.catalog.file) {
            Ok(found) => (found.into_iter().map(|(entry, origin)| (entry, origin.to_string())).collect(), Vec::new(), Some(active.path.clone())),
            Err((source, problems)) => {
                return failure("show", 1, format!("{}: source {source} does not replay: {}", shown(&active.path), problems.join("; ")))
            }
        },
        Ok(None) => match unfrozen_listing() {
            Ok((found, warnings)) => (found, warnings, None),
            Err(message) => return failure("show", 1, message),
        },
    };
    let chosen = select(&listed, selector);
    if chosen.is_empty() {
        let sections: BTreeSet<String> = listed.iter().map(|(entry, _)| section_and_key(entry).0).collect();
        return failure(
            "show",
            2,
            format!(
                "no section or key {selector:?}: expected a section, a `section.key` or a key prefix; the sections are: {}",
                sections.into_iter().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    let mut rows: Vec<(String, String, &str, &str, &str)> = chosen
        .iter()
        .map(|(entry, origin)| {
            let (section, key) = section_and_key(entry);
            (section, key, entry.kind, entry.value.as_str(), origin.as_str())
        })
        .collect();
    rows.sort();
    let mut text = String::new();
    let mut json_rows = Vec::with_capacity(rows.len());
    for (section, key, kind, value, origin) in &rows {
        let kind_shown = if POOLED_SECTIONS.contains(&section.as_str()) { format!(" ({kind})") } else { String::new() };
        let origin_shown = if args.origin { format!("  [{origin}]") } else { String::new() };
        text.push_str(&format!("{section}.{key}{kind_shown} = {value}{origin_shown}
"));
        let mut row = json!({ "section": section, "key": key, "kind": kind, "value": value });
        if args.origin {
            row["origin"] = Value::from(*origin);
        }
        json_rows.push(row);
    }
    if !args.json {
        for warning in &warnings {
            say_error(&format!("phs conf: warning: {}\n", escape_controls(warning)));
        }
    }
    let fields = json!({
        "frozen": path.is_some(),
        "path": path.as_deref().map(shown),
        "selector": selector,
        "entries": json_rows,
        "warnings": warnings,
    });
    report("show", text, fields, 0)
}

/// The unfrozen catalog of the working directory as the runtime builds it, each entry with its origin: `embedded`
/// when the shipped catalog has it with that value, `physure.conf` otherwise; and the warnings of that reading.
fn unfrozen_listing() -> Result<(Listing, Vec<String>), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("the current directory cannot be read: {e}"))?;
    let (found, warnings) = unfrozen_entries(&cwd)?;
    let shipped = embedded_entries().map_err(|problems| format!("the embedded catalog does not replay: {}", problems.join("; ")))?;
    let shipped: BTreeSet<Entry> = shipped.into_iter().collect();
    let listed = found
        .into_iter()
        .map(|entry| {
            let origin = if shipped.contains(&entry) { "embedded" } else { "physure.conf" };
            (entry, origin.to_string())
        })
        .collect();
    Ok((listed, warnings))
}

/// A value of a field as text shows it: a key present on one side only has nothing on the other.
fn or_none(value: &str) -> &str {
    if value.is_empty() {
        "(none)"
    } else {
        value
    }
}

/// The catalog differences, grouped by kind (`+` added, `-` removed, `~` changed `old -> new`), as text and JSON.
fn changes_report(changes: &[Change]) -> (String, Value) {
    let mut text = String::new();
    let mut rows = Vec::with_capacity(changes.len());
    let mut kind = "";
    for change in changes {
        if change.kind() != kind {
            kind = change.kind();
            text.push_str(&format!("  {kind}:\n"));
        }
        let (what, old, new) = match change {
            Change::Added(entry) => {
                text.push_str(&format!("    + {} = {}\n", entry.name, entry.value));
                ("added", Value::Null, Value::from(entry.value.as_str()))
            }
            Change::Removed(entry) => {
                text.push_str(&format!("    - {} = {}\n", entry.name, entry.value));
                ("removed", Value::from(entry.value.as_str()), Value::Null)
            }
            Change::Changed { name, old, new, .. } => {
                text.push_str(&format!("    ~ {name}: {old} -> {new}\n"));
                ("changed", Value::from(old.as_str()), Value::from(new.as_str()))
            }
        };
        rows.push(json!({ "change": what, "kind": change.kind(), "name": change.name(), "old": old, "new": new }));
    }
    (text, Value::Array(rows))
}

/// Field differences (`[Engine]`, `[Environment]`) as text lines.
fn field_lines(changes: &[FieldChange]) -> String {
    changes.iter().map(|c| format!("  {}: {} -> {}\n", c.key, or_none(&c.old), or_none(&c.new))).collect()
}

/// What `diff` says of a comparison: text, JSON fields, and the exit code (0 identical, 1 otherwise). The environment
/// is listed but never makes two sides different (it is outside the seal, informational everywhere).
fn comparison_report(comparison: &Comparison, default: bool) -> (String, Value, i32) {
    let mut text = format!("a: {}\nb: {}\n", comparison.a, comparison.b);
    if default {
        text.push_str(
            "note: b is what this build gives unfrozen in this directory, so the data below covers both what a library \
             upgrade changes and any drift of this directory's physure.conf from the frozen overrides (the frozen file \
             ignores physure.conf)\n",
        );
    }
    for (side, problems) in [("a", &comparison.a_problems), ("b", &comparison.b_problems)] {
        if !problems.is_empty() {
            text.push_str(&format!("{side} does not verify, so its catalog was not compared:\n"));
            for problem in problems {
                text.push_str(&format!("  {problem}\n"));
            }
        }
    }
    let (data_text, data) = changes_report(&comparison.data);
    let counts = summary(&comparison.data);
    text.push_str(&format!("data differences ({}): {counts}\n{data_text}", comparison.data.len()));
    text.push_str(&format!("engine differences ({}):\n{}", comparison.engine.len(), field_lines(&comparison.engine)));
    text.push_str(&format!(
        "environment differences ({}, informational):\n{}",
        comparison.environment.len(),
        field_lines(&comparison.environment)
    ));
    let identical = comparison.is_identical();
    text.push_str(if identical { "identical\n" } else { "different\n" });
    for warning in &comparison.warnings {
        text.push_str(&format!("warning: {warning}\n"));
    }
    let fields = json!({
        "a": comparison.a,
        "b": comparison.b,
        "identical": identical,
        "data": data,
        "engine": changes_json(&comparison.engine),
        "environment": changes_json(&comparison.environment),
        "environment_identical": comparison.environment.is_empty(),
        "summary": counts,
        "warnings": comparison.warnings,
        "a_problems": comparison.a_problems,
        "b_problems": comparison.b_problems,
    });
    (text, fields, if identical { 0 } else { 1 })
}

/// `phs conf diff [A] [B] [--allow-engine-mismatch]`. No engine is checked (the blocks are compared as written), so
/// `--allow-engine-mismatch` changes nothing, the exit code included.
fn diff_command(args: &ConfArgs) -> Outcome {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => return failure("diff", 2, format!("the current directory cannot be read: {e}")),
    };
    let unfrozen = || Side::Unfrozen { cwd: cwd.clone() };
    let side = |arg: &str| if arg == "unfrozen" { unfrozen() } else { Side::File(PathBuf::from(arg)) };
    let (a, b) = match args.positional.as_slice() {
        [] => match in_force("diff") {
            Err(outcome) => return outcome,
            Ok(None) => {
                return failure(
                    "diff",
                    2,
                    "no frozen configuration in force: nothing to compare (name a FILE, or two: A B; `unfrozen` stands for what this build gives unfrozen here)",
                )
            }
            Ok(Some((path, _))) => (Side::File(path), unfrozen()),
        },
        [one] => (side(one), unfrozen()),
        [one, two, ..] => (side(one), side(two)),
    };
    match compare(&a, &b, engine_file().as_deref()) {
        Ok(comparison) => {
            let (text, fields, code) = comparison_report(&comparison, args.positional.is_empty());
            report("diff", text, fields, code)
        }
        Err(message) => failure("diff", 1, message),
    }
}

/// The exit code of a refused operation (decision S2 of the Phase 2 plan): what the operation will not do, or a
/// resolution problem, is 2; a file or a text that does not read, verify or replay is a discrepancy, 1.
fn ops_code(error: &OpsError) -> i32 {
    match error {
        OpsError::AlreadyFrozen(_) | OpsError::WouldOverwrite(_) | OpsError::Refused(_) | OpsError::Engine(_) | OpsError::Resolve(_) => 2,
        OpsError::Conf { .. } | OpsError::Load(_) | OpsError::Io { .. } | OpsError::Frozen(_) => 1,
    }
}

/// `phs conf freeze --reason R [--engine-policy P] [--out PATH]`.
fn freeze_command(args: &ConfArgs) -> Outcome {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => return failure("freeze", 2, format!("the current directory cannot be read: {e}")),
    };
    let exe = engine_file();
    let policy = args.policy.unwrap_or_default();
    let actor = actor_from_env(|name| std::env::var(name).ok());
    let options = FreezeOptions {
        cwd: &cwd,
        out: args.out.clone(),
        reason: args.reason.as_deref().unwrap_or_default().trim(),
        policy,
        engine_file: exe.as_deref(),
        time_unix: now_unix(),
        actor: &actor,
    };
    let written = freeze(&options).and_then(|(file, path)| {
        write_atomic(&path, &file.render()?, false)?;
        Ok((file, path))
    });
    let (file, path) = match written {
        Ok(done) => done,
        Err(e) => return failure("freeze", ops_code(&e), e.to_string()),
    };
    let engine = EngineIdentity::current().short();
    let seal = file.seal_sha256().to_string();
    let text = format!(
        "wrote {}
seal: {seal}
  record this seal outside this directory: `phs conf verify --expect SEAL` checks the file against it
engine: {engine} (policy {})
",
        shown(&path),
        policy.as_str()
    );
    let sources: Vec<Value> = file.sources().iter().map(|s| json!({ "origin": s.origin(), "sha256": s.sha256() })).collect();
    let fields = json!({ "path": shown(&path), "seal": seal, "engine": engine, "policy": policy.as_str(), "sources": sources });
    report("freeze", text, fields, 0)
}

/// `phs conf verify [FILE] [--expect SEAL] [--allow-engine-mismatch]`.
fn verify(args: &ConfArgs) -> Outcome {
    let path = match target("verify", args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let inspection = inspect(&path, engine_file().as_deref());
    let mut code = inspection.exit_code();
    let (mut text, mut fields) = inspection_report(&inspection);
    fields["seal_matches"] = Value::Null;
    if let Some(expected) = &args.expect {
        let matches = inspection.seal_matches(expected);
        fields["seal_matches"] = Value::Bool(matches);
        if matches {
            text.push_str("expected seal: matches
");
        } else {
            code = 1;
            let why = if inspection.unreadable.is_some() || !inspection.problems.is_empty() {
                " (the file does not verify, so its stored seal vouches for nothing)"
            } else {
                ""
            };
            text.push_str(&format!("expected seal: the seal does not match --expect {expected}{why}
"));
        }
    }
    if args.allow_engine_mismatch && code == 3 {
        text.push_str("warning: --allow-engine-mismatch: inspected under another engine; the exit code still says so (3)
");
    }
    report("verify", text, fields, code)
}

/// Residual risk 1c-14 of the spec: the file must be as the plan read it. Its length and modification time now are
/// compared with `stamp` (from the handle the plan read it through); `Err` when either moved, or when the file cannot
/// be examined.
fn unchanged_since(path: &Path, stamp: &FileStamp) -> Result<(), String> {
    let now = std::fs::metadata(path).map(|metadata| FileStamp::of(&metadata)).map_err(|e| {
        format!("{}: cannot be examined before writing ({e}); nothing written", shown(path))
    })?;
    if now == *stamp {
        Ok(())
    } else {
        Err(format!(
            "{}: changed since it was read (expected length {} and modification time {:?}, found {} and {:?}); nothing written: run the command again",
            shown(path),
            stamp.len,
            stamp.modified,
            now.len,
            now.modified
        ))
    }
}

/// Writes a planned file in place of the one at `path`, atomically, only if that file is unchanged since the plan
/// read it ([`unchanged_since`]). A refusal is exit 1: the file moved under the command (a discrepancy).
fn write_planned(command: &str, path: &Path, file: &FrozenFile, stamp: &FileStamp) -> Result<(), Outcome> {
    let text = file.render().map_err(|e| failure(command, 1, format!("the planned file does not render: {e}; nothing written")))?;
    unchanged_since(path, stamp).map_err(|message| failure(command, 1, message))?;
    write_atomic(path, &text, true).map_err(|e| failure(command, ops_code(&e), format!("{e}; nothing written")))
}

/// Decision 7 of the Phase 2 plan: `--yes` never asks; otherwise the question goes to stderr (stdout stays the
/// report, or the one JSON document) and only `y` or `yes` on a terminal proceeds. Without a terminal it refuses
/// rather than read piped input. `Err` is why nothing is written (exit 2).
fn confirm(yes: bool, what: &str) -> Result<(), String> {
    if yes {
        return Ok(());
    }
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return Err(format!("{what}: refusing to proceed without a terminal to confirm on; pass --yes in scripts; nothing written"));
    }
    let _ = std::io::stdout().flush();
    say_error(&format!("{}. Proceed? [y/N] ", escape_controls(what)));
    let mut answer = String::new();
    stdin.read_line(&mut answer).map_err(|e| format!("{what}: the answer cannot be read ({e}); nothing written"))?;
    match answer.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err(format!("{what}: declined; nothing written")),
    }
}

/// Merges the members of `extra` into the object `into`.
fn merge(into: &mut Value, extra: &Value) {
    if let (Some(into), Some(extra)) = (into.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            into.insert(key.clone(), value.clone());
        }
    }
}

/// The end of `update` and `seal`: the preview is printed (text) or carried in the document (JSON), the person
/// confirms, the file is written if it did not change since it was read, and the new seal is printed. With `--json`
/// the preview is printed only at the end, inside the document, so it never asks: without `--yes` it refuses (exit 2)
/// with the preview in the document.
fn apply_plan(command: &str, args: &ConfArgs, path: &Path, preview: (String, Value), file: &FrozenFile, stamp: &FileStamp) -> Outcome {
    let (preview_text, preview_fields) = preview;
    if !args.json {
        say(&escape_controls(&preview_text));
    }
    let asked = if args.json && !args.yes {
        Err(format!(
            "with --json, `phs conf {command}` never asks (the preview is in this document, printed at the end): pass --yes to write {}; nothing written",
            shown(path)
        ))
    } else {
        confirm(args.yes, &format!("`phs conf {command}` rewrites {}", shown(path)))
    };
    let written = asked
        .map_err(|message| failure(command, 2, message))
        .and_then(|()| write_planned(command, path, file, stamp));
    if let Err(mut outcome) = written {
        merge(&mut outcome.json, &preview_fields);
        outcome.json["written"] = Value::Bool(false);
        return outcome;
    }
    let seal = file.seal_sha256();
    let text = format!(
        "wrote {}\nseal: {seal}\n  record this seal outside this directory: `phs conf verify --expect SEAL` checks the file against it\n",
        shown(path)
    );
    let mut fields = json!({ "path": shown(path), "seal": seal, "written": true });
    merge(&mut fields, &preview_fields);
    report(command, text, fields, 0)
}

/// The catalog entries of a file, or why they cannot be shown.
fn shown_entries(file: &FrozenFile, which: &str) -> Result<Vec<Entry>, String> {
    catalog_entries(file).map_err(|(source, problems)| {
        format!("the catalog of {which} does not replay with this build (source {source}: {}), so its data cannot be compared", problems.join("; "))
    })
}

/// `phs conf update --reason R [--engine-policy P] [--yes]`: the file in force adopts this build's embedded catalog
/// and engine (the overrides are kept). What is shown is the plan's own difference, from memory: the catalog of the
/// file as read against the catalog of the file to write, and the `[Engine]` pairs that differ.
fn update(args: &ConfArgs) -> Outcome {
    let path = match target("update", args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let reason = args.reason.as_deref().unwrap_or_default().trim();
    let actor = actor_from_env(|name| std::env::var(name).ok());
    let plan = match plan_update(&path, reason, args.policy, engine_file().as_deref(), now_unix(), &actor) {
        Ok(Some(plan)) => plan,
        Ok(None) => {
            let text = format!("{}: nothing to update: the embedded catalog and the engine are this build's\n", shown(&path));
            return report("update", text, json!({ "path": shown(&path), "written": false, "nothing_to_do": true }), 0);
        }
        Err(e) => return failure("update", ops_code(&e), e.to_string()),
    };
    let mut text = format!("update: {}\nsummary: {}\n", shown(&path), plan.summary);
    let mut fields = json!({ "summary": plan.summary, "nothing_to_do": false });
    match shown_entries(&plan.old, "the file as it is").and_then(|old| Ok((old, shown_entries(&plan.file, "the updated file")?))) {
        Ok((old, new)) => {
            let changes = diff(&old, &new);
            let (changes_text, data) = changes_report(&changes);
            text.push_str(&format!("data differences ({}): {}\n{changes_text}", changes.len(), summary(&changes)));
            fields["data"] = data;
        }
        Err(message) => {
            text.push_str(&format!("data differences: not shown: {message}\n"));
            fields["data"] = Value::Null;
            fields["data_problem"] = Value::from(message);
        }
    }
    let engine = pair_changes(plan.old.engine(), plan.file.engine());
    text.push_str(&format!("engine differences ({}):\n{}", engine.len(), field_lines(&engine)));
    fields["engine"] = changes_json(&engine);
    apply_plan("update", args, &path, (text, fields), &plan.file, &plan.stamp)
}

/// `phs conf seal --reason R [--yes]`: accepts a hand edit of the overrides of the file in force. The file keeps only
/// the marker of the overrides it last sealed, not their text, so what is shown is (a) the canonical text of the
/// overrides as they will be sealed and (b) what they contribute to the catalog (sources without them against with
/// them); a copy of the previously sealed file can be compared with `phs conf diff`.
fn seal(args: &ConfArgs) -> Outcome {
    let path = match target("seal", args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let reason = args.reason.as_deref().unwrap_or_default().trim();
    let actor = actor_from_env(|name| std::env::var(name).ok());
    let plan = match plan_seal(&path, reason, now_unix(), &actor) {
        Ok(Some(plan)) => plan,
        Ok(None) => {
            let text = format!("{}: nothing to seal: the file already verifies\n", shown(&path));
            return report("seal", text, json!({ "path": shown(&path), "written": false, "nothing_to_do": true }), 0);
        }
        Err(e) => return failure("seal", ops_code(&e), e.to_string()),
    };
    // the plan proved that only source 2 changed, so the engine block is the sealed one: a seal re-records it under
    // this build, which must be the engine it pins (the only way onto another engine is `update`, spec §7)
    let refused = match engine_state(&plan.old, engine_file().as_deref()) {
        EngineState::Matches { .. } => None,
        EngineState::Differs { differences, .. } => Some((3, format!("the engine differs: {}", differences.join("; ")))),
        EngineState::Unreadable(message) => Some((1, format!("the [Engine] block does not read: {message}"))),
    };
    if let Some((code, why)) = refused {
        let mut outcome = failure(
            "seal",
            code,
            format!(
                "{}: the pinned engine is not this build ({why}); run `phs conf update` first, the only way onto another engine (it takes the file as last sealed: set your edit of the overrides aside, update, then edit again and seal); nothing written",
                shown(&path)
            ),
        );
        outcome.json["written"] = Value::Bool(false);
        return outcome;
    }
    let overrides = canonical_text(plan.file.sources()[1].text());
    let mut text = format!("seal: {}\nsummary: {}\nthe overrides (source 2) as they will be sealed, canonical:\n", shown(&path), plan.summary);
    for line in overrides.lines() {
        text.push_str(&format!("  | {line}\n"));
    }
    let mut fields = json!({ "summary": plan.summary, "overrides": overrides, "nothing_to_do": false });
    match overrides_contribution(&plan.file) {
        Ok(changes) => {
            let (changes_text, data) = changes_report(&changes);
            text.push_str(&format!(
                "what the overrides contribute to the catalog ({}): {}\n{changes_text}",
                changes.len(),
                summary(&changes)
            ));
            fields["data"] = data;
        }
        Err((source, problems)) => {
            // the plan was validated, so this is not expected; said, not hidden
            let message = format!("source {source} does not replay: {}", problems.join("; "));
            text.push_str(&format!("what the overrides contribute: not shown: {message}\n"));
            fields["data"] = Value::Null;
            fields["data_problem"] = Value::from(message);
        }
    }
    apply_plan("seal", args, &path, (text, fields), &plan.file, &plan.stamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_reads_command_flags_and_values() {
        let parsed = parse(&args(&["verify", "f.conf", "--expect", "ab", "--json", "--engine-policy", "record"])).unwrap();
        assert_eq!(parsed.command, "verify");
        assert_eq!(parsed.positional, vec!["f.conf"]);
        assert_eq!(parsed.expect.as_deref(), Some("ab"));
        assert!(parsed.json);
        assert_eq!(parsed.policy, Some(EnginePolicy::Record));
        assert_eq!(parse(&[]).unwrap().command, "status");
        // a value that looks like a word another command claims is a value
        assert_eq!(parse(&args(&["freeze", "--reason", "help"])).unwrap().reason.as_deref(), Some("help"));
    }

    /// Residual risk 1c-14: a file changed between the plan and the write is not overwritten.
    #[test]
    fn a_file_changed_since_the_plan_read_it_is_not_written() {
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!("physure_cli_cas_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let options = FreezeOptions {
            cwd: &dir,
            out: None,
            reason: "first",
            policy: EnginePolicy::Release,
            engine_file: None,
            time_unix: now_unix(),
            actor: "t@h",
        };
        let (file, path) = freeze(&options).unwrap();
        write_atomic(&path, &file.render().unwrap(), false).unwrap();
        let read = std::fs::read_to_string(&path).unwrap();
        let edited = read.replace(physure_core::frozen::EMPTY_OVERRIDES.trim_end(), "[Constants]\nzz_c = 3 m");
        std::fs::write(&path, &edited).unwrap();
        let plan = plan_seal(&path, "mine", now_unix() + 1, "t@h").unwrap().unwrap();
        // another writer appends to the file after the plan read it: a different length
        std::fs::write(&path, format!("{edited}\n# someone else")).unwrap();
        let refused = write_planned("seal", &path, &plan.file, &plan.stamp).expect_err("refused");
        assert_eq!(refused.code, 1);
        assert!(refused.text.contains("changed since it was read") && refused.text.contains("nothing written"), "{}", refused.text);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("{edited}\n# someone else"), "not overwritten");
        // the same length as read, but written later: the modification time moved
        std::fs::write(&path, &edited).unwrap();
        let later = plan.stamp.modified.unwrap() + Duration::from_secs(5);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(later).unwrap();
        assert!(unchanged_since(&path, &plan.stamp).unwrap_err().contains("changed since it was read"));
        assert!(write_planned("seal", &path, &plan.file, &plan.stamp).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited, "not overwritten");
        // as it was read: written
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(plan.stamp.modified.unwrap()).unwrap();
        assert!(write_planned("seal", &path, &plan.file, &plan.stamp).is_ok());
        assert!(FrozenFile::parse(&std::fs::read_to_string(&path).unwrap()).unwrap().verify().is_empty());
        // a file that is gone cannot be examined
        let missing = dir.join("missing.conf");
        assert!(unchanged_since(&missing, &plan.stamp).unwrap_err().contains("cannot be examined"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn control_characters_are_escaped_and_lines_kept() {
        assert_eq!(escape_controls("a\u{1b}[31mb\nc\td\u{7f}\u{9b}é"), "a\\x1b[31mb\nc\\x09d\\x7f\\u{9b}é");
        assert_eq!(escape_controls("plain\n"), "plain\n");
    }

    /// A writer whose reader went away.
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn a_broken_pipe_is_not_a_panic() {
        emit(Closed, "anything\n");
        let mut written = Vec::new();
        emit(&mut written, "text\n");
        assert_eq!(written, b"text\n");
    }

    #[test]
    fn yes_never_asks() {
        assert_eq!(confirm(true, "x"), Ok(()));
    }

    #[test]
    fn parse_refuses_unknown_flags_missing_values_and_repeats() {
        assert!(parse(&args(&["-V"])).unwrap_err().contains("--json"));
        assert!(parse(&args(&["status", "--conf"])).unwrap_err().contains("expects a value"));
        assert!(parse(&args(&["status", "--conf", "--json"])).unwrap_err().contains("expects a value"));
        assert!(parse(&args(&["--out", "a", "--out", "b"])).unwrap_err().contains("twice"));
        assert!(parse(&args(&["--engine-policy", "strict"])).unwrap_err().contains("binary, build, release or record"));
    }
}
