//! The conf examples of `docs/atomic_weights.md` are executable: every fenced block marked ```ini is a text meant to
//! be pasted into a conf of your own or a unit system, so it must be read by the strict reading
//! (`extend_registry`) without a problem. A line with a trailing `# comment`, a name the library reserves or a
//! misspelt header would fail here, which keeps the page honest. Blocks that only show lines of the shipped tables
//! (which a text of yours cannot define) are marked ```text and are not run.

use physure_core::units::conf::{build_registry_with, extend_registry};

const DOC: &str = include_str!("../../docs/atomic_weights.md");

/// The bodies of the fenced blocks marked ```ini, in order (the fence may be indented, inside a list item).
fn ini_blocks() -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in DOC.lines() {
        let fence = line.trim();
        match (&mut current, fence) {
            (None, "```ini") => current = Some(Vec::new()),
            (Some(body), "```") => {
                blocks.push(body.join("\n") + "\n");
                current = None;
            }
            (Some(body), _) => body.push(line),
            (None, _) => {}
        }
    }
    assert!(current.is_none(), "an ```ini block is not closed");
    blocks
}

#[test]
fn the_page_has_its_conf_examples_marked() {
    // 5.3 (the selector), 5.4 (your definitions), 6 (a table of your own), 8 (the three forms of a line)
    assert!(ini_blocks().len() >= 4, "{} blocks", ini_blocks().len());
}

#[test]
fn every_conf_example_of_the_page_is_read_without_a_problem() {
    for (i, block) in ini_blocks().iter().enumerate() {
        let (mut registry, mut constants) = build_registry_with("").unwrap();
        let outcome = extend_registry(&mut registry, &mut constants, block);
        assert!(outcome.is_ok(), "block {i} of docs/atomic_weights.md is a problem: {:?}\n{block}", outcome.err());
    }
}

#[test]
fn the_example_of_your_definitions_does_what_the_text_says() {
    // 5.4: "Here Fe is 56.0 with 0.1732 under abridged-2024"
    let block = ini_blocks().into_iter().find(|b| b.contains("Fe = 56.0, 0.3") && b.contains("[Chemistry]")).unwrap();
    let (mut registry, mut constants) = build_registry_with("").unwrap();
    extend_registry(&mut registry, &mut constants, &block).unwrap();
    let (weight, std) = registry.atomic_weight("Fe").unwrap();
    assert_eq!(weight, 56.0);
    assert!((std - 0.1732).abs() < 5e-5, "{std}");
    assert!(registry.atomic_weight_edition().starts_with("abridged-2024 + 1 definitions"), "{}", registry.atomic_weight_edition());
}
