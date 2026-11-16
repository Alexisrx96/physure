//! The frozen-file layer through its public API only.

use physure_core::frozen::canonical::exact_sha256;
use physure_core::frozen::sha256::sha256_hex;
use physure_core::frozen::time::rfc3339_from_unix;
use physure_core::frozen::{
    engine_pairs, EngineIdentity, EnginePolicy, EntryInput, FrozenFile, FrozenParts, Pairs, Source, WitnessScript,
};

fn entry<'a>(action: &'a str, reason: &'a str, time: &'a str) -> EntryInput<'a> {
    EntryInput { time, action, engine: "0.0.0+test", actor: "ci@host", reason, diff: "n/a" }
}

/// A valid `[Engine]` block with fixed fields, independent of the build running the test.
fn engine() -> Pairs {
    let identity = EngineIdentity {
        version: "0.0.0".into(),
        commit: "unknown".into(),
        profile: "debug".into(),
        target: "x86_64-unknown-linux-gnu".into(),
        features: String::new(),
        rustc: "rustc 1.97.1 (8bab26f4f 2026-07-14)".into(),
        flags: "opt-level=0;debug-assertions=on;panic=unwind;rustflags=00;target-features=00".into(),
    };
    engine_pairs(&identity, EnginePolicy::Release, None).unwrap()
}

fn file() -> FrozenFile {
    let created = rfc3339_from_unix(1_700_000_000).unwrap();
    FrozenFile::new(
        &created,
        FrozenParts {
            engine: engine(),
            environment: Pairs::default(),
            witnesses: Pairs::new(&[("user.w", "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")]).unwrap(),
            sources: vec![
                Source::new("embedded", "[Constants]\nmolar_gas_constant = 8.31446261815324 J*mol^-1*K^-1").unwrap(),
                Source::new("overrides", "").unwrap(),
            ],
            witness_scripts: vec![WitnessScript::new("w", "1 m => ft").unwrap()],
        },
        entry("freeze", "initial", "2023-11-14T22:13:20Z"),
    )
    .unwrap()
}

#[test]
fn the_documented_hash_of_the_empty_string_is_what_sha256_says() {
    assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
}

#[test]
fn a_file_survives_text_and_every_single_byte_change_to_its_data_is_caught() {
    let text = file().render().unwrap();
    assert!(FrozenFile::parse(&text).unwrap().verify().is_empty());
    let marker = text.find("8.314").unwrap();
    // change each digit of the constant in turn: the seal must break every time
    for offset in 0..15 {
        let at = marker + offset;
        let c = text.as_bytes()[at];
        if !c.is_ascii_digit() {
            continue;
        }
        let mut bytes = text.clone().into_bytes();
        bytes[at] = if c == b'9' { b'0' } else { c + 1 };
        let edited = FrozenFile::parse(&String::from_utf8(bytes).unwrap()).unwrap();
        assert!(!edited.verify().is_empty(), "digit at offset {offset} went unnoticed");
    }
}

#[test]
fn the_seal_does_not_depend_on_the_environment_block_nor_on_comments() {
    let text = file().render().unwrap();
    let with_env = text.replace("\n[Environment]\n", "\n[Environment]\nos = linux\n");
    assert!(FrozenFile::parse(&with_env).unwrap().verify().is_empty());
    let with_comment = text.replace("[Constants]\n", "[Constants]\n# a comment\n");
    assert!(FrozenFile::parse(&with_comment).unwrap().verify().is_empty());
}

#[test]
fn a_witness_script_edited_together_with_its_marker_hash_still_breaks_the_seal() {
    let text = file().render().unwrap();
    let edited = text.replace("1 m => ft", "1 m => yd").replace(&exact_sha256("1 m => ft"), &exact_sha256("1 m => yd"));
    assert!(FrozenFile::parse(&edited).unwrap().verify().iter().any(|p| p.to_string().contains("seal")));
}

#[test]
fn the_public_api_reads_the_file_and_reseals_it() {
    let mut f = file();
    let sealed = f.seal_sha256().to_string();
    assert_eq!(f.format(), 1);
    assert_eq!(f.created(), "2023-11-14T22:13:20Z");
    assert_eq!(f.engine().get("version"), Some("0.0.0"));
    assert!(f.environment().is_empty());
    assert_eq!(f.witnesses().len(), 1);
    assert_eq!(f.sources()[0].origin(), "embedded");
    assert_eq!(f.witness_scripts()[0].sha256(), exact_sha256("1 m => ft"));
    assert_eq!(f.history()[0].new_content_sha256(), f.content_sha256());
    f.set_source_text(1, "[Constants]\nmolar_gas_constant = 8.314 J*mol^-1*K^-1").unwrap();
    assert!(!f.verify().is_empty(), "a change is visible until the file is resealed");
    f.reseal(entry("seal", "rounded R for the lab", "2023-11-15T08:00:00Z")).unwrap();
    assert!(f.verify().is_empty());
    assert_ne!(f.seal_sha256(), sealed);
    assert_eq!(f.history()[1].action(), "seal");
    assert_eq!(FrozenFile::parse(&f.render().unwrap()).unwrap(), f);
}
