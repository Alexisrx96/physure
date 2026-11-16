//! Standard thermochemical data as catalog data (Phase 4b, Task 3): the sections `[Thermochemistry:<edition>]`, the
//! selector `[Chemistry] thermochemistry`, your definitions in `[Thermochemistry]`, and the readers
//! `chemistry::standard_enthalpy` / `chemistry::standard_entropy` that go through the catalog in scope.
//!
//! The shipped edition `legacy-python-1` is held to the table of the Python package bit for bit: the test reads the
//! literals of `physure-python/physure/ext/chemistry/thermo_kinetics.py` itself.

use physure_core::chemistry::{standard_enthalpy, standard_entropy};
use physure_core::units::conf::{build_registry_with, extend_registry};
use physure_core::units::{with_catalog, UnitRegistry};
use std::collections::HashMap;
use std::path::PathBuf;

/// The shipped catalog and nothing else (a `physure.conf` of the working directory must not change these tests).
fn shipped() -> UnitRegistry {
    build_registry_with("").unwrap().0
}

/// The shipped catalog extended strictly with `text`.
fn with_text(text: &str) -> Result<UnitRegistry, Vec<String>> {
    let (mut registry, mut constants): (UnitRegistry, HashMap<String, String>) = build_registry_with("").unwrap();
    extend_registry(&mut registry, &mut constants, text)?;
    Ok(registry)
}

/// The `"species": number,` lines of the dict `name` in the Python file, as written.
fn python_table(name: &str) -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../physure-python/physure/ext/chemistry/thermo_kinetics.py");
    let text = std::fs::read_to_string(&path).unwrap();
    let start = text.find(&format!("{name}: dict[str, float] = {{")).unwrap_or_else(|| panic!("{name} is gone from {path:?}"));
    let body = &text[start..];
    let body = &body[body.find('{').unwrap() + 1..body.find('}').unwrap()];
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (key, value) = line.trim_end_matches(',').split_once(':').unwrap();
            (key.trim().trim_matches('"').to_string(), value.trim().to_string())
        })
        .collect()
}

#[test]
fn the_shipped_edition_is_the_table_of_the_python_package_bit_for_bit() {
    let registry = shipped();
    let enthalpies = python_table("STANDARD_ENTHALPY_FORMATION");
    let entropies = python_table("STANDARD_ENTROPY");
    assert_eq!(enthalpies.len(), 8, "{enthalpies:?}");
    assert_eq!(entropies.len(), 8, "{entropies:?}");
    with_catalog(&registry, || {
        for (species, literal) in &enthalpies {
            let q = standard_enthalpy(species).unwrap();
            let (value, u, unit) = q.display_parts();
            assert_eq!(value.to_bits(), literal.parse::<f64>().unwrap().to_bits(), "{species}: {value} vs {literal}");
            assert_eq!((u, unit.as_str()), (0.0, "kJ/mol"), "{species}");
        }
        for (species, literal) in &entropies {
            let q = standard_entropy(species).unwrap();
            let (value, u, unit) = q.display_parts();
            assert_eq!(value.to_bits(), literal.parse::<f64>().unwrap().to_bits(), "{species}: {value} vs {literal}");
            assert_eq!((u, unit.as_str()), (0.0, "J/(mol*K)"), "{species}");
        }
    });
    // the edition carries each species with its phase, and nothing else
    let table = &registry.thermochemistry_tables["legacy-python-1"];
    let keys: Vec<&str> = table.keys().map(String::as_str).collect();
    assert_eq!(keys, ["CH4(g)", "CO2(g)", "H2(g)", "H2O(l)", "N2(g)", "NH3(g)", "NaCl(cr)", "O2(g)"]);
    assert_eq!(registry.thermochemistry_selection, "legacy-python-1");
    assert!(registry.thermochemistry.is_empty(), "the shipped catalog defines nothing in [Thermochemistry]");
}

#[test]
fn a_species_is_read_with_or_without_its_phase_and_in_subscripts() {
    with_catalog(&shipped(), || {
        let unqualified = standard_enthalpy("H2O").unwrap();
        assert_eq!(standard_enthalpy("H2O(l)").unwrap().display_parts(), unqualified.display_parts());
        assert_eq!(standard_entropy("H₂O(l)").unwrap().display_parts().0, 69.9);
        assert_eq!(standard_entropy("NaCl").unwrap().display_parts().0, 72.1);
        // the quantities carry their units and convert
        assert_eq!(unqualified.to("J/mol").unwrap().display_parts().0, -285_800.0);
    });
}

#[test]
fn a_species_or_phase_the_edition_lacks_is_an_error_naming_the_edition_and_the_phases_it_has() {
    with_catalog(&shipped(), || {
        let message = standard_enthalpy("XeF4").unwrap_err().to_string();
        assert!(message.contains("XeF4") && message.contains("legacy-python-1"), "{message}");
        let message = standard_entropy("H2O(g)").unwrap_err().to_string();
        assert!(message.contains("H2O(g)") && message.contains("H2O(l)") && message.contains("legacy-python-1"), "{message}");
        // a malformed species is an error, not a miss
        for bad in ["h2o", "H2O(x)", "H2O()", "", "H2O (l)", "(l)", "H0"] {
            assert!(standard_enthalpy(bad).is_err(), "{bad:?}");
        }
    });
}

#[test]
fn an_unqualified_name_present_in_two_phases_is_ambiguous_and_the_error_names_them() {
    let registry = with_text(
        "[Thermochemistry:mine-1]\nH2O(l) = -285.83, 69.95\nH2O(g) = -241.826, 188.835\nCa(OH)2(s) = -985.2, 83.4\n[Chemistry]\nthermochemistry = mine-1\n",
    )
    .unwrap();
    with_catalog(&registry, || {
        let message = standard_enthalpy("H2O").unwrap_err().to_string();
        assert!(message.contains("H2O(g)") && message.contains("H2O(l)") && message.contains("mine-1"), "{message}");
        assert_eq!(standard_enthalpy("H2O(g)").unwrap().display_parts().0, -241.826);
        // the phase is read before the formula: `Ca(OH)2(s)` is calcium hydroxide, solid
        assert_eq!(standard_enthalpy("Ca(OH)2").unwrap().display_parts().0, -985.2);
        assert_eq!(standard_entropy("Ca(OH)2(s)").unwrap().display_parts().0, 83.4);
        // a table of your own is complete: nothing comes from the shipped edition
        assert!(standard_enthalpy("CO2").unwrap_err().to_string().contains("mine-1"));
    });
}

#[test]
fn a_definition_without_a_phase_is_the_unique_phase_of_the_edition_and_wins_over_it() {
    let registry = with_text("[Thermochemistry]\nH2O = -280.0, 70.5\nH2O2(l) = -187.8, 109.6\n").unwrap();
    with_catalog(&registry, || {
        assert_eq!(standard_enthalpy("H2O(l)").unwrap().display_parts().0, -280.0);
        assert_eq!(standard_entropy("H2O").unwrap().display_parts().0, 70.5);
        // a species the edition does not have is added with its phase
        assert_eq!(standard_enthalpy("H2O2").unwrap().display_parts().0, -187.8);
        // the rest of the edition is untouched
        assert_eq!(standard_enthalpy("CO2").unwrap().display_parts().0, -393.5);
    });
    with_catalog(&shipped(), || assert_eq!(standard_enthalpy("H2O").unwrap().display_parts().0, -285.8));
}

#[test]
fn a_definition_that_cannot_be_placed_is_an_error_when_the_data_are_read() {
    let two_phases = "[Thermochemistry:mine-2]\nH2O(l) = -285.83, 69.95\nH2O(g) = -241.826, 188.835\n[Chemistry]\nthermochemistry = mine-2\n";
    for (text, expected) in [
        // ambiguous: the edition has two phases of water
        (format!("{two_phases}[Thermochemistry]\nH2O = -280.0, 70.0\n"), "H2O(g)"),
        // a species the edition does not have, without a phase: where would it go?
        ("[Thermochemistry]\nXeF4 = -251.0, 316.0\n".to_string(), "XeF4"),
        // two spellings of one species
        ("[Thermochemistry]\nH2O = -280.0, 70.0\nH2O(l) = -281.0, 70.0\n".to_string(), "H2O(l)"),
    ] {
        // each text reads (the selection may come from another text), the data do not
        let registry = with_text(&text).unwrap();
        with_catalog(&registry, || {
            let message = standard_enthalpy("N2").unwrap_err().to_string();
            assert!(message.contains(expected) && message.contains("[Thermochemistry]"), "{text}: {message}");
        });
        assert!(registry.thermochemistry_problem().is_some(), "{text}");
    }
    assert_eq!(shipped().thermochemistry_problem(), None);
}

#[test]
fn a_strict_reading_refuses_a_bad_line_a_bad_edition_and_a_duplicate() {
    for (text, expected) in [
        ("[Thermochemistry]\nh2o(l) = -285.8, 69.9\n", "h2o(l)"),
        ("[Thermochemistry]\nH2O(x) = -285.8, 69.9\n", "phase"),
        ("[Thermochemistry]\nH2O(l) = abc, 69.9\n", "abc"),
        ("[Thermochemistry]\nH2O(l) = -285.8\n", "enthalpy, entropy"),
        ("[Thermochemistry]\nH2O(l) = -285.8, 69.9, 1\n", "enthalpy, entropy"),
        ("[Thermochemistry]\nH2O(l) = inf, 69.9\n", "finite"),
        ("[Thermochemistry]\nH2O(l) = -285.8, 69.9\nH2O(l) = -285.0, 69.9\n", "second time"),
        ("[Thermochemistry:mine-1]\nH2O = -285.8, 69.9\n", "phase"),
        ("[Thermochemistry:mine-1]\nH2O(l) = -285.8, 69.9\nH2O(l) = -285.8, 69.9\n", "second time"),
        ("[Thermochemistry:legacy-python-1]\nH2O(l) = -1.0, 1.0\n", "frozen"),
        ("[Thermochemistry:legacy-python-2]\nH2O(l) = -1.0, 1.0\n", "reserved"),
        ("[Thermochemistry:standard-1]\nH2O(l) = -1.0, 1.0\n", "reserved"),
        ("[Thermochemistry:Mine]\nH2O(l) = -1.0, 1.0\n", "edition name"),
        ("[Chemistry]\nthermochemistry = Not A Name\n", "edition name"),
        ("[Chemistry]\nthermo = mine\n", "thermochemistry"),
        ("[thermochemistry]\nH2O(l) = -1.0, 1.0\n", "[Thermochemistry]"),
        ("[Thermochemistry :mine-1]\nH2O(l) = -1.0, 1.0\n", "[Thermochemistry:mine-1]"),
    ] {
        let problems = with_text(text).unwrap_err();
        assert!(problems.iter().any(|p| p.contains(expected)), "{text}: {problems:?}");
    }
    // `legacy-` is reserved for the atomic weights too: one rule for the names of the editions of this library
    let problems = with_text("[AtomicWeights:legacy-1]\nH = 1.0, 0\n").unwrap_err();
    assert!(problems.iter().any(|p| p.contains("reserved")), "{problems:?}");
    // a selection and its table, in either order
    with_text("[Chemistry]\nthermochemistry = mine-1\n[Thermochemistry:mine-1]\nH2O(l) = -1.0, 1.0\n").unwrap();
}

#[test]
fn the_working_directory_conf_is_read_leniently_and_the_rest_of_it_applies() {
    let (mut registry, mut constants) = build_registry_with("").unwrap();
    physure_core::units::conf::parse_physure_conf(
        "[Thermochemistry]\nH2O(l) = abc, 1\nCO2 = -393.0, 213.0\n",
        &mut registry,
        &mut constants,
    );
    assert_eq!(registry.thermochemistry.get("CO2"), Some(&(-393.0, 213.0)));
    assert!(!registry.thermochemistry.contains_key("H2O(l)"));
    with_catalog(&registry, || assert_eq!(standard_enthalpy("CO2(g)").unwrap().display_parts().0, -393.0));
}

#[test]
fn a_catalog_without_thermochemistry_reads_and_says_so_when_the_data_are_asked_for() {
    // a catalog frozen before the data moved into it has no edition and no selection
    let mut registry = UnitRegistry::new();
    let mut constants = HashMap::new();
    extend_registry(&mut registry, &mut constants, "[Units]\nm = 1.0, L, [m]\n").unwrap();
    assert_eq!(registry.thermochemistry_selection, "");
    assert_eq!(registry.thermochemistry_problem(), None);
    with_catalog(&registry, || {
        let message = standard_enthalpy("H2O").unwrap_err().to_string();
        assert!(message.contains("[Chemistry] thermochemistry"), "{message}");
    });
    // a selection with no table is an error naming the edition
    let registry = with_text("[Chemistry]\nthermochemistry = nope-1\n").unwrap();
    with_catalog(&registry, || assert!(standard_enthalpy("H2O").unwrap_err().to_string().contains("nope-1")));
    assert!(registry.thermochemistry_problem().unwrap().contains("nope-1"));
}
