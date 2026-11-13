//! `phs conf`: inspect, freeze and verify the frozen configuration (spec §5.4 of
//! `docs/superpowers/specs/2026-11-07-conf-freeze-design.md`, Phase 2 of the conf freeze).
//!
//! Everything that decides lives in the core (`physure_core::frozen::{report, ops}`); this module parses the arguments,
//! calls the core, renders text or one JSON document and picks the exit code: 0 ok, 1 a discrepancy, 2 misuse or a
//! resolution problem or a refused operation, 3 the engine differs from the one the file pins. `main` dispatches
//! `phs conf` before anything else looks at the arguments and before the pre-command check of the configuration in
//! force: these commands exist to look at a file that may be broken or pinned to another engine.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process;

use physure_core::frozen::active::resolve_process;
use physure_core::frozen::{
    active, actor_from_env, embedded_entries, ensure_valid, entries_with_origin, freeze, history_rows, inspect, now_unix, section_and_key,
    select, set_engine_file, set_explicit_conf, unfrozen_entries, write_atomic, EngineIdentity, EnginePolicy, EngineState, Entry,
    FieldChange, FreezeOptions, Inspection, OpsError, Resolution, Why,
};
use serde_json::{json, Value};

/// The commands this build has.
const COMMANDS: [&str; 5] = ["status", "show", "history", "freeze", "verify"];
/// The commands of the design that this build does not have yet: they refuse instead of doing something else.
const PLANNED: [&str; 3] = ["diff", "update", "seal"];

const HELP: &str = "\
phs conf: the frozen configuration (physure.frozen.conf)

USAGE:
    phs conf [status]          Which configuration is in force and why; seal, engine, history, warnings
    phs conf show [SELECTOR] [--origin]
                               The configuration in force as `section.key = value`, sorted: all of it, one section
                               (`constants`) or one key (`constants.molar_gas_constant`); --origin adds the source of
                               each value. A unit's value includes an internal `id` (a hash of this build)
    phs conf history [FILE]    The history of the file in force (or FILE), problems first
    phs conf freeze --reason TEXT [--engine-policy binary|build|release|record] [--out PATH]
                               Write physure.frozen.conf from what is in force now (default policy: build)
    phs conf verify [FILE] [--expect SEAL] [--allow-engine-mismatch]
                               Check the seal, the hashes, the chain and the engine; compare the seal with SEAL
    phs conf diff | update | seal
                               Planned; not available in this build

FLAGS:
    --conf PATH                The frozen file in force (not with freeze, which writes to --out)
    --json                     One JSON document on stdout, with \"schema\": 1 and \"command\"
    --allow-engine-mismatch    status, verify: inspect a file pinned to another engine (the exit code stays 3)
    -h, --help                 This help

EXIT CODES:
    0 ok    1 a discrepancy (modified file, broken seal, --expect mismatch)
    2 misuse, a missing required frozen file, a refused operation    3 the engine differs from the pinned one
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

/// Runs `phs conf ...` (the whole argument list, `phs` and `conf` included), prints its outcome and exits.
pub fn run_conf(args: &[String]) -> ! {
    let rest = args.get(2..).unwrap_or_default();
    let wants_json = rest.iter().any(|a| a == "--json");
    let outcome = match parse(rest) {
        Err(message) => failure("conf", 2, message),
        Ok(parsed) if parsed.help => Outcome { text: HELP.to_string(), json: Value::Null, code: 0, error: false },
        Ok(parsed) => run(&parsed),
    };
    if wants_json && !outcome.json.is_null() {
        println!("{}", serde_json::to_string_pretty(&outcome.json).unwrap_or_default());
        if outcome.error {
            eprintln!("phs conf: {}", outcome.text);
        }
    } else if outcome.error {
        eprintln!("phs conf: {}", outcome.text);
    } else {
        print!("{}", outcome.text);
    }
    process::exit(outcome.code)
}

/// The flags and the number of positional arguments each command takes.
fn shape(command: &str) -> (&'static [&'static str], usize) {
    match command {
        "status" => (&["--conf", "--json", "--allow-engine-mismatch"], 0),
        "show" => (&["--conf", "--json", "--origin"], 1),
        "history" => (&["--conf", "--json"], 1),
        "freeze" => (&["--json", "--reason", "--engine-policy", "--out"], 0),
        "verify" => (&["--conf", "--json", "--expect", "--allow-engine-mismatch"], 1),
        _ => (&[], 0),
    }
}

fn run(args: &ConfArgs) -> Outcome {
    let command = args.command.as_str();
    if PLANNED.contains(&command) {
        return failure(command, 2, format!("`phs conf {command}` is not available in this build; the commands are: {}", COMMANDS.join(", ")));
    }
    if !COMMANDS.contains(&command) {
        return failure(
            "conf",
            2,
            format!("unknown command {command:?}; the commands are: {} (planned: {})", COMMANDS.join(", "), PLANNED.join(", ")),
        );
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
    if command == "freeze" && args.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
        return failure(command, 2, "`phs conf freeze` needs --reason \"...\" (not empty): the history records why");
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
        "history" => history(args),
        "freeze" => freeze_command(args),
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
        None => "engine: not read\n".to_string(),
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
        Ok(None) => {
            let mut text = "no frozen configuration in force: the embedded catalog".to_string();
            match &local {
                Some(local) => text.push_str(&format!(", with {} read as an overlay of it\n", shown(local))),
                None => text.push_str(" (no physure.conf here)\n"),
            }
            let fields = json!({ "frozen": false, "physure_conf": local.as_deref().map(shown), "warnings": [] });
            return report("status", text, fields, 0);
        }
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

/// The sections a `show` of the catalog groups several kinds under: the kind is shown with the key there.
const POOLED_SECTIONS: [&str; 3] = ["units", "atomic_weights", "settings"];

/// `phs conf show [SELECTOR] [--origin]`: the catalog in force. Unlike the other commands it needs the configuration
/// in force to be usable (decision 2 of the Phase 2 plan), so a broken file is its exit code (1, 2 or 3).
fn show(args: &ConfArgs) -> Outcome {
    if let Err(e) = ensure_valid() {
        return failure("show", e.exit_code(), e.to_string());
    }
    let selector = args.positional.first().map_or("", String::as_str);
    let (listed, warnings, path): (Vec<(Entry, String)>, Vec<String>, Option<PathBuf>) = match active() {
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
            eprintln!("phs conf: warning: {warning}");
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
fn unfrozen_listing() -> Result<(Vec<(Entry, String)>, Vec<String>), String> {
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

    #[test]
    fn parse_refuses_unknown_flags_missing_values_and_repeats() {
        assert!(parse(&args(&["-V"])).unwrap_err().contains("--json"));
        assert!(parse(&args(&["status", "--conf"])).unwrap_err().contains("expects a value"));
        assert!(parse(&args(&["status", "--conf", "--json"])).unwrap_err().contains("expects a value"));
        assert!(parse(&args(&["--out", "a", "--out", "b"])).unwrap_err().contains("twice"));
        assert!(parse(&args(&["--engine-policy", "strict"])).unwrap_err().contains("binary, build, release or record"));
    }
}
