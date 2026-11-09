//! The identity of the running engine and the policies that pin it (spec §6, §7).
//!
//! An identity can be wrong in only one direction here: it can fail to prove that two builds are the same. It
//! never claims they are when it cannot tell, so every doubt is a mismatch. **Trust boundary:** whoever controls
//! the build environment (the variable `PHYSURE_BUILD_COMMIT`, `PATH`, `RUSTFLAGS`, the toolchain) controls every
//! field; `build` guards against accidents, not against a malicious builder. Not visible to a build script and
//! therefore not pinned: `overflow-checks`, `lto`, `codegen-units`, the versions of dependencies when `physure`
//! is built inside someone else's workspace, and every dynamically linked library.
//!
//! **The `commit` is a claim, not a proof.** The release pipeline sets `PHYSURE_BUILD_COMMIT` to the commit it checked
//! out and the build records it; without the variable the commit is `unknown`, which never equals anything. Nothing
//! asks git: a build cannot prove that the files it compiles are those of a commit. It also covers **physure-core**
//! at most, while the code that evaluates includes `physure-script`, `physure-cli` and the bindings. `build` therefore
//! pins what the builder says about the core's source revision, not the whole evaluating engine; only `binary` pins
//! the artefact.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use super::canonical::is_sha256_hex;
use super::model::{FrozenError, FrozenResult, Pairs};
use super::sha256::{to_hex, Sha256};

// The build script's commit rule, compiled into the library too so that both use one `is_commit` and one
// `UNKNOWN` (and so that the rule's unit tests run with the library's).
#[allow(dead_code)]
#[path = "../../build_support/commit.rs"]
mod commit_rules;
use commit_rules::is_commit;

/// What an identity says when it cannot name a commit.
pub const UNKNOWN: &str = commit_rules::UNKNOWN;

/// The named features of this crate and whether each is enabled. `default` is a bundle and is not listed. Keep in
/// step with `[features]` of `Cargo.toml` (a test checks it).
const FEATURES: [(&str, bool); 1] = [("html-viewer", cfg!(feature = "html-viewer"))];

/// Which build of the engine this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIdentity {
    pub version: String,
    /// 40 or 64 lowercase hexadecimal characters, or `unknown`.
    pub commit: String,
    pub profile: String,
    pub target: String,
    /// Sorted, comma-separated named features, without `default`.
    pub features: String,
    /// The first line of `rustc -V` of the compiler that built it.
    pub rustc: String,
    /// `opt-level=…;debug-assertions=…;panic=…;rustflags=<sha256>;target-features=<sha256>`.
    pub flags: String,
}

fn is_word(text: &str, extra: &[char]) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

/// One non-empty line without padding: no control character at all (tab, NUL, escape, the C1 controls, CR, LF) and
/// no Unicode line or paragraph separator, so the value reads and compares the same everywhere it is shown.
fn one_line(what: &str, text: &str) -> FrozenResult<()> {
    let breaks = |c: char| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}');
    if text.is_empty() || text != text.trim() || text.chars().any(breaks) {
        return Err(FrozenError(format!(
            "the engine's {what} must be one non-empty line without padding or control characters: {text:?}"
        )));
    }
    Ok(())
}

impl EngineIdentity {
    /// The identity of the engine that is running. All values come from the build script, which always sets them.
    pub fn current() -> EngineIdentity {
        let features: Vec<&str> = FEATURES.iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect();
        EngineIdentity {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: env!("PHYSURE_ENGINE_COMMIT").to_string(),
            profile: env!("PHYSURE_ENGINE_PROFILE").to_string(),
            target: env!("PHYSURE_ENGINE_TARGET").to_string(),
            features: features.join(","),
            rustc: env!("PHYSURE_ENGINE_RUSTC").to_string(),
            flags: env!("PHYSURE_ENGINE_FLAGS").to_string(),
        }
    }

    /// Refuses a value that could make two different builds look alike or that cannot be stored on one line.
    pub fn validate(&self) -> FrozenResult<()> {
        let bad = |what: &str, value: &str| -> FrozenResult<()> { Err(FrozenError(format!("invalid engine {what}: {value:?}"))) };
        if !is_word(&self.version, &['.', '+', '-']) {
            return bad("version", &self.version);
        }
        if self.commit != UNKNOWN && !is_commit(&self.commit) {
            return bad("commit (40 or 64 lowercase hexadecimal characters, or unknown)", &self.commit);
        }
        if !self.profile.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') || self.profile.is_empty() {
            return bad("profile", &self.profile);
        }
        if !is_word(&self.target, &['_', '.', '-']) {
            return bad("target", &self.target);
        }
        if !self.features.is_empty() {
            let names: Vec<&str> = self.features.split(',').collect();
            let well_formed = names.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            let sorted_unique = names.windows(2).all(|w| w[0] < w[1]);
            if !well_formed || !sorted_unique {
                return bad("features (sorted, unique, lowercase names)", &self.features);
            }
        }
        one_line("rustc", &self.rustc)?;
        one_line("flags", &self.flags)
    }

    /// `version+commit` (twelve characters of the commit, or `unknown`), the form the history records.
    pub fn short(&self) -> String {
        // `get` and not slicing: a hand-built identity may hold anything, and this must not panic
        let commit = self.commit.get(..12).unwrap_or(&self.commit);
        format!("{}+{}", self.version, commit)
    }
}

/// How strictly a frozen file pins the engine (spec §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnginePolicy {
    /// The same executable file (SHA-256) — the only policy whose evidence is the artefact itself.
    Binary,
    /// The same version, commit, profile, target, features, compiler and build flags.
    #[default]
    Build,
    /// The same version string. (Development builds between releases keep the last release's number, so this
    /// accepts unreleased code.)
    Release,
    /// Nothing is enforced; the identity is only recorded.
    Record,
}

impl EnginePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            EnginePolicy::Binary => "binary",
            EnginePolicy::Build => "build",
            EnginePolicy::Release => "release",
            EnginePolicy::Record => "record",
        }
    }
}

impl FromStr for EnginePolicy {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "binary" => Ok(EnginePolicy::Binary),
            "build" => Ok(EnginePolicy::Build),
            "release" => Ok(EnginePolicy::Release),
            "record" => Ok(EnginePolicy::Record),
            other => Err(format!("unknown engine policy {other:?}: use binary, build, release or record")),
        }
    }
}

/// One field in which the recorded and the running engines differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub field: &'static str,
    pub recorded: String,
    pub running: String,
}

/// Why the running engine is not the one a frozen file pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineMismatch {
    pub policy: EnginePolicy,
    pub differences: Vec<Difference>,
}

impl std::fmt::Display for EngineMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "this is not the engine the configuration was validated with (policy {}):", self.policy.as_str())?;
        for d in &self.differences {
            write!(f, " {}: recorded {}, running {};", d.field, d.recorded, d.running)?;
        }
        Ok(())
    }
}

impl std::error::Error for EngineMismatch {}

fn same(differences: &mut Vec<Difference>, field: &'static str, recorded: &str, running: &str) {
    if recorded != running {
        differences.push(Difference { field, recorded: recorded.to_string(), running: running.to_string() });
    }
}

/// Compares the engine a frozen file pins with the one that is running, under `policy`.
///
/// `recorded_binary` is the `binary_sha256` of the file, if it has one. `running_binary` must compute the hash of
/// **the file that contains this engine** (for a Python extension module that is the module, not the
/// interpreter) and is called **only** under the policy `binary` and only when a hash was recorded. Every doubt is
/// a difference: an unknown commit equals nothing, a missing recorded hash under `binary` is a difference, an
/// unreadable file is a difference. Under `build` a recorded hash is informational and ignored. `binary` attests
/// the file at its path at check time, not the image in memory.
pub fn check(
    recorded: &EngineIdentity,
    recorded_binary: Option<&str>,
    policy: EnginePolicy,
    running: &EngineIdentity,
    running_binary: impl FnOnce() -> std::io::Result<String>,
) -> Result<(), EngineMismatch> {
    let mut differences = Vec::new();
    match policy {
        EnginePolicy::Record => {}
        EnginePolicy::Release => same(&mut differences, "version", &recorded.version, &running.version),
        EnginePolicy::Build => {
            same(&mut differences, "version", &recorded.version, &running.version);
            if recorded.commit == UNKNOWN || running.commit == UNKNOWN || recorded.commit != running.commit {
                let shown = |c: &str| if c == UNKNOWN { "unknown (this build cannot be told apart from another)".to_string() } else { c.to_string() };
                differences.push(Difference { field: "commit", recorded: shown(&recorded.commit), running: shown(&running.commit) });
            }
            same(&mut differences, "profile", &recorded.profile, &running.profile);
            same(&mut differences, "target", &recorded.target, &running.target);
            same(&mut differences, "features", &recorded.features, &running.features);
            same(&mut differences, "rustc", &recorded.rustc, &running.rustc);
            same(&mut differences, "flags", &recorded.flags, &running.flags);
        }
        EnginePolicy::Binary => match recorded_binary {
            None => differences.push(Difference {
                field: "binary_sha256",
                recorded: "(not recorded)".to_string(),
                running: "(the policy binary requires it)".to_string(),
            }),
            Some(recorded_hash) => match running_binary() {
                Ok(running_hash) => same(&mut differences, "binary_sha256", recorded_hash, &running_hash),
                Err(e) => differences.push(Difference {
                    field: "binary_sha256",
                    recorded: recorded_hash.to_string(),
                    running: format!("(the file could not be read: {e})"),
                }),
            },
        },
    }
    if differences.is_empty() {
        Ok(())
    } else {
        Err(EngineMismatch { policy, differences })
    }
}

/// Whether a frozen file may be made with this identity under `policy`: `build` and `binary` name a source
/// revision (a binary hash alone does not tell an auditor which source it came from), so they need a known
/// commit. `release` and `record` do not.
pub fn freezable(identity: &EngineIdentity, policy: EnginePolicy) -> Result<(), String> {
    if matches!(policy, EnginePolicy::Build | EnginePolicy::Binary) && identity.commit == UNKNOWN {
        return Err(format!(
            "this build does not name its commit (build it with PHYSURE_BUILD_COMMIT set to the full lowercase hash), so the policy {} cannot be recorded; use release or record",
            policy.as_str()
        ));
    }
    Ok(())
}

/// A decoded `[Engine]` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineBlock {
    pub identity: EngineIdentity,
    pub policy: EnginePolicy,
    /// The SHA-256 of the file that contains the engine, if recorded. Required by `binary`, informational under
    /// `build`.
    pub binary_sha256: Option<String>,
}

impl EngineBlock {
    /// [`check`] of the running engine against this block, under its policy and with its recorded hash.
    pub fn check(&self, running: &EngineIdentity, running_binary: impl FnOnce() -> std::io::Result<String>) -> Result<(), EngineMismatch> {
        check(&self.identity, self.binary_sha256.as_deref(), self.policy, running, running_binary)
    }
}

/// Whether a block with these values could have been recorded: the identity is valid, the policy is
/// [`freezable`] for it, `binary` has a hash, and a hash is a SHA-256. [`engine_pairs`] refuses to build such a
/// block and `FrozenFile::verify` reports one written by hand, so a file can never pin what it cannot check.
pub(crate) fn recordable(identity: &EngineIdentity, policy: EnginePolicy, binary_sha256: Option<&str>) -> FrozenResult<()> {
    identity.validate()?;
    freezable(identity, policy).map_err(FrozenError)?;
    match (policy, binary_sha256) {
        (EnginePolicy::Binary, None) => Err(FrozenError("the policy binary needs the binary_sha256 of the engine's file".into())),
        (_, Some(hash)) if !is_sha256_hex(hash) => {
            Err(FrozenError(format!("binary_sha256 is not a SHA-256 in lowercase hexadecimal: {hash:?}")))
        }
        _ => Ok(()),
    }
}

/// The `[Engine]` block for `identity` and `policy`, with the hash of the engine's file if given. Refuses an
/// invalid identity, a policy that cannot be recorded for it (`freezable`) and `binary` without a hash.
pub fn engine_pairs(identity: &EngineIdentity, policy: EnginePolicy, binary_sha256: Option<&str>) -> FrozenResult<Pairs> {
    recordable(identity, policy, binary_sha256)?;
    let mut items = vec![
        ("version".to_string(), identity.version.clone()),
        ("commit".to_string(), identity.commit.clone()),
        ("profile".to_string(), identity.profile.clone()),
        ("target".to_string(), identity.target.clone()),
        ("features".to_string(), identity.features.clone()),
        ("rustc".to_string(), identity.rustc.clone()),
        ("flags".to_string(), identity.flags.clone()),
        ("policy".to_string(), policy.as_str().to_string()),
    ];
    if let Some(hash) = binary_sha256 {
        items.push(("binary_sha256".to_string(), hash.to_string()));
    }
    Pairs::from_vec(items)
}

/// Reads an `[Engine]` block back. Strict: the eight keys are required, `binary_sha256` is optional and must be a
/// SHA-256, every value is validated, and **any other key is an error**: an unknown key in the block that sets the
/// policy would be a knob nobody can see.
pub fn read_engine(pairs: &Pairs) -> FrozenResult<EngineBlock> {
    const KNOWN: [&str; 9] = ["version", "commit", "profile", "target", "features", "rustc", "flags", "policy", "binary_sha256"];
    if let Some((key, _)) = pairs.iter().find(|(k, _)| !KNOWN.contains(k)) {
        return Err(FrozenError(format!("[Engine] has an unknown key {key}; the keys are {}", KNOWN.join(", "))));
    }
    let need = |key: &str| -> FrozenResult<String> {
        pairs.get(key).map(str::to_string).ok_or_else(|| FrozenError(format!("[Engine] has no {key}")))
    };
    let identity = EngineIdentity {
        version: need("version")?,
        commit: need("commit")?,
        profile: need("profile")?,
        target: need("target")?,
        features: need("features")?,
        rustc: need("rustc")?,
        flags: need("flags")?,
    };
    identity.validate().map_err(|e| FrozenError(format!("[Engine] {e}")))?;
    let policy: EnginePolicy = need("policy")?.parse().map_err(|e: String| FrozenError(format!("[Engine] {e}")))?;
    let binary_sha256 = match pairs.get("binary_sha256") {
        None => None,
        Some(hash) if is_sha256_hex(hash) => Some(hash.to_string()),
        Some(hash) => return Err(FrozenError(format!("[Engine] binary_sha256 is not a SHA-256 in lowercase hexadecimal: {hash:?}"))),
    };
    Ok(EngineBlock { identity, policy, binary_sha256 })
}

/// What can be said about the machine, for the `[Environment]` block. Informational: outside the seal, and no
/// behaviour may ever depend on it.
pub fn environment_pairs() -> Pairs {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    Pairs::from_vec(vec![
        ("os".to_string(), std::env::consts::OS.to_string()),
        ("arch".to_string(), std::env::consts::ARCH.to_string()),
        ("family".to_string(), std::env::consts::FAMILY.to_string()),
        ("endian".to_string(), if cfg!(target_endian = "little") { "little" } else { "big" }.to_string()),
        ("pointer_width".to_string(), usize::BITS.to_string()),
        ("cpus".to_string(), cpus.to_string()),
    ])
    .expect("the environment record is valid by construction")
}

/// The SHA-256 of a file, read in blocks. Hash **the file that contains the engine**: for the CLI that is
/// `std::env::current_exe()`, for a Python extension module it is the module (not the interpreter), for the Java
/// library the library. The core deliberately has no "hash my own executable" convenience, because in a binding
/// that would pin the interpreter and let a swapped module pass.
pub fn sha256_of_file(path: &Path) -> std::io::Result<String> {
    sha256_of_reader(File::open(path)?)
}

/// The SHA-256 of everything `reader` yields, read in blocks.
pub(crate) fn sha256_of_reader(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut block = [0u8; 65_536];
    loop {
        let read = reader.read(&mut block)?;
        if read == 0 {
            break;
        }
        hasher.update(&block[..read]);
    }
    Ok(to_hex(&hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

    fn identity(commit: &str) -> EngineIdentity {
        EngineIdentity {
            version: "0.3.1".into(),
            commit: commit.into(),
            profile: "release".into(),
            target: "x86_64-pc-windows-msvc".into(),
            features: "html-viewer".into(),
            rustc: "rustc 1.97.1 (8bab26f4f 2026-07-14)".into(),
            flags: "opt-level=3;debug-assertions=off;panic=unwind;rustflags=00;target-features=00".into(),
        }
    }

    #[test]
    fn the_unknown_constant_is_the_one_of_the_build_rule() {
        assert_eq!(UNKNOWN, commit_rules::UNKNOWN);
    }

    #[test]
    fn the_running_identity_is_valid_and_its_commit_is_what_the_variable_says() {
        let now = EngineIdentity::current();
        now.validate().unwrap();
        assert_eq!(now.version, env!("CARGO_PKG_VERSION"));
        assert!(["debug", "release"].contains(&now.profile.as_str()), "{}", now.profile);
        assert!(now.target.contains('-'), "a target triple: {}", now.target);
        assert!(now.rustc.starts_with("rustc "), "{}", now.rustc);
        assert!(now.flags.starts_with("opt-level="), "{}", now.flags);
        // the commit is what the build's rule gives for the variable as it is now (the script reruns when it changes)
        let variable = std::env::var("PHYSURE_BUILD_COMMIT").ok();
        assert_eq!(now.commit, commit_rules::decide(variable.as_deref()).commit);
    }

    #[test]
    fn features_are_the_named_ones_without_default_and_match_cargo_toml() {
        let manifest = include_str!("../../Cargo.toml");
        let section = manifest.split("\n[features]").nth(1).expect("[features]").split("\n[").next().unwrap();
        let mut declared: Vec<&str> = section
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
            .filter(|k| !k.is_empty() && !k.starts_with('#') && *k != "default")
            .collect();
        declared.sort_unstable();
        let mut known: Vec<&str> = FEATURES.iter().map(|(name, _)| *name).collect();
        known.sort_unstable();
        assert_eq!(known, declared, "a feature was added to Cargo.toml: add it to FEATURES in engine.rs");
        assert!(!EngineIdentity::current().features.split(',').any(|f| f == "default"));
    }

    #[test]
    fn the_short_form_uses_twelve_characters_of_the_commit() {
        assert_eq!(identity(HEAD).short(), "0.3.1+0123456789ab");
        assert_eq!(identity(UNKNOWN).short(), "0.3.1+unknown");
    }

    #[test]
    fn validation_refuses_what_could_make_two_builds_look_alike() {
        identity(HEAD).validate().unwrap();
        identity(UNKNOWN).validate().unwrap();
        let bad_commits = ["", "Unknown", "UNKNOWN", " unknown", "main", "HEAD", "release", "0123456", "ABCDEF0123456789ABCDEF0123456789ABCDEF01"];
        for commit in bad_commits {
            assert!(identity(commit).validate().is_err(), "commit {commit:?}");
        }
        let edits: [(&str, fn(&mut EngineIdentity)); 12] = [
            ("empty version", |i| i.version = String::new()),
            ("version with space", |i| i.version = "0.3 1".into()),
            ("profile upper case", |i| i.profile = "Release".into()),
            ("empty target", |i| i.target = String::new()),
            ("target with space", |i| i.target = "a b".into()),
            ("features unsorted", |i| i.features = "b,a".into()),
            ("features duplicated", |i| i.features = "a,a".into()),
            ("features with empty name", |i| i.features = "a,,b".into()),
            ("features upper case", |i| i.features = "Html".into()),
            ("two lines in rustc", |i| i.rustc = "a\nb".into()),
            ("padded flags", |i| i.flags = " x".into()),
            ("empty rustc", |i| i.rustc = String::new()),
        ];
        for (what, edit) in edits {
            let mut i = identity(HEAD);
            edit(&mut i);
            assert!(i.validate().is_err(), "{what}");
        }
        let mut empty_features = identity(HEAD);
        empty_features.features = String::new();
        empty_features.validate().unwrap();
    }

    #[test]
    fn rustc_and_flags_refuse_every_control_character_and_the_unicode_line_separators() {
        for bad in ["\t", "\u{0}", "\u{1b}", "\u{7f}", "\u{85}", "\u{2028}", "\u{2029}", "\u{b}", "\u{c}"] {
            let mut i = identity(HEAD);
            i.rustc = format!("rustc{bad}1.97.1");
            assert!(i.validate().is_err(), "rustc with {bad:?}");
            let mut i = identity(HEAD);
            i.flags = format!("opt-level=3{bad};x");
            assert!(i.validate().is_err(), "flags with {bad:?}");
        }
        let mut i = identity(HEAD);
        i.rustc = "rustc 1.97.1-nightly (a b c 2026-07-14) é".into();
        i.validate().unwrap();
    }

    #[test]
    fn the_short_form_never_panics_on_a_hand_built_identity() {
        let mut i = identity(HEAD);
        // byte 12 falls inside a two-byte character
        i.commit = format!("a{}", "é".repeat(20));
        assert!(i.validate().is_err());
        let short = std::panic::catch_unwind(|| i.short());
        assert!(short.is_ok(), "short() panicked");
        let mut i = identity(HEAD);
        i.commit = "abc".into();
        assert_eq!(i.short(), "0.3.1+abc");
    }

    #[test]
    fn a_mismatch_is_an_error_and_a_block_checks_itself() {
        let a = identity(HEAD);
        let mut b = a.clone();
        b.profile = "debug".into();
        let mismatch = run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err();
        let as_error: &dyn std::error::Error = &mismatch;
        assert!(as_error.to_string().contains("profile"));
        let block = read_engine(&engine_pairs(&a, EnginePolicy::Build, None).unwrap()).unwrap();
        assert!(block.check(&a, || panic!("hashed under build")).is_ok());
        assert_eq!(fields(&block.check(&b, || panic!("hashed under build")).unwrap_err()), ["profile"]);
        let hash = "a".repeat(64);
        let pinned = read_engine(&engine_pairs(&a, EnginePolicy::Binary, Some(&hash)).unwrap()).unwrap();
        assert!(pinned.check(&b, || Ok(hash.clone())).is_ok());
        assert_eq!(fields(&pinned.check(&a, || Ok("b".repeat(64))).unwrap_err()), ["binary_sha256"]);
    }

    #[test]
    fn policies_round_trip_and_refuse_anything_else() {
        for p in [EnginePolicy::Binary, EnginePolicy::Build, EnginePolicy::Release, EnginePolicy::Record] {
            assert_eq!(p.as_str().parse::<EnginePolicy>().unwrap(), p);
        }
        for bad in ["", "Build", "exact", "build ", "none"] {
            assert!(bad.parse::<EnginePolicy>().is_err(), "{bad:?}");
        }
        assert_eq!(EnginePolicy::default(), EnginePolicy::Build);
    }

    fn run_check(
        recorded: &EngineIdentity,
        recorded_binary: Option<&str>,
        policy: EnginePolicy,
        running: &EngineIdentity,
        running_binary: Result<&str, &str>,
    ) -> Result<(), EngineMismatch> {
        check(recorded, recorded_binary, policy, running, || {
            running_binary.map(str::to_string).map_err(|e| std::io::Error::other(e.to_string()))
        })
    }

    fn fields(mismatch: &EngineMismatch) -> Vec<&str> {
        mismatch.differences.iter().map(|d| d.field).collect()
    }

    #[test]
    fn record_never_refuses() {
        let a = identity(HEAD);
        let mut b = identity(UNKNOWN);
        b.version = "9.9.9".into();
        assert!(run_check(&a, None, EnginePolicy::Record, &b, Err("unreadable")).is_ok());
    }

    #[test]
    fn release_compares_only_the_version() {
        let a = identity(HEAD);
        let mut b = identity(UNKNOWN);
        b.profile = "debug".into();
        b.target = "aarch64-apple-darwin".into();
        b.rustc = "rustc 1.0.0".into();
        assert!(run_check(&a, None, EnginePolicy::Release, &b, Ok("x")).is_ok());
        b.version = "0.3.2".into();
        let m = run_check(&a, None, EnginePolicy::Release, &b, Ok("x")).unwrap_err();
        assert_eq!(fields(&m), ["version"]);
        assert_eq!((m.differences[0].recorded.as_str(), m.differences[0].running.as_str()), ("0.3.1", "0.3.2"));
    }

    #[test]
    fn build_compares_every_field_one_at_a_time() {
        let a = identity(HEAD);
        assert!(run_check(&a, None, EnginePolicy::Build, &a.clone(), Ok("x")).is_ok());
        let cases: [(&str, fn(&mut EngineIdentity)); 7] = [
            ("version", |i| i.version = "0.3.2".into()),
            ("commit", |i| i.commit = "89abcdef0123456789abcdef0123456789abcdef".into()),
            ("profile", |i| i.profile = "debug".into()),
            ("target", |i| i.target = "aarch64-apple-darwin".into()),
            ("features", |i| i.features = String::new()),
            ("rustc", |i| i.rustc = "rustc 1.98.0".into()),
            ("flags", |i| i.flags = "opt-level=0;debug-assertions=on;panic=unwind;rustflags=00;target-features=00".into()),
        ];
        for (field, change) in cases {
            let mut b = a.clone();
            change(&mut b);
            let m = run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err();
            assert_eq!(fields(&m), [field], "{field}");
        }
        let mut b = a.clone();
        b.version = "0.3.2".into();
        b.profile = "debug".into();
        assert_eq!(fields(&run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err()), ["version", "profile"]);
    }

    #[test]
    fn an_unknown_commit_never_equals_anything_not_even_itself() {
        let known = identity(HEAD);
        let unknown = identity(UNKNOWN);
        for (recorded, running) in [(&known, &unknown), (&unknown, &known), (&unknown, &unknown)] {
            let m = run_check(recorded, None, EnginePolicy::Build, running, Ok("x")).unwrap_err();
            assert_eq!(fields(&m), ["commit"]);
            assert!(m.to_string().contains("unknown"), "{m}");
        }
    }

    #[test]
    fn a_hash_recorded_under_build_is_ignored() {
        let a = identity(HEAD);
        let mut b = a.clone();
        b.profile = "debug".into();
        let m = run_check(&a, Some("ab12"), EnginePolicy::Build, &b, Ok("ab12")).unwrap_err();
        assert_eq!(fields(&m), ["profile"]);
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Build, &a, Ok("cd34")).is_ok(), "informational under build");
    }

    #[test]
    fn binary_compares_the_hash_and_needs_one_to_have_been_recorded() {
        let a = identity(HEAD);
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Ok("ab12")).is_ok());
        let m = run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Ok("cd34")).unwrap_err();
        assert_eq!(fields(&m), ["binary_sha256"]);
        let m = run_check(&a, None, EnginePolicy::Binary, &a, Ok("ab12")).unwrap_err();
        assert!(m.to_string().contains("not recorded"), "{m}");
        let m = run_check(&a, Some("ab12"), EnginePolicy::Binary, &a, Err("no such file")).unwrap_err();
        assert!(m.to_string().contains("no such file"), "{m}");
    }

    #[test]
    fn binary_does_not_look_at_the_other_fields_and_hashes_only_when_something_is_recorded() {
        let a = identity(HEAD);
        let mut b = identity(UNKNOWN);
        b.version = "0.3.2".into();
        assert!(run_check(&a, Some("ab12"), EnginePolicy::Binary, &b, Ok("ab12")).is_ok());
        let result = check(&a, None, EnginePolicy::Binary, &a, || panic!("hashed although nothing was recorded"));
        assert!(result.is_err());
    }

    #[test]
    fn the_hash_is_only_read_under_binary() {
        let a = identity(HEAD);
        for policy in [EnginePolicy::Record, EnginePolicy::Release, EnginePolicy::Build] {
            assert!(check(&a, Some("ab12"), policy, &a, || panic!("the executable was hashed under {policy:?}")).is_ok());
        }
    }

    #[test]
    fn the_mismatch_says_what_was_recorded_and_what_is_running() {
        let a = identity(HEAD);
        let mut b = a.clone();
        b.commit = "89abcdef0123456789abcdef0123456789abcdef".into();
        let text = run_check(&a, None, EnginePolicy::Build, &b, Ok("x")).unwrap_err().to_string();
        assert!(text.contains("build") && text.contains("commit") && text.contains(HEAD) && text.contains("89abcdef"), "{text}");
    }

    #[test]
    fn freezing_needs_a_known_commit_for_build_and_binary_only() {
        let unknown = identity(UNKNOWN);
        assert!(freezable(&unknown, EnginePolicy::Build).unwrap_err().contains("commit"));
        assert!(freezable(&unknown, EnginePolicy::Binary).unwrap_err().contains("commit"));
        assert!(freezable(&unknown, EnginePolicy::Release).is_ok());
        assert!(freezable(&unknown, EnginePolicy::Record).is_ok());
        for p in [EnginePolicy::Binary, EnginePolicy::Build, EnginePolicy::Release, EnginePolicy::Record] {
            assert!(freezable(&identity(HEAD), p).is_ok());
        }
    }

    use crate::frozen::model::Pairs;

    #[test]
    fn the_engine_block_round_trips() {
        let a = identity(HEAD);
        let hash = "a".repeat(64);
        let pairs = engine_pairs(&a, EnginePolicy::Binary, Some(&hash)).unwrap();
        assert_eq!((pairs.get("policy"), pairs.get("commit")), (Some("binary"), Some(HEAD)));
        let block = read_engine(&pairs).unwrap();
        assert_eq!((block.identity, block.policy, block.binary_sha256), (a.clone(), EnginePolicy::Binary, Some(hash)));
        let without = engine_pairs(&a, EnginePolicy::Build, None).unwrap();
        assert_eq!(without.get("binary_sha256"), None);
        assert_eq!(read_engine(&without).unwrap().binary_sha256, None);
    }

    #[test]
    fn building_a_block_that_could_never_pass_is_refused() {
        let a = identity(HEAD);
        assert!(engine_pairs(&a, EnginePolicy::Binary, None).unwrap_err().to_string().contains("binary_sha256"));
        assert!(engine_pairs(&a, EnginePolicy::Binary, Some("not a hash")).is_err());
        assert!(engine_pairs(&identity(UNKNOWN), EnginePolicy::Build, None).unwrap_err().to_string().contains("commit"));
        assert!(engine_pairs(&identity(UNKNOWN), EnginePolicy::Release, None).is_ok());
        let mut bad = identity(HEAD);
        bad.target = "two\nlines".into();
        assert!(engine_pairs(&bad, EnginePolicy::Build, None).is_err());
        bad.target = "x86_64-pc-windows-msvc".into();
        bad.commit = "Unknown".into();
        assert!(engine_pairs(&bad, EnginePolicy::Release, None).is_err());
    }

    fn rebuilt(pairs: &Pairs, edit: impl Fn(Vec<(String, String)>) -> Vec<(String, String)>) -> Pairs {
        Pairs::from_vec(edit(pairs.as_slice().to_vec())).unwrap()
    }

    #[test]
    fn the_engine_block_is_read_strictly() {
        let good = engine_pairs(&identity(HEAD), EnginePolicy::Build, None).unwrap();
        for key in ["version", "commit", "profile", "target", "features", "rustc", "flags", "policy"] {
            let missing = rebuilt(&good, |v| v.into_iter().filter(|(k, _)| k != key).collect());
            let err = read_engine(&missing).unwrap_err().to_string();
            assert!(err.contains(key), "{key}: {err}");
        }
        // an unknown key is an invisible knob
        let extra = rebuilt(&good, |mut v| { v.push(("enforce".into(), "never".into())); v });
        assert!(read_engine(&extra).unwrap_err().to_string().contains("enforce"));
        // a policy that does not exist
        let bad_policy = rebuilt(&good, |v| v.into_iter().map(|(k, x)| if k == "policy" { (k, "exact".into()) } else { (k, x) }).collect());
        assert!(read_engine(&bad_policy).unwrap_err().to_string().contains("exact"));
        // a hash that is not a hash
        let bad_hash = rebuilt(&good, |mut v| { v.push(("binary_sha256".into(), "xyz".into())); v });
        assert!(read_engine(&bad_hash).unwrap_err().to_string().contains("binary_sha256"));
        // every spelling of an unknown commit that is not exactly `unknown` is refused
        for spelling in ["", "Unknown", "UNKNOWN", "main"] {
            let block = rebuilt(&good, |v| v.into_iter().map(|(k, x)| if k == "commit" { (k, spelling.into()) } else { (k, x) }).collect());
            assert!(read_engine(&block).is_err(), "commit {spelling:?}");
        }
        // unsorted or duplicated features are refused
        for features in ["b,a", "a,a"] {
            let block = rebuilt(&good, |v| v.into_iter().map(|(k, x)| if k == "features" { (k, features.into()) } else { (k, x) }).collect());
            assert!(read_engine(&block).is_err(), "features {features:?}");
        }
    }

    #[test]
    fn the_environment_record_is_complete_and_valid() {
        let env = environment_pairs();
        for key in ["os", "arch", "family", "endian", "pointer_width", "cpus"] {
            assert!(env.get(key).is_some(), "{key}");
        }
        assert!(["little", "big"].contains(&env.get("endian").unwrap()));
        assert!(env.get("pointer_width").unwrap().parse::<u32>().is_ok());
        assert!(env.get("cpus").unwrap().parse::<usize>().unwrap() >= 1);
    }

    #[test]
    fn hashing_a_file_agrees_with_hashing_its_bytes() {
        let path = std::env::current_exe().unwrap();
        let direct = crate::frozen::sha256::sha256_hex(&std::fs::read(&path).unwrap());
        assert_eq!(sha256_of_file(&path).unwrap(), direct);
        assert!(crate::frozen::canonical::is_sha256_hex(&direct));
    }

    #[test]
    fn hashing_a_missing_file_is_an_error() {
        assert!(sha256_of_file(std::path::Path::new("definitely/not/here.bin")).is_err());
    }
}
