//! Witnesses for `phs conf` (conf freeze Phase 3, spec §8): user witness scripts, the sandbox they run in, and the
//! evaluation of the witnesses under a given frozen file.
//!
//! # What bounds a user witness (correction C3 of the Phase 3 plan)
//!
//! `phs conf verify --run` executes scripts that came inside a file someone sent. Each script runs:
//!
//! * in the sandbox of `physure_script` ([`Limits::sandboxed`]: no files, no plugins, bounded call depth and native
//!   stack, a cap on the output, one thread), with Monte Carlo seed [`DEFAULT_SEED`] (0);
//! * with the loop-iteration and pipeline-step ceilings **fixed** at [`WITNESS_MAX_LOOP_ITERATIONS`] and
//!   [`WITNESS_MAX_PIPELINE_STEPS`] (the compiled defaults), opened on the evaluating thread: the `[Settings]` of the
//!   activated file, which set the process-wide values, cannot raise them;
//! * with a wall-clock deadline ([`DEFAULT_TIMEOUT`], 60 s, unless `--timeout-ms` says otherwise) that the interpreter
//!   honours cooperatively, and a watchdog [`WATCHDOG_GRACE`] after it for an evaluation that does not;
//! * under `catch_unwind`: a panic is that witness's error, never an abort.
//!
//! A limit (the deadline, a ceiling, the call depth, the stack, the output) is an error `limit: ...`, which is not a
//! difference. A thread the watchdog gave up on cannot be killed: every later script is reported
//! [`NOT_RUN`], and the command ends the process after reporting (`run_conf` always exits), which ends that thread.
//! There is no memory cap.
//!
//! # Evaluating under a file
//!
//! [`with_probe`] (freeze, update, seal: the file about to be written, as a sealed throwaway copy in the temporary
//! directory) and [`with_configuration`] (`verify --run`: the file itself) make a frozen file the explicit
//! configuration of this process for the duration of a closure, and always put the previous one back.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use physure_core::frozen::active::{engine_file, explicit_conf};
use physure_core::frozen::sha256::sha256_hex;
use physure_core::frozen::{ensure_valid, reset_cache, run_battery, set_engine_file, set_explicit_conf, FrozenFile, WitnessScript, WitnessValue};
use physure_script::report::{canonical_witness, evaluate_with_ceilings, Report, DEFAULT_SEED};
use physure_script::Limits;

/// The wall-clock time a user witness may take when no `--timeout-ms` is given.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
/// How long after its deadline the watchdog waits for an evaluation that does not honour it.
pub const WATCHDOG_GRACE: Duration = Duration::from_secs(2);
/// The `max_loop_iterations` ceiling of a witness, whatever the file in force sets: the compiled default
/// (`physure_core::settings::max_loop_iterations`).
pub const WITNESS_MAX_LOOP_ITERATIONS: usize = 1_000_000;
/// The `max_pipeline_steps` ceiling of a witness, whatever the file in force sets: the compiled default
/// (`physure_core::settings::max_pipeline_steps`).
pub const WITNESS_MAX_PIPELINE_STEPS: usize = 1_000;
/// The largest witness script `freeze --witness` reads.
pub const MAX_SCRIPT_BYTES: u64 = 1 << 20;
/// The error of every script after one the watchdog gave up on.
pub const NOT_RUN: &str = "not run: an earlier script did not stop";
/// The start of the name of every probe file in the temporary directory.
pub const PROBE_PREFIX: &str = "phs-witness-probe-";

/// The evaluated result of a user witness script.
#[derive(Debug, Clone)]
pub struct WitnessResult {
    pub report: Report,
    /// `report::canonical_witness` of the report (format `phs-witness-1`): printed when the hash differs.
    pub canonical: String,
    /// The SHA-256 (lowercase hex) of `canonical`: the value of `user.<name>`.
    pub sha256: String,
}

impl WitnessResult {
    fn of(report: Report) -> WitnessResult {
        let canonical = canonical_witness(&report);
        let sha256 = sha256_hex(canonical.as_bytes());
        WitnessResult { report, canonical, sha256 }
    }
}

/// The name of the witness a script file gives: its file stem, which must be a name of a frozen file (letters,
/// digits and `_ . : / -`).
pub fn witness_name(path: &Path) -> Result<String, String> {
    let shown = path.to_string_lossy();
    let stem = path
        .file_stem()
        .ok_or_else(|| format!("{shown}: has no file name to name the witness after"))?
        .to_str()
        .ok_or_else(|| format!("{shown}: the file name is not UTF-8, so it cannot name a witness"))?;
    WitnessScript::new(stem, "").map(|_| stem.to_string()).map_err(|_| {
        format!(
            "{shown}: the witness is named after the file stem {stem:?}, which must be letters, digits and _ . : / - only (the rule of names in a frozen file): rename the file"
        )
    })
}

/// The text of a witness script file: UTF-8, at most [`MAX_SCRIPT_BYTES`].
pub fn read_script(path: &Path) -> Result<String, String> {
    let shown = path.to_string_lossy();
    let file = File::open(path).map_err(|e| format!("{shown}: cannot be read: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_SCRIPT_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("{shown}: cannot be read: {e}"))?;
    if bytes.len() as u64 > MAX_SCRIPT_BYTES {
        return Err(format!("{shown}: a witness script is at most {MAX_SCRIPT_BYTES} bytes, this one is larger"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{shown}: is not UTF-8 text ({e})"))
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic without a message".to_string()
    }
}

/// What one guarded evaluation gave.
enum Attempt {
    /// It returned (`Err`: it panicked, with the panic's text).
    Done(Result<Report, String>),
    /// It did not return within the time it was given; its thread is still running.
    Stuck,
}

/// Runs `work` on a thread of its own under `catch_unwind` and waits at most `wait` for it.
fn guarded(work: impl FnOnce() -> Report + Send + 'static, wait: Duration) -> Attempt {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let outcome = catch_unwind(AssertUnwindSafe(work)).map_err(|payload| panic_text(payload.as_ref()));
        let _ = sender.send(outcome);
    });
    match receiver.recv_timeout(wait) {
        Ok(outcome) => Attempt::Done(outcome),
        Err(mpsc::RecvTimeoutError::Timeout) => Attempt::Stuck,
        Err(mpsc::RecvTimeoutError::Disconnected) => Attempt::Done(Err("the evaluation ended without a result".to_string())),
    }
}

/// The result of an attempt, and whether its thread is stuck.
fn interpret(attempt: Attempt, timeout: Duration) -> (Result<WitnessResult, String>, bool) {
    match attempt {
        Attempt::Stuck => (
            Err(format!(
                "limit: the script did not stop {} ms after its deadline of {} ms (the watchdog gave up on it)",
                WATCHDOG_GRACE.as_millis(),
                timeout.as_millis()
            )),
            true,
        ),
        Attempt::Done(Err(panic)) => (Err(format!("panic: {panic}")), false),
        Attempt::Done(Ok(report)) => match report.diagnostics.iter().find(|d| d.code == "limit") {
            Some(limit) => (Err(format!("limit: {}", limit.message)), false),
            None => (Ok(WitnessResult::of(report)), false),
        },
    }
}

/// One user witness, evaluated in the sandbox (see the module documentation) under the configuration in force:
/// `Err` is a limit (`limit: ...`) or a panic (`panic: ...`), never a difference. A finding that is not a limit is
/// part of the result (`report.ok` is false): `freeze` refuses such a script, `verify --run` compares its hash.
#[allow(dead_code)] // the one-script form; the commands evaluate through `evaluate_witnesses`
pub fn evaluate_user_witness(text: &str, timeout: Duration) -> Result<WitnessResult, String> {
    attempt(text, timeout).0
}

fn attempt(text: &str, timeout: Duration) -> (Result<WitnessResult, String>, bool) {
    let source = text.to_string();
    let work = move || {
        evaluate_with_ceilings(&source, Limits::sandboxed(timeout), DEFAULT_SEED, WITNESS_MAX_LOOP_ITERATIONS, WITNESS_MAX_PIPELINE_STEPS)
    };
    interpret(guarded(work, timeout + WATCHDOG_GRACE), timeout)
}

/// Evaluates `scripts` (name, text) in order with `attempt`; after one attempt whose thread is stuck, the rest are
/// [`NOT_RUN`]. The flag says whether one got stuck.
fn run_scripts(
    scripts: &[(String, String)],
    mut attempt: impl FnMut(&str) -> (Result<WitnessResult, String>, bool),
) -> (Vec<(String, Result<WitnessResult, String>)>, bool) {
    let mut stuck = false;
    let mut results = Vec::with_capacity(scripts.len());
    for (name, text) in scripts {
        let result = if stuck {
            Err(NOT_RUN.to_string())
        } else {
            let (result, now_stuck) = attempt(text);
            stuck = now_stuck;
            result
        };
        results.push((name.clone(), result));
    }
    (results, stuck)
}

/// Every witness evaluated under the configuration in force.
#[derive(Debug)]
pub struct WitnessRun {
    /// The rows to record or compare, in the one order (decision S2): the battery in id order (`builtin.<id>`), then
    /// the user witnesses in the order given (`user.<name>`, `Ok(WitnessValue::Hash)` of the canonical result).
    pub rows: Vec<(String, Result<WitnessValue, String>)>,
    /// The user witnesses by name, with their reports and canonical texts.
    pub user: Vec<(String, Result<WitnessResult, String>)>,
    /// A script did not stop and its thread is still running: the process must end after reporting.
    pub stuck: bool,
}

/// What [`unclean`] says of a script that produces no result.
pub const NO_RESULT: &str = "a witness must produce at least one result (this script produces none: an empty, comment-only or definitions-only script witnesses nothing)";

/// Why the result of a user witness cannot be a reference result, if it cannot: it has an error finding, or it has
/// no result item at all (its canonical result would be `phs-witness-1\nok true\n` whatever the configuration, a
/// match forever). `freeze` refuses both, so an honest file never records one.
pub fn unclean(report: &Report) -> Option<String> {
    if !report.ok {
        let first = report.diagnostics.iter().find(|d| d.severity == "error");
        let finding = first.map_or_else(|| "an error finding".to_string(), |d| format!("line {}: {}: {}", d.line, d.code, d.message));
        return Some(format!("the script does not evaluate cleanly: {finding}"));
    }
    if report.results.is_empty() {
        return Some(NO_RESULT.to_string());
    }
    None
}

/// The battery, then `scripts` (their normalised text, `WitnessScript::text`, decision C13) in order, each user
/// witness bounded by `timeout`. Call it inside [`with_probe`] or [`with_configuration`]. The row of a user witness
/// whose result is [`unclean`] is that error, not its hash: `verify --run` reports it even when the hash matches,
/// and `update` and `seal` never re-record it.
pub fn evaluate_witnesses(scripts: &[WitnessScript], timeout: Duration) -> WitnessRun {
    let mut rows = run_battery();
    let named: Vec<(String, String)> = scripts.iter().map(|s| (s.name().to_string(), s.text().to_string())).collect();
    let (user, stuck) = run_scripts(&named, |text| attempt(text, timeout));
    for (name, result) in &user {
        let row = match result {
            Ok(result) => unclean(&result.report).map_or_else(|| Ok(WitnessValue::Hash(result.sha256.clone())), Err),
            Err(e) => Err(e.clone()),
        };
        rows.push((format!("user.{name}"), row));
    }
    WitnessRun { rows, user, stuck }
}

/// Why a file could not be made the configuration to evaluate under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeError {
    /// The exit code it is reported with (`ActiveError::exit_code`, or 1 for a probe that cannot be written).
    pub code: i32,
    pub message: String,
}

/// What a closure run under a configuration returned, and the warnings of putting things back (a probe file that
/// could not be removed).
#[derive(Debug)]
pub struct Evaluated<R> {
    pub value: R,
    pub warnings: Vec<String>,
}

/// The configuration of this process before [`with_configuration`] or [`with_probe`] changed it, put back on every
/// path, also on unwind.
struct Restore {
    previous: Option<PathBuf>,
    previous_engine: Option<PathBuf>,
    /// The probe file to delete.
    temporary: Option<PathBuf>,
    done: bool,
}

impl Restore {
    fn new(temporary: Option<PathBuf>) -> Restore {
        Restore { previous: explicit_conf(), previous_engine: engine_file(), temporary, done: false }
    }

    /// Puts the previous explicit configuration and engine file back, forgets the cache, deletes the probe. The
    /// warnings say what could not be done.
    fn restore(&mut self) -> Vec<String> {
        if self.done {
            return Vec::new();
        }
        self.done = true;
        set_explicit_conf(self.previous.take());
        set_engine_file(self.previous_engine.take());
        reset_cache();
        let mut warnings = Vec::new();
        if let Some(path) = self.temporary.take() {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => warnings.push(format!(
                    "the temporary probe file {} could not be removed ({e}); delete it by hand",
                    path.to_string_lossy()
                )),
            }
        }
        warnings
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        for warning in self.restore() {
            eprintln!("phs conf: warning: {warning}");
        }
    }
}

/// Runs `f` with the frozen file at `path` as the explicit configuration of this process (a relative path is made
/// absolute first; the file is used in place, never copied), the engine file being this executable; the file must
/// load (`ensure_valid`), otherwise `f` is not run. The previous explicit configuration and engine file are put back
/// and the cache forgotten on every path.
pub fn with_configuration<R>(path: &Path, f: impl FnOnce() -> R) -> Result<Evaluated<R>, ProbeError> {
    let absolute = std::path::absolute(path).map_err(|e| ProbeError {
        code: 1,
        message: format!("{}: cannot be made an absolute path ({e})", path.to_string_lossy()),
    })?;
    run_under(Restore::new(None), &absolute, f)
}

fn run_under<R>(mut restore: Restore, path: &Path, f: impl FnOnce() -> R) -> Result<Evaluated<R>, ProbeError> {
    set_explicit_conf(Some(path.to_path_buf()));
    if let Ok(exe) = std::env::current_exe() {
        set_engine_file(Some(exe));
    }
    reset_cache();
    if let Err(e) = ensure_valid() {
        let warnings = restore.restore();
        let mut message = e.to_string();
        for warning in warnings {
            message.push_str(&format!("; warning: {warning}"));
        }
        return Err(ProbeError { code: e.exit_code(), message });
    }
    let value = f();
    let warnings = restore.restore();
    Ok(Evaluated { value, warnings })
}

/// A name for a probe file nobody else picks: the process id and a random suffix.
fn probe_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    format!("{PROBE_PREFIX}{}-{:016x}.frozen.conf", std::process::id(), hasher.finish())
}

/// How old a probe file left in the temporary directory must be before [`remove_stale_probes`] deletes it: no
/// command runs that long, so it belongs to a process that ended without deleting it (one killed, or aborted).
pub const STALE_PROBE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Deletes the probe files in `dir` (names starting with [`PROBE_PREFIX`] and ending in `.frozen.conf`) last modified
/// more than [`STALE_PROBE_AGE`] before `now`. Nothing else is touched, and every error is ignored: this is tidying up
/// after a process that could not, never a step a command depends on.
pub fn remove_stale_probes(dir: &Path, now: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(PROBE_PREFIX) || !name.ends_with(".frozen.conf") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        let old = metadata.modified().ok().and_then(|modified| now.duration_since(modified).ok()).is_some_and(|age| age > STALE_PROBE_AGE);
        if metadata.is_file() && old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Runs `f` with `probe` (a draft's sealed throwaway copy, `Draft::probe` / `FreezeDraft::probe`) as the explicit
/// configuration: it is written to a new file in `std::env::temp_dir()` (`create_new`, a random name starting with
/// [`PROBE_PREFIX`]), loaded as [`with_configuration`] loads a file, and deleted on every path. Probe files more than a
/// day old, left by a process that ended without deleting its own, are deleted first ([`remove_stale_probes`]).
pub fn with_probe<R>(probe: &FrozenFile, f: impl FnOnce() -> R) -> Result<Evaluated<R>, ProbeError> {
    let text = probe.render().map_err(|e| ProbeError { code: 1, message: format!("the probe file does not render: {e}") })?;
    let dir = std::env::temp_dir();
    remove_stale_probes(&dir, std::time::SystemTime::now());
    let mut tries = 0;
    let (path, mut file) = loop {
        let path = dir.join(probe_name());
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && tries < 8 => tries += 1,
            Err(e) => {
                return Err(ProbeError {
                    code: 1,
                    message: format!("a probe file cannot be created in {} ({e}); nothing evaluated", dir.to_string_lossy()),
                })
            }
        }
    };
    let restore = Restore::new(Some(path.clone()));
    if let Err(e) = file.write_all(text.as_bytes()).and_then(|()| file.sync_all()) {
        drop(file);
        drop(restore);
        return Err(ProbeError { code: 1, message: format!("the probe file {} cannot be written ({e}); nothing evaluated", path.to_string_lossy()) });
    }
    drop(file);
    run_under(restore, &path, f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use physure_core::frozen::{
        engine_pairs, now_unix, prepare_freeze, EngineIdentity, EnginePolicy, EntryInput, FreezeOptions, FrozenParts, Source,
    };
    use physure_core::units::conf::DEFAULT_PHYSURE_CONF;

    /// The variable that tells a child test process which test it runs alone.
    const ISOLATED: &str = "PHYSURE_CLI_ISOLATED_TEST";

    /// Whether the test `name` (of this module) is to run its body here. A test that changes the configuration of
    /// the whole process (the explicit configuration, the engine file) cannot share a process with the other tests
    /// of this binary, which read the catalog in parallel (they saw the probe as the configuration in force, or its
    /// path after it was deleted): it runs again in a child process of this test binary that runs only that test.
    /// In the parent this asserts that the child passed and returns `false`; in the child it returns `true`.
    fn in_own_process(name: &str) -> bool {
        if std::env::var(ISOLATED).as_deref() == Ok(name) {
            return true;
        }
        let filter = format!("conf_witness::tests::{name}");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([filter.as_str(), "--exact", "--test-threads=1", "--nocapture"])
            .env(ISOLATED, name)
            .env_remove("PHYSURE_CONF")
            .env_remove("PHYSURE_REQUIRE_FROZEN")
            .output()
            .unwrap();
        let (out, err) = (String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "{name} in its own process:\n{out}{err}");
        assert!(out.contains("1 passed"), "{name} did not run in its own process:\n{out}{err}");
        false
    }

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Dir {
            let path = std::env::temp_dir().join(format!("physure_cli_witness_{}_{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Dir(path)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The probe of a freeze of `dir` (with its `physure.conf`, if any).
    fn probe_of(dir: &Dir) -> FrozenFile {
        let options = FreezeOptions {
            cwd: &dir.0,
            out: None,
            reason: "probe test",
            policy: EnginePolicy::Release,
            engine_file: None,
            time_unix: now_unix(),
            actor: "t@h",
        };
        prepare_freeze(&options).unwrap().probe().clone()
    }

    /// The probe files of this process left in the temporary directory.
    fn probes_left() -> Vec<String> {
        let mine = format!("{PROBE_PREFIX}{}-", std::process::id());
        std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.starts_with(&mine))
            .collect()
    }

    #[test]
    fn a_witness_is_named_after_its_file_stem_in_the_name_alphabet() {
        assert_eq!(witness_name(Path::new("dir/validation-1.phs")).unwrap(), "validation-1");
        assert_eq!(witness_name(Path::new("a.b_c.phs")).unwrap(), "a.b_c");
        for bad in ["my witness.phs", "café.phs", "a+b.phs"] {
            let e = witness_name(Path::new(bad)).unwrap_err();
            assert!(e.contains("letters, digits and _ . : / -") && e.contains("file stem"), "{bad}: {e}");
        }
    }

    #[test]
    fn a_script_is_read_as_utf8_up_to_one_mebibyte() {
        let dir = Dir::new("read_script");
        let ok = dir.0.join("ok.phs");
        std::fs::write(&ok, "x = 1 m\r\n").unwrap();
        assert_eq!(read_script(&ok).unwrap(), "x = 1 m\r\n");
        let large = dir.0.join("large.phs");
        std::fs::write(&large, vec![b'1'; MAX_SCRIPT_BYTES as usize + 1]).unwrap();
        assert!(read_script(&large).unwrap_err().contains("at most"));
        let binary = dir.0.join("binary.phs");
        std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert!(read_script(&binary).unwrap_err().contains("UTF-8"));
        assert!(read_script(&dir.0.join("missing.phs")).unwrap_err().contains("cannot be read"));
    }

    #[test]
    fn a_clean_script_gives_its_canonical_result_and_its_hash() {
        let result = evaluate_user_witness("x = 2 m\ny = x * 3", Duration::from_secs(30)).unwrap();
        assert!(result.report.ok, "{:?}", result.report.diagnostics);
        assert!(result.canonical.starts_with("phs-witness-1\nok true\n"), "{}", result.canonical);
        assert_eq!(result.sha256, sha256_hex(result.canonical.as_bytes()));
        let again = evaluate_user_witness("x = 2 m\ny = x * 3", Duration::from_secs(30)).unwrap();
        assert_eq!(again.sha256, result.sha256, "deterministic");
        // a finding that is not a limit is part of the result
        let wrong = evaluate_user_witness("1 m + 1 s", Duration::from_secs(30)).unwrap();
        assert!(!wrong.report.ok && wrong.canonical.contains("ok false"), "{}", wrong.canonical);
    }

    #[test]
    fn a_script_past_its_deadline_is_a_limit_error() {
        let forever = "i = 0\nwhile i < 1 {\nj = 0\nwhile j < 1 {\nk = 0\nwhile k < 9999 {\nk = k + 1\n}\nj = j * 1\n}\ni = i * 1\n}";
        let started = std::time::Instant::now();
        let e = evaluate_user_witness(forever, Duration::from_millis(200)).unwrap_err();
        assert!(e.starts_with("limit: "), "{e}");
        assert!(started.elapsed() < Duration::from_secs(30), "{:?}", started.elapsed());
    }

    #[test]
    fn a_loop_past_the_fixed_ceiling_is_a_limit_error() {
        let e = evaluate_user_witness("x = for i in 0..2000000 { i }", Duration::from_secs(30)).unwrap_err();
        assert!(e.starts_with("limit: ") && e.contains("ceiling of 1000000"), "{e}");
    }

    #[test]
    fn a_panic_is_that_witness_s_error() {
        let (result, stuck) = interpret(guarded(|| panic!("boom in a builtin"), Duration::from_secs(10)), DEFAULT_TIMEOUT);
        assert!(!stuck);
        assert_eq!(result.unwrap_err(), "panic: boom in a builtin");
    }

    #[test]
    fn after_a_stuck_script_the_rest_are_not_run() {
        let started = std::time::Instant::now();
        let attempt = guarded(
            || {
                std::thread::sleep(Duration::from_secs(60));
                unreachable!("the watchdog answers first")
            },
            Duration::from_millis(100),
        );
        let (result, stuck) = interpret(attempt, Duration::from_millis(50));
        assert!(stuck && result.unwrap_err().starts_with("limit: "));
        assert!(started.elapsed() < Duration::from_secs(10));
        // the sequence: the stuck one is a limit, the ones after it are not run, the ones before it ran
        let scripts: Vec<(String, String)> =
            ["first", "stuck", "third", "fourth"].iter().map(|n| (n.to_string(), n.to_string())).collect();
        let mut ran = Vec::new();
        let (results, stuck) = run_scripts(&scripts, |text| {
            ran.push(text.to_string());
            if text == "stuck" {
                (Err("limit: did not stop".to_string()), true)
            } else {
                (Ok(WitnessResult::of(Report { ok: true, results: Vec::new(), diagnostics: Vec::new() })), false)
            }
        });
        assert!(stuck);
        assert_eq!(ran, ["first", "stuck"]);
        assert!(results[0].1.is_ok());
        assert_eq!(results[1].1.as_ref().unwrap_err(), "limit: did not stop");
        for (_, result) in &results[2..] {
            assert_eq!(result.as_ref().unwrap_err(), NOT_RUN);
        }
    }

    #[test]
    fn the_probe_is_in_force_while_f_runs_and_never_remains() {
        if !in_own_process("the_probe_is_in_force_while_f_runs_and_never_remains") {
            return;
        }
        let dir = Dir::new("probe_success");
        std::fs::write(dir.0.join("physure.conf"), "[Constants]\nzz_probe_constant = 3 m\n").unwrap();
        let probe = probe_of(&dir);
        let before = explicit_conf();
        let seen = with_probe(&probe, || {
            let path = explicit_conf().expect("an explicit configuration");
            assert!(path.starts_with(std::env::temp_dir()) && path.is_file(), "{path:?}");
            assert_eq!(probes_left().len(), 1);
            let active = physure_core::frozen::active().unwrap().expect("the probe is in force");
            (path, active.catalog.file.sources()[1].text().to_string())
        })
        .unwrap();
        assert!(seen.warnings.is_empty(), "{:?}", seen.warnings);
        let (path, overrides) = seen.value;
        assert_eq!(overrides, "[Constants]\nzz_probe_constant = 3 m");
        assert!(!path.exists(), "the probe was deleted");
        assert!(probes_left().is_empty(), "{:?}", probes_left());
        assert_eq!(explicit_conf(), before, "the previous explicit configuration is back");
    }

    #[test]
    fn a_probe_that_does_not_load_is_not_evaluated_and_never_remains() {
        if !in_own_process("a_probe_that_does_not_load_is_not_evaluated_and_never_remains") {
            return;
        }
        let identity = EngineIdentity { version: "0.0.1".into(), ..EngineIdentity::current() };
        let other = FrozenFile::new(
            "2026-11-08T10:00:00Z",
            FrozenParts {
                engine: engine_pairs(&identity, EnginePolicy::Release, None).unwrap(),
                sources: vec![Source::new("embedded", DEFAULT_PHYSURE_CONF).unwrap(), Source::new("overrides", "").unwrap()],
                ..FrozenParts::default()
            },
            EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
        )
        .unwrap();
        let before = explicit_conf();
        let mut ran = false;
        let e = with_probe(&other, || ran = true).unwrap_err();
        assert!(!ran, "nothing is evaluated under a file that does not load");
        assert_eq!(e.code, 3, "{e:?}");
        assert!(probes_left().is_empty(), "{:?}", probes_left());
        assert_eq!(explicit_conf(), before);
    }

    #[test]
    fn a_panic_under_the_probe_restores_the_previous_configuration_and_deletes_the_probe() {
        if !in_own_process("a_panic_under_the_probe_restores_the_previous_configuration_and_deletes_the_probe") {
            return;
        }
        let dir = Dir::new("probe_panic");
        let probe = probe_of(&dir);
        // a previous explicit configuration that is not "none": a real frozen file elsewhere
        let previous = dir.0.join("previous.frozen.conf");
        std::fs::write(&previous, probe.render().unwrap()).unwrap();
        set_explicit_conf(Some(previous.clone()));
        let engine_before = engine_file();
        let unwound = catch_unwind(AssertUnwindSafe(|| with_probe(&probe, || panic!("inside the probe"))));
        let restored = explicit_conf();
        set_explicit_conf(None);
        assert!(unwound.is_err());
        assert_eq!(restored, Some(previous), "the previous explicit configuration is back");
        assert_eq!(engine_file(), engine_before);
        assert!(probes_left().is_empty(), "{:?}", probes_left());
    }

    #[test]
    fn stale_probe_files_are_deleted_and_nothing_else_is() {
        let dir = Dir::new("stale_probes");
        let now = std::time::SystemTime::now();
        let aged = |name: &str, age: Duration| {
            let path = dir.0.join(name);
            std::fs::write(&path, "x").unwrap();
            File::options().write(true).open(&path).unwrap().set_modified(now - age).unwrap();
            path
        };
        let day = STALE_PROBE_AGE;
        let stale = aged(&format!("{PROBE_PREFIX}1-00000000000000aa.frozen.conf"), day + Duration::from_secs(3600));
        let fresh = aged(&format!("{PROBE_PREFIX}2-00000000000000bb.frozen.conf"), Duration::from_secs(60));
        let almost = aged(&format!("{PROBE_PREFIX}3-00000000000000cc.frozen.conf"), day - Duration::from_secs(3600));
        let other_suffix = aged(&format!("{PROBE_PREFIX}4.txt"), day * 3);
        let other_prefix = aged("physure.frozen.conf", day * 3);
        let folder = dir.0.join(format!("{PROBE_PREFIX}5.frozen.conf"));
        std::fs::create_dir(&folder).unwrap();
        remove_stale_probes(&dir.0, now);
        assert!(!stale.exists(), "a probe more than a day old is deleted");
        for kept in [&fresh, &almost, &other_suffix, &other_prefix, &folder] {
            assert!(kept.exists(), "{kept:?} is kept");
        }
        // a directory that cannot be read is no error
        remove_stale_probes(&dir.0.join("missing"), now);
    }

    #[test]
    fn with_configuration_uses_the_file_in_place() {
        if !in_own_process("with_configuration_uses_the_file_in_place") {
            return;
        }
        let dir = Dir::new("configuration_in_place");
        let probe = probe_of(&dir);
        let file = dir.0.join("in-place.frozen.conf");
        std::fs::write(&file, probe.render().unwrap()).unwrap();
        let before = explicit_conf();
        let seen = with_configuration(&file, || explicit_conf()).unwrap();
        assert_eq!(seen.value, Some(std::path::absolute(&file).unwrap()));
        assert!(probes_left().is_empty());
        assert_eq!(explicit_conf(), before);
        assert!(file.is_file(), "the file itself is kept");
    }
}
