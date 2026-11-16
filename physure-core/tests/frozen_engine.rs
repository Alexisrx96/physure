//! The engine block of a frozen file, through the public API only.

use physure_core::frozen::engine::{check, engine_pairs, environment_pairs, read_engine, EngineIdentity, EnginePolicy, UNKNOWN};
use physure_core::frozen::model::{FrozenFile, Pairs, Source};
use physure_core::frozen::seal::{EntryInput, FrozenParts};

const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

fn known() -> EngineIdentity {
    let mut i = EngineIdentity::current();
    i.commit = HEAD.into();
    i
}

fn file(identity: &EngineIdentity, policy: EnginePolicy) -> FrozenFile {
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(identity, policy, None).unwrap(),
            environment: environment_pairs(),
            witnesses: Pairs::default(),
            sources: vec![Source::new("embedded", "[Constants]\nx = 1 m").unwrap(), Source::new("overrides", "").unwrap()],
            witness_scripts: vec![],
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: &identity.short(), actor: "ci@host", reason: "test", diff: "n/a" },
    )
    .unwrap()
}

#[test]
fn a_file_frozen_with_a_known_engine_checks_against_it_and_against_nothing_else() {
    let id = known();
    let parsed = FrozenFile::parse(&file(&id, EnginePolicy::Build).render().unwrap()).unwrap();
    assert!(parsed.verify().is_empty());
    let block = read_engine(parsed.engine()).unwrap();
    assert_eq!(block.identity, id);
    assert!(check(&block.identity, None, block.policy, &id, || unreachable!()).is_ok());
    let mut other = id.clone();
    other.rustc = "rustc 0.0.0".into();
    assert!(check(&block.identity, None, block.policy, &other, || unreachable!()).is_err());
}

#[test]
fn the_running_engine_can_always_be_recorded_under_release_and_record() {
    let now = EngineIdentity::current();
    for policy in [EnginePolicy::Release, EnginePolicy::Record] {
        let parsed = FrozenFile::parse(&file(&now, policy).render().unwrap()).unwrap();
        assert!(parsed.verify().is_empty());
    }
    // build is recorded only if the running build knows its commit; binary needs no commit
    let build = engine_pairs(&now, EnginePolicy::Build, None);
    assert_eq!(build.is_ok(), now.commit != UNKNOWN);
    assert!(engine_pairs(&now, EnginePolicy::Binary, Some(&"0".repeat(64))).is_ok());
}

#[test]
fn weakening_the_policy_in_the_text_breaks_the_seal() {
    let text = file(&known(), EnginePolicy::Build).render().unwrap();
    let weakened = text.replace("policy = build", "policy = record");
    assert_ne!(weakened, text);
    assert!(!FrozenFile::parse(&weakened).unwrap().verify().is_empty());
}

#[test]
fn an_unknown_key_slipped_into_the_engine_block_is_refused_by_the_seal_and_by_the_reader() {
    let text = file(&known(), EnginePolicy::Build).render().unwrap();
    let with_knob = text.replace("[Engine]\n", "[Engine]\nenforce = never\n");
    let parsed = FrozenFile::parse(&with_knob).unwrap();
    assert!(!parsed.verify().is_empty());
    assert!(read_engine(parsed.engine()).is_err());
}

#[test]
fn the_policy_is_part_of_what_the_seal_protects_and_of_what_the_log_records() {
    let a = file(&known(), EnginePolicy::Build);
    let b = file(&known(), EnginePolicy::Release);
    assert_ne!(a.seal_sha256(), b.seal_sha256());
    assert_ne!(a.history()[0].engine_sha256(), b.history()[0].engine_sha256());
}

#[test]
fn weakening_the_policy_with_an_innocuous_reason_shows_in_the_log_as_policy_and_pinned() {
    let mut f = file(&known(), EnginePolicy::Build);
    f.set_engine(engine_pairs(&known(), EnginePolicy::Record, None).unwrap());
    f.reseal(EntryInput { time: "2026-11-08T11:00:00Z", action: "seal", engine: "9.9.9+ffffffffffff", actor: "a@h", reason: "routine reseal", diff: "no changes" })
        .unwrap();
    assert!(f.verify().is_empty());
    let pinned = known().short();
    let log: Vec<(&str, &str)> = f.history().iter().map(|e| (e.policy(), e.pinned())).collect();
    assert_eq!(log, [("build", pinned.as_str()), ("record", pinned.as_str())]);
    // a block that can never be checked (build with no commit) cannot be sealed, however it is written
    let mut unknown = known();
    unknown.commit = UNKNOWN.into();
    let mut forged: Vec<(String, String)> = engine_pairs(&unknown, EnginePolicy::Release, None).unwrap().as_slice().to_vec();
    for (key, value) in &mut forged {
        if key == "policy" {
            *value = "build".into();
        }
    }
    f.set_engine(Pairs::from_vec(forged).unwrap());
    assert!(f.reseal(EntryInput { time: "2026-11-08T12:00:00Z", action: "seal", engine: "e", actor: "a@h", reason: "r", diff: "d" }).is_err());
}
