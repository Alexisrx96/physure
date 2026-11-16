//! The headers of the sections of a conf. A header that is a near miss of a section physure reads
//! (`[chemistry]`, `[Chemistry ]`, `[Atomicweights]`) would otherwise be an unknown section, which is ignored
//! without a word: the choice or the weights it holds would silently fall back to the defaults. Strict reading
//! (`extend_registry`, a unit system) refuses it and names the spelling; a header that is not close to any known
//! section is still ignored, because a system conf may use sections of its own.

use physure_core::units::conf::{build_registry_with, extend_registry, parse_physure_conf};
use physure_core::units::UnitRegistry;
use std::collections::HashMap;

const KNOWN: [&str; 9] =
    ["Settings", "Dimensions", "Prefixes", "Units", "Constants", "Categories", "Compatibility", "Chemistry", "AtomicWeights"];

fn embedded() -> (UnitRegistry, HashMap<String, String>) {
    build_registry_with("").unwrap()
}

/// The problems of reading `text` strictly, which must be one, with the registry as it was.
fn the_one_problem(text: &str) -> String {
    let (mut registry, mut constants) = embedded();
    let before = registry.clone();
    let problems = extend_registry(&mut registry, &mut constants, text).unwrap_err();
    assert_eq!(problems.len(), 1, "{text:?}: {problems:?}");
    assert_eq!(registry, before, "{text:?} left a trace");
    problems.into_iter().next().unwrap()
}

#[test]
fn a_misspelt_chemistry_or_atomic_weights_header_is_a_problem_naming_the_right_spelling() {
    for (header, right) in [
        ("[chemistry]", "Chemistry"),
        ("[CHEMISTRY]", "Chemistry"),
        ("[Chemistry ]", "Chemistry"),
        ("[ Chemistry]", "Chemistry"),
        ("[Atomicweights]", "AtomicWeights"),
        ("[atomicweights]", "AtomicWeights"),
        ("[ AtomicWeights ]", "AtomicWeights"),
        ("[atomicweights:acme-1]", "AtomicWeights:acme-1"),
        ("[Atomicweights:acme-1]", "AtomicWeights:acme-1"),
        ("[ AtomicWeights:acme-1 ]", "AtomicWeights:acme-1"),
        ("[AtomicWeights :acme-1]", "AtomicWeights:acme-1"),
        ("[AtomicWeights: acme-1]", "AtomicWeights:acme-1"),
    ] {
        // with a body that would have been read had the header been right, and with none
        for body in ["atomic_weights = abridged\nFe = 56.0, 0.3\n", ""] {
            let problem = the_one_problem(&format!("{header}\n{body}"));
            assert!(problem.contains(&format!("[{right}]")), "{header}: {problem}");
            assert!(problem.contains(header), "{header}: the problem names the header as written: {problem}");
        }
    }
}

#[test]
fn every_other_known_section_has_the_same_rule() {
    for right in KNOWN {
        let misspelt = [
            right.to_ascii_lowercase(),
            right.to_ascii_uppercase(),
            format!("{right} "),
            format!(" {right}"),
            format!("  {right}\t"),
        ];
        for wrong in misspelt {
            let header = format!("[{wrong}]");
            let problem = the_one_problem(&format!("{header}\nX = 1\n"));
            assert!(problem.contains(&format!("[{right}]")), "{header}: {problem}");
        }
    }
}

#[test]
fn the_right_spellings_and_unknown_sections_are_not_problems() {
    for right in KNOWN {
        let (mut registry, mut constants) = embedded();
        // an empty body: nothing in it can be a problem of its own
        extend_registry(&mut registry, &mut constants, &format!("[{right}]\n")).unwrap();
    }
    let (mut registry, mut constants) = embedded();
    let before = registry.clone();
    // a system conf has sections of its own: they are not close to any section that is read
    extend_registry(&mut registry, &mut constants, "[System]\nname = x\n[Whatever]\ny = 2\n[AtomicWeight]\nz = 3\n").unwrap();
    assert_eq!(registry, before);
}

#[test]
fn the_lines_of_a_misspelt_section_are_not_read_and_are_not_problems_of_their_own() {
    // `Fe = abc, 0.1` would be a problem in [AtomicWeights]; under a misspelt header there is one problem, the header
    let problem = the_one_problem("[atomicweights]\nFe = abc, 0.1\natomic_weights = nonsense name\n");
    assert!(problem.contains("[AtomicWeights]"), "{problem}");
}

#[test]
fn the_lenient_reading_ignores_a_misspelt_section_and_keeps_the_defaults() {
    // The working-directory conf is read leniently (and reports the problem on stderr, see the unit tests of conf.rs).
    let (mut registry, mut constants) = embedded();
    let before = registry.clone();
    parse_physure_conf("[chemistry]\natomic_weights = abridged\n[atomicweights]\nFe = 56.0, 0.3\n", &mut registry, &mut constants);
    assert_eq!(registry.atomic_weight_selection, before.atomic_weight_selection);
    assert!(registry.atomic_weights.is_empty());
}
