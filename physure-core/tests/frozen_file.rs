//! The frozen-file layer through its public API only.

use physure_core::frozen::model::{FrozenFile, Source, WitnessScript};
use physure_core::frozen::seal::EntryInput;
use physure_core::frozen::sha256::sha256_hex;
use physure_core::frozen::time::rfc3339_from_unix;

fn entry<'a>(action: &'a str, reason: &'a str, time: &'a str) -> EntryInput<'a> {
    EntryInput { time, action, engine: "0.0.0+test", actor: "ci@host", reason, diff: "n/a" }
}

fn file() -> FrozenFile {
    FrozenFile::new(
        &rfc3339_from_unix(1_700_000_000),
        vec![("version".into(), "0.0.0".into())],
        vec![],
        vec![],
        vec![
            Source::new("embedded", "[Constants]\nmolar_gas_constant = 8.31446261815324 J*mol^-1*K^-1").unwrap(),
            Source::new("overrides", "").unwrap(),
        ],
        vec![WitnessScript::new("w", "1 m => ft").unwrap()],
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
