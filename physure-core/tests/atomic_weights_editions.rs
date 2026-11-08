//! Named, frozen editions of the atomic weights, your definitions, and the selector of `[Chemistry]`.
//!
//! - A *table* is a section `[AtomicWeights:<edition>]`. Two are shipped and frozen: `standard-2024` (the CIAAW
//!   standard atomic weights, with intervals) and `abridged-2024` (the CIAAW abridged table). Any text can define a
//!   complete table of its own under its own name.
//! - `[Chemistry] atomic_weights = <edition or alias>` selects the table that is read. The aliases `standard` and
//!   `abridged` move to the newest shipped edition of their kind; a pinned name never changes meaning.
//! - `[AtomicWeights]` (no suffix) holds your definitions: they win over whichever table is selected and over
//!   nothing else, in any order. There is no fallback from one table to another.
//! - `atomic_weight_edition()` says which table produced a result, so a report can quote it.
//!
//! The expected numbers are hand-written from the CIAAW tables (`tests/golden/ciaaw_*_atomic_weights_2024.tsv`),
//! never read back from the code under test. The embedded tables are held to the TSVs in `atomic_weights_ciaaw.rs`.

use physure_core::units::conf::{build_registry_with, extend_registry};
use physure_core::units::{resolve_atomic_weight_edition, UnitRegistry};
use std::collections::HashMap;

/// The shipped catalog and nothing else: a `physure.conf` in the working directory must not change what these
/// tests assert about the shipped editions.
fn embedded_catalog() -> (UnitRegistry, HashMap<String, String>) {
    build_registry_with("").unwrap()
}

const STANDARD: &str = "standard-2024";
const ABRIDGED: &str = "abridged-2024";

/// What the edition report says about the definitions `Fe = 56.0, 0.3`: their number and the FNV-1a 64 hash of
/// their canonical text `Fe=<weight bits>,<standard uncertainty bits>;`, computed outside this code (Python,
/// `struct.pack('>d', x).hex()`). The weight is 56.0 and the standard uncertainty is 0.3 / sqrt(3).
const FE_56: &str = "1 definitions (fnv1a:e03dd5b5b2476b01)";

/// The embedded catalog with `texts` applied one after the other.
fn registry_with(texts: &[&str]) -> UnitRegistry {
    let (mut registry, mut constants) = embedded_catalog();
    for text in texts {
        extend_registry(&mut registry, &mut constants, text).unwrap();
    }
    registry
}

fn select(name: &str) -> String {
    format!("[Chemistry]\natomic_weights = {name}\n")
}

/// The H, Na and Tc entries of the abridged table, as CIAAW prints them (1.0080 +- 0.0002, 22.990 +- 0.001, 98).
fn assert_abridged_values(registry: &UnitRegistry) {
    assert_eq!(registry.atomic_weight("H"), Some((1.0080, 0.0002 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weight("Na"), Some((22.990, 0.001 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weight("Tc"), Some((98.0, 0.0)));
}

// (a) defaults

#[test]
fn by_default_standard_2024_is_read_and_nothing_is_defined_by_the_user() {
    let registry = embedded_catalog().0;
    // H is the interval [1.00784, 1.00811] of the standard table
    assert_eq!(
        registry.atomic_weight("H"),
        Some(((1.00784 + 1.00811) / 2.0, (1.00811 - 1.00784) / 12f64.sqrt()))
    );
    assert_eq!(registry.atomic_weight_edition(), "standard-2024");
    assert_eq!(registry.atomic_weight_tables.len(), 2, "exactly the two shipped editions");
    assert_eq!(registry.atomic_weight_tables[STANDARD].len(), 118);
    assert_eq!(registry.atomic_weight_tables[ABRIDGED].len(), 118);
    assert!(registry.atomic_weights.is_empty(), "the definitions of the user are empty in the shipped file");
    assert_eq!(registry.atomic_weight_selection, "standard", "the shipped file selects the alias `standard`");
    assert_eq!(registry.selected_atomic_weight_table_missing(), None);
    assert_eq!(registry.atomic_weight("Xx"), None);
}

// (b) alias and pin

#[test]
fn an_alias_and_the_edition_it_stands_for_read_the_same_values_for_all_118_elements() {
    for (alias, pinned) in [("standard", STANDARD), ("abridged", ABRIDGED)] {
        let by_alias = registry_with(&[&select(alias)]);
        let by_name = registry_with(&[&select(pinned)]);
        assert_eq!(by_alias.atomic_weight_edition(), pinned);
        assert_eq!(by_name.atomic_weight_edition(), pinned);
        let symbols: Vec<&String> = by_name.atomic_weight_tables[pinned].keys().collect();
        assert_eq!(symbols.len(), 118);
        for symbol in symbols {
            assert!(by_alias.atomic_weight(symbol).is_some(), "{symbol}");
            assert_eq!(by_alias.atomic_weight(symbol), by_name.atomic_weight(symbol), "{alias} vs {pinned}: {symbol}");
        }
    }
}

#[test]
fn the_abridged_edition_is_the_table_ciaaw_prints_with_its_conservative_plus_minus() {
    let registry = registry_with(&[&select(ABRIDGED)]);
    assert_abridged_values(&registry);
    assert_eq!(registry.atomic_weight_edition(), "abridged-2024");
    // selecting does not touch either table
    assert_eq!(registry.atomic_weight_tables[STANDARD]["Na"], (22.98976928, 0.00000002 / 3f64.sqrt()));
    assert_eq!(registry.atomic_weight_tables[ABRIDGED]["Na"], (22.990, 0.001 / 3f64.sqrt()));
}

// (c) your definitions win over whichever table is selected, in any order

#[test]
fn a_definition_of_yours_wins_under_the_standard_the_abridged_and_a_custom_table() {
    let definitions = "[AtomicWeights]\nFe = 56.0, 0.3\n";
    let custom = "[AtomicWeights:acme-2026-03]\nFe = 55.9, 0.1\nH = 1.00794, 0.00007\n";
    let expected = Some((56.0, 0.3 / 3f64.sqrt()));
    for (selection, label) in [
        (select("standard"), format!("standard-2024 + {FE_56}")),
        (select(STANDARD), format!("standard-2024 + {FE_56}")),
        (select("abridged"), format!("abridged-2024 + {FE_56}")),
        (select(ABRIDGED), format!("abridged-2024 + {FE_56}")),
        (select("acme-2026-03"), format!("acme-2026-03 + {FE_56}")),
    ] {
        let one_text_each_way = [
            registry_with(&[&format!("{definitions}{custom}{selection}")]),
            registry_with(&[&format!("{selection}{custom}{definitions}")]),
            registry_with(&[&format!("{custom}{selection}{definitions}")]),
        ];
        let several_texts = [
            registry_with(&[definitions, custom, &selection]),
            registry_with(&[&selection, custom, definitions]),
            registry_with(&[custom, definitions, &selection]),
            registry_with(&[&selection, definitions, custom]),
        ];
        for registry in one_text_each_way.iter().chain(&several_texts) {
            assert_eq!(registry.atomic_weight("Fe"), expected, "{label}");
            assert_eq!(registry.atomic_weight_edition(), label);
            // the published tables are untouched by a definition
            assert_eq!(registry.atomic_weight_tables[STANDARD]["Fe"], (55.845, 0.002 / 3f64.sqrt()));
            assert_eq!(registry.atomic_weight_tables[ABRIDGED]["Fe"], (55.845, 0.002 / 3f64.sqrt()));
            assert_eq!(registry.atomic_weight_tables["acme-2026-03"]["Fe"], (55.9, 0.1 / 3f64.sqrt()));
            assert_eq!(registry.atomic_weights.len(), 1);
        }
    }
}

#[test]
fn what_you_did_not_define_is_read_from_the_selected_table_only() {
    let definitions = "[AtomicWeights]\nFe = 56.0, 0.3\n";
    let abridged = registry_with(&[definitions, &select("abridged")]);
    assert_eq!(abridged.atomic_weight("Na"), Some((22.990, 0.001 / 3f64.sqrt())));
    let standard = registry_with(&[definitions]);
    assert_eq!(standard.atomic_weight("Na"), Some((22.98976928, 0.00000002 / 3f64.sqrt())));
}

#[test]
fn a_later_definition_replaces_an_earlier_one() {
    let registry = registry_with(&["[AtomicWeights]\nFe = 56.0, 0.5\n", "[AtomicWeights]\nFe = 57.0, 0.7\nCo = 59.0, 0.2\n"]);
    assert_eq!(registry.atomic_weight("Fe"), Some((57.0, 0.7 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weights.len(), 2, "only the definitions of the user");
    assert_eq!(registry.atomic_weight_edition(), "standard-2024 + 2 definitions (fnv1a:3fccc6fb20553b8a)");
    // and inside one text
    let registry = registry_with(&["[AtomicWeights]\nFe = 56.0, 0.5\nFe = 58.0, 0.9\n"]);
    assert_eq!(registry.atomic_weight("Fe"), Some((58.0, 0.9 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weight_edition(), "standard-2024 + 1 definitions (fnv1a:e9d85fb980104d81)");
}

#[test]
fn a_later_line_of_the_same_symbol_in_a_table_replaces_the_earlier_one_in_that_table_only() {
    let registry = registry_with(&["[AtomicWeights:acme-1]\nH = 1.0, 0.1\nH = 1.5, 0.2\n"]);
    assert_eq!(registry.atomic_weight_tables["acme-1"]["H"], (1.5, 0.2 / 3f64.sqrt()));
    assert_eq!(registry.atomic_weight_tables.len(), 3);
}

// (d) an added isotope

#[test]
fn an_added_isotope_is_found_under_both_shipped_editions() {
    let definitions = "[AtomicWeights]\nD = 2.01410177812, 0.00000000012, standard\n";
    assert_eq!(embedded_catalog().0.atomic_weight("D"), None, "in neither shipped table");
    for selection in [select("standard"), select(STANDARD), select("abridged"), select(ABRIDGED)] {
        for registry in [
            registry_with(&[&format!("{definitions}{selection}")]),
            registry_with(&[&format!("{selection}{definitions}")]),
            registry_with(&[definitions, &selection]),
            registry_with(&[&selection, definitions]),
        ] {
            assert_eq!(registry.atomic_weight("D"), Some((2.01410177812, 0.00000000012)), "{selection:?}");
        }
    }
}

// (e) a complete table of your own, with no fallback

#[test]
fn a_complete_table_of_your_own_takes_nothing_from_the_shipped_ones() {
    let registry = registry_with(&[
        "[AtomicWeights:acme-2026-03]\nH = 1.00794, 0.00007\nO = 15.9994, 0.0003\n[Chemistry]\natomic_weights = acme-2026-03\n",
    ]);
    assert_eq!(registry.atomic_weight("H"), Some((1.00794, 0.00007 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weight("O"), Some((15.9994, 0.0003 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weight("Fe"), None, "nothing is taken from the shipped tables");
    assert_eq!(registry.atomic_weight("Na"), None);
    assert_eq!(registry.atomic_weight_edition(), "acme-2026-03");
    assert_eq!(registry.selected_atomic_weight_table_missing(), None);
    // and the shipped tables are still there, unchanged, for whoever selects them
    assert_eq!(registry.atomic_weight_tables[STANDARD].len(), 118);
    assert_eq!(registry.atomic_weight_tables[ABRIDGED].len(), 118);
}

#[test]
fn your_definitions_still_apply_over_a_complete_table_of_your_own() {
    let registry = registry_with(&[
        "[AtomicWeights:acme-2026-03]\nH = 1.00794, 0.00007\n[Chemistry]\natomic_weights = acme-2026-03\n[AtomicWeights]\nFe = 56.0, 0.3\n",
    ]);
    assert_eq!(registry.atomic_weight("Fe"), Some((56.0, 0.3 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weight("Na"), None);
    assert_eq!(registry.atomic_weight_edition(), format!("acme-2026-03 + {FE_56}"));
}

// (f) the selected edition is not defined

#[test]
fn a_selected_edition_that_is_not_defined_is_reported_until_some_text_defines_it() {
    let mut registry = registry_with(&[&select("nope-1")]);
    assert_eq!(registry.selected_atomic_weight_table_missing(), Some("nope-1"));
    assert_eq!(registry.atomic_weight("H"), None, "no fallback to any other table");
    assert_eq!(registry.atomic_weight_edition(), "nope-1 (undefined)", "the report names what was asked for, and says it is not defined");
    // another text defines it: the problem is gone
    let mut constants = std::collections::HashMap::new();
    extend_registry(&mut registry, &mut constants, "[AtomicWeights:nope-1]\nH = 1.0, 0.1\n").unwrap();
    assert_eq!(registry.selected_atomic_weight_table_missing(), None);
    assert_eq!(registry.atomic_weight("H"), Some((1.0, 0.1 / 3f64.sqrt())));
    assert_eq!(registry.atomic_weight_edition(), "nope-1");
}

#[test]
fn the_order_of_the_choice_and_of_the_table_does_not_matter() {
    let table = "[AtomicWeights:nope-1]\nH = 1.0, 0.1\n";
    let choice = select("nope-1");
    for registry in [
        registry_with(&[&format!("{table}{choice}")]),
        registry_with(&[&format!("{choice}{table}")]),
        registry_with(&[table, &choice]),
        registry_with(&[&choice, table]),
    ] {
        assert_eq!(registry.selected_atomic_weight_table_missing(), None);
        assert_eq!(registry.atomic_weight("H"), Some((1.0, 0.1 / 3f64.sqrt())));
    }
}

#[test]
fn an_alias_is_resolved_when_it_is_used_not_when_it_is_read() {
    let mut registry = registry_with(&[&select("abridged")]);
    assert_eq!(registry.atomic_weight_selection, "abridged", "what was selected is kept as written");
    assert_eq!(registry.atomic_weight_edition(), "abridged-2024");
    assert_eq!(registry.selected_atomic_weight_table_missing(), None);
    let mut constants = std::collections::HashMap::new();
    extend_registry(&mut registry, &mut constants, &select("standard")).unwrap();
    assert_eq!(registry.atomic_weight_selection, "standard");
}

// (g) text that does not read

#[test]
fn text_that_does_not_read_is_refused_with_one_problem_and_leaves_no_trace() {
    let mut texts: Vec<String> = [
        "[Chemistry]\natomic_weights = \n",
        "[Chemistry]\nunits = si\n",
        "[Chemistry]\natomic_weights\n",
        "[Chemistry]\natomic_weights = Bad Name\n",
        "[AtomicWeights:Bad Name]\n",
        "[AtomicWeights:Bad Name]\nFe = 56.0, 0.3\n",
        "[AtomicWeights:]\n",
        "[AtomicWeights:]\nFe = 56.0, 0.3\n",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    for section in ["AtomicWeights", "AtomicWeights:acme-1"] {
        texts.push(format!("[{section}]\nFe = abc, 0.1\n"));
    }
    for text in &texts {
        let (mut registry, mut constants) = embedded_catalog();
        let before = registry.clone();
        // a good selection and a good definition before the bad one must not survive the refusal
        let full = format!("[Chemistry]\natomic_weights = abridged\n[AtomicWeights]\nCo = 59.0, 0.2\n{text}");
        let problems = extend_registry(&mut registry, &mut constants, &full).unwrap_err();
        assert_eq!(problems.len(), 1, "{text:?}: {problems:?}");
        assert_eq!(registry, before, "{text:?} left a trace");
        // and alone
        let problems = extend_registry(&mut registry, &mut constants, text).unwrap_err();
        assert_eq!(problems.len(), 1, "{text:?}: {problems:?}");
        assert_eq!(registry, before, "{text:?} left a trace");
    }
}

#[test]
fn edition_names_are_lowercase_ascii_letters_digits_and_dash_underscore_dot_starting_with_a_letter() {
    let longest = format!("a{}", "b".repeat(63));
    for good in ["a", "acme-2026-03", "x_1.2", "standard2026", "abridgedx", longest.as_str()] {
        let registry = registry_with(&[&format!("[AtomicWeights:{good}]\nH = 1.0, 0.1\n"), &select(good)]);
        assert_eq!(registry.atomic_weight("H"), Some((1.0, 0.1 / 3f64.sqrt())), "{good}");
    }
    let too_long = format!("a{}", "b".repeat(64));
    for bad in ["", "1abc", "-a", "_a", ".a", "Acme", "a b", "a/b", "a:b", "é", "a=b", too_long.as_str()] {
        for text in [format!("[AtomicWeights:{bad}]\nH = 1.0, 0.1\n"), select(bad)] {
            let (mut registry, mut constants) = embedded_catalog();
            let before = registry.clone();
            let problems = extend_registry(&mut registry, &mut constants, &text).unwrap_err();
            assert_eq!(problems.len(), 1, "{text:?}: {problems:?}");
            assert_eq!(registry, before, "{text:?} left a trace");
        }
    }
}

#[test]
fn a_table_cannot_be_called_like_an_alias_because_the_alias_would_shadow_it() {
    for alias in ["standard", "abridged"] {
        let (mut registry, mut constants) = embedded_catalog();
        let before = registry.clone();
        let text = format!("[AtomicWeights:{alias}]\nH = 1.0, 0.1\n");
        let problems = extend_registry(&mut registry, &mut constants, &text).unwrap_err();
        assert_eq!(problems.len(), 1, "{alias}: {problems:?}");
        assert!(problems[0].contains(alias) && problems[0].contains(resolve_atomic_weight_edition(alias)), "{problems:?}");
        assert_eq!(registry, before);
    }
}

// (g2) the shipped editions are frozen against user text, and their names are reserved

/// The one problem of reading `text` strictly, the registry as it was.
fn the_one_problem(text: &str) -> String {
    let (mut registry, mut constants) = embedded_catalog();
    let before = registry.clone();
    let problems = extend_registry(&mut registry, &mut constants, text).unwrap_err();
    assert_eq!(problems.len(), 1, "{text:?}: {problems:?}");
    assert_eq!(registry, before, "{text:?} left a trace");
    problems.into_iter().next().unwrap()
}

#[test]
fn user_text_cannot_edit_a_shipped_edition() {
    for name in [STANDARD, ABRIDGED] {
        // with a line, with a line that would not read either, and with none: one problem, about the section
        for body in ["H = 1.0, 0.1\n", "H = 1.0, 0.1\nFe = abc, 0.1\n", "Zz = 1.0, 0.1\n", ""] {
            let problem = the_one_problem(&format!("[AtomicWeights:{name}]\n{body}"));
            assert!(problem.contains(name) && problem.contains("frozen"), "{problem}");
        }
        // nor can it be redefined by a text that also selects it
        let problem = the_one_problem(&format!("[Chemistry]\natomic_weights = {name}\n[AtomicWeights:{name}]\nH = 1.0, 0.1\n"));
        assert!(problem.contains("frozen"), "{problem}");
    }
}

#[test]
fn the_names_of_future_shipped_editions_are_reserved() {
    // a user table called `standard-2026` would be merged with the shipped one the day a release adds it
    for name in ["standard-2026", "abridged-2026", "standard-", "abridged-x", "standard-acme", "abridged-2024-b"] {
        let problem = the_one_problem(&format!("[AtomicWeights:{name}]\nH = 1.0, 0.1\n"));
        assert!(problem.contains(name) && problem.contains("reserved"), "{problem}");
    }
    // other names are the user's, and so is selecting a reserved one (it is checked when it is used)
    let registry = registry_with(&["[AtomicWeights:standardized-1]\nH = 1.0, 0.1\n", &select("standard-2026")]);
    assert_eq!(registry.selected_atomic_weight_table_missing(), Some("standard-2026"));
}

#[test]
fn the_embedded_catalog_defines_the_shipped_editions_and_the_lenient_reading_cannot_edit_them() {
    let (mut registry, mut constants) = embedded_catalog();
    assert_eq!(registry.atomic_weight_tables[STANDARD].len(), 118);
    assert_eq!(registry.atomic_weight_tables[ABRIDGED].len(), 118);
    // the working-directory conf is read leniently: the section is reported (stderr) and ignored
    let before = registry.clone();
    physure_core::units::conf::parse_physure_conf(
        "[AtomicWeights:standard-2024]\nH = 1.0, 0.1\n[AtomicWeights:standard-2026]\nH = 1.0, 0.1\n",
        &mut registry,
        &mut constants,
    );
    assert_eq!(registry, before);
}

// (h) the last selection wins; the aliases

#[test]
fn the_last_selection_wins() {
    let registry = registry_with(&["[Chemistry]\natomic_weights = abridged\n[Chemistry]\natomic_weights = standard\n"]);
    assert_eq!(registry.atomic_weight_edition(), STANDARD);
    let registry = registry_with(&[&select("abridged"), &select("standard-2024")]);
    assert_eq!(registry.atomic_weight_edition(), STANDARD);
    let registry = registry_with(&[&select("standard"), &select("abridged")]);
    assert_eq!(registry.atomic_weight_edition(), ABRIDGED);
    assert_abridged_values(&registry);
}

#[test]
fn a_text_without_a_chemistry_section_leaves_the_selection_alone() {
    let registry = registry_with(&[&select("abridged"), "[AtomicWeights]\nFe = 56.0, 0.3\n"]);
    assert_eq!(registry.atomic_weight_edition(), format!("abridged-2024 + {FE_56}"));
}

// (i) the report identifies the definitions, not just their number

#[test]
fn the_report_tells_two_sets_of_definitions_apart_and_the_same_set_in_any_order_alike() {
    let report = |text: &str| registry_with(&[text]).atomic_weight_edition();
    // same count, different symbol / weight / uncertainty / kind of number: all different
    let all = [
        report("[AtomicWeights]\nFe = 56.0, 0.3\n"),
        report("[AtomicWeights]\nFe = 56.0, 0.4\n"),
        report("[AtomicWeights]\nFe = 56.1, 0.3\n"),
        report("[AtomicWeights]\nCo = 56.0, 0.3\n"),
        report("[AtomicWeights]\nFe = 56.0, 0.3, standard\n"),
        report("[AtomicWeights]\nH = 1.5, 0.3, standard\n"),
        report("[AtomicWeights]\nH = 1.5, 0.4, standard\n"),
        report("[AtomicWeights]\nFe = 56.0, 0.3\nCo = 59.0, 0.2\n"),
    ];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b);
        }
        assert!(a.starts_with("standard-2024 + ") && a.contains(" definitions (fnv1a:") && a.ends_with(')'), "{a}");
    }
    // `H = 1.5, 0.3, standard` and `H = 1.5, 0.4, standard`: hashed over the bits of the stored numbers
    assert_eq!(all[5], "standard-2024 + 1 definitions (fnv1a:6ad53743530cd794)");
    assert_eq!(all[6], "standard-2024 + 1 definitions (fnv1a:93696f4a450c59a2)");
    // the order of the lines, and the text they come in, do not change it; a repeated symbol is the last one
    let a = report("[AtomicWeights]\nFe = 57.0, 0.7\nCo = 59.0, 0.2\n");
    assert_eq!(a, "standard-2024 + 2 definitions (fnv1a:3fccc6fb20553b8a)");
    assert_eq!(a, report("[AtomicWeights]\nCo = 59.0, 0.2\nFe = 57.0, 0.7\n"));
    assert_eq!(a, report("[AtomicWeights]\nFe = 1.0, 9.0\nCo = 59.0, 0.2\nFe = 57.0, 0.7\n"));
    assert_eq!(a, registry_with(&["[AtomicWeights]\nFe = 57.0, 0.7\n", "[AtomicWeights]\nCo = 59.0, 0.2\n"]).atomic_weight_edition());
}

#[test]
fn the_report_of_an_undefined_edition_says_so_next_to_the_definitions() {
    let registry = registry_with(&[&select("nope-1"), "[AtomicWeights]\nFe = 56.0, 0.3\n"]);
    assert_eq!(registry.atomic_weight_edition(), format!("nope-1 (undefined) + {FE_56}"));
}

#[test]
fn the_aliases_resolve_to_the_newest_shipped_edition_and_a_name_is_never_rewritten() {
    assert_eq!(resolve_atomic_weight_edition("standard"), "standard-2024");
    assert_eq!(resolve_atomic_weight_edition("abridged"), "abridged-2024");
    assert_eq!(resolve_atomic_weight_edition("acme-1"), "acme-1");
    assert_eq!(resolve_atomic_weight_edition("standard-2024"), "standard-2024");
    assert_eq!(resolve_atomic_weight_edition("abridged-2024"), "abridged-2024");
}
