//! The coverage table of the configuration freeze, `docs/conf_coverage.md`, is part of the guarantee (spec §9): it says
//! what a frozen file governs and what it does not. This test keeps it honest against the code, so the page cannot
//! name a key the catalog does not ship, a witness the battery does not have, or leave out a setting the core reads.
//!
//! # The format (strict; the page says so too)
//!
//! * Every Markdown table of the page is a coverage table: a line that starts with `|` is either the header
//!   `| Item | Where | Status | Covered by | Owner |`, the separator `|---|---|---|---|---|` right after it, or a row
//!   of exactly five cells. No cell may contain `|`.
//! * **Status** is one of `frozen` (the frozen file holds it, the seal covers it), `pinned+witnessed` (in the engine:
//!   the engine pin identifies it and at least one built-in witness exercises it), `pinned` (in the engine, no
//!   witness) and `not frozen`.
//! * **Item** of a `frozen` row starts with a code span naming what the shipped catalog holds: `` `[Section]` `` or
//!   `` `[Section] key` `` (the key as written before `=`).
//! * **Covered by** names witnesses as code spans `` `builtin.<id>` ``; a `pinned+witnessed` row names at least one,
//!   and a `pinned` row says exactly `none`.
//! * **Owner** of a `not frozen` row names who moves it and when (not `-`).
//!
//! # What is checked
//!
//! (a) every catalog reference of a `frozen` row exists in the shipped `physure.conf`, and every section the shipped
//! catalog has is named by a `frozen` row; (b) every `builtin.<id>` code span anywhere on the page is a case of
//! `battery()`; (c) every key of `CORE_SETTING_KEYS` has a `frozen` row `` `[Settings] <key>` ``. A failure lists every
//! problem, each naming the row (line of the page) or the missing row.

use std::collections::{BTreeMap, BTreeSet};

use physure_core::frozen::battery;
use physure_core::units::conf::{CORE_SETTING_KEYS, DEFAULT_PHYSURE_CONF};

const DOC: &str = include_str!("../../docs/conf_coverage.md");
const HEADER: &str = "| Item | Where | Status | Covered by | Owner |";
const SEPARATOR: &str = "|---|---|---|---|---|";
const STATUSES: [&str; 4] = ["frozen", "pinned+witnessed", "pinned", "not frozen"];

#[derive(Debug)]
struct Row {
    /// 1-based line of the page.
    line: usize,
    item: String,
    status: String,
    covered: String,
    owner: String,
}

/// The rows of every coverage table of `doc`, or the lines that break the format.
fn rows(doc: &str) -> Result<Vec<Row>, Vec<String>> {
    let (mut rows, mut problems) = (Vec::new(), Vec::new());
    let lines: Vec<&str> = doc.lines().map(str::trim).collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if !line.starts_with('|') {
            i += 1;
            continue;
        }
        if line != HEADER {
            problems.push(format!("line {}: a table outside the format (header must be `{HEADER}`): {line}", i + 1));
            while i < lines.len() && lines[i].starts_with('|') {
                i += 1;
            }
            continue;
        }
        if lines.get(i + 1) != Some(&SEPARATOR) {
            problems.push(format!("line {}: the header must be followed by `{SEPARATOR}`", i + 2));
        }
        i += 2;
        while i < lines.len() && lines[i].starts_with('|') {
            let cells: Vec<String> = lines[i]
                .strip_prefix('|')
                .and_then(|l| l.strip_suffix('|'))
                .unwrap_or("")
                .split('|')
                .map(|c| c.trim().to_string())
                .collect();
            if cells.len() != 5 {
                problems.push(format!("line {}: {} cells, a row has 5 (no `|` inside a cell)", i + 1, cells.len()));
            } else {
                let mut cells = cells.into_iter();
                let mut next = || cells.next().unwrap_or_default();
                let item = next();
                let _where = next();
                rows.push(Row { line: i + 1, item, status: next(), covered: next(), owner: next() });
            }
            i += 1;
        }
    }
    if problems.is_empty() {
        Ok(rows)
    } else {
        Err(problems)
    }
}

/// The text of the code spans of `text`, in order.
fn code_spans(text: &str) -> Vec<&str> {
    text.split('`').skip(1).step_by(2).collect()
}

/// The sections of the shipped catalog, each with the keys its lines define (the text before `=`).
fn shipped_catalog() -> BTreeMap<String, BTreeSet<String>> {
    let mut sections: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current = String::new();
    for line in DEFAULT_PHYSURE_CONF.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].to_string();
            sections.entry(current.clone()).or_default();
        } else if let Some((key, _)) = line.split_once('=') {
            sections.entry(current.clone()).or_default().insert(key.trim().to_string());
        }
    }
    sections
}

/// A catalog reference `[Section]` or `[Section] key` as (section, key).
fn catalog_reference(span: &str) -> Option<(&str, Option<&str>)> {
    let rest = span.strip_prefix('[')?;
    let (section, key) = rest.split_once(']')?;
    let key = key.trim();
    Some((section, (!key.is_empty()).then_some(key)))
}

/// Everything wrong with the page `doc` against the shipped catalog, the battery and the setting keys.
fn problems(doc: &str) -> Vec<String> {
    let rows = match rows(doc) {
        Ok(rows) => rows,
        Err(problems) => return problems,
    };
    let catalog = shipped_catalog();
    let witnesses: BTreeSet<&str> = battery().iter().map(|case| case.id).collect();
    let mut problems = Vec::new();
    let mut frozen_references: BTreeSet<(String, Option<String>)> = BTreeSet::new();
    for row in &rows {
        let at = format!("line {} ({})", row.line, row.item);
        if !STATUSES.contains(&row.status.as_str()) {
            problems.push(format!("{at}: status `{}` is not one of {STATUSES:?}", row.status));
        }
        let named_witnesses = code_spans(&row.covered).iter().filter(|s| s.starts_with("builtin.")).count();
        match row.status.as_str() {
            "frozen" => match code_spans(&row.item).first().and_then(|span| catalog_reference(span)) {
                None => problems.push(format!("{at}: a frozen row starts with `[Section]` or `[Section] key`")),
                Some((section, key)) => {
                    match (catalog.get(section), key) {
                        (None, _) => problems.push(format!("{at}: the shipped physure.conf has no section [{section}]")),
                        (Some(keys), Some(key)) if !keys.contains(key) => {
                            problems.push(format!("{at}: the shipped physure.conf has no key `{key}` in [{section}]"))
                        }
                        _ => {}
                    }
                    frozen_references.insert((section.to_string(), key.map(str::to_string)));
                }
            },
            "pinned+witnessed" if named_witnesses == 0 => {
                problems.push(format!("{at}: pinned+witnessed names no `builtin.<id>` witness (status `pinned` if none)"))
            }
            "pinned" if row.covered != "none" => {
                problems.push(format!("{at}: a pinned row has no witness, so Covered by says exactly `none`"))
            }
            "not frozen" if row.owner.is_empty() || row.owner == "-" => {
                problems.push(format!("{at}: a not-frozen row names its owner and phase"))
            }
            _ => {}
        }
    }
    // (b) anywhere on the page, not only in tables
    for span in code_spans(doc) {
        if let Some(id) = span.strip_prefix("builtin.") {
            if !witnesses.contains(id) {
                problems.push(format!("`{span}` is not a case of the built-in battery"));
            }
        }
    }
    // (a) every shipped section is accounted for
    for section in catalog.keys() {
        if !frozen_references.iter().any(|(s, _)| s == section) {
            problems.push(format!("missing row: a frozen row for the shipped section `[{section}]`"));
        }
    }
    // (c) every setting the core reads
    for key in CORE_SETTING_KEYS {
        if !frozen_references.contains(&("Settings".to_string(), Some(key.to_string()))) {
            problems.push(format!("missing row: `[Settings] {key}` (status frozen), saying what the setting governs"));
        }
    }
    problems
}

#[test]
fn the_coverage_table_agrees_with_the_catalog_the_battery_and_the_settings() {
    let found = problems(DOC);
    assert!(found.is_empty(), "docs/conf_coverage.md:\n  {}", found.join("\n  "));
}

#[test]
fn the_page_has_every_kind_of_row() {
    let rows = rows(DOC).unwrap_or_default();
    for status in STATUSES {
        assert!(rows.iter().any(|r| r.status == status), "no `{status}` row");
    }
    assert!(rows.len() >= CORE_SETTING_KEYS.len() + 40, "{} rows", rows.len());
}

/// The checks fire: a page with a fake key, an unknown witness, a dropped setting row and a bad status is refused,
/// each problem named (so a green run of the real page is not a vacuous one).
#[test]
fn the_checks_are_not_vacuous() {
    let doc = format!(
        "{HEADER}\n{SEPARATOR}\n\
         | `[Settings] no_such_key` | x | frozen | - | - |\n\
         | `[NoSuchSection]` | x | frozen | - | - |\n\
         | thing | x | pinned+witnessed | `builtin.no.such_case` | - |\n\
         | other | x | pinned | `builtin.units.foot_to_metre` | - |\n\
         | open | x | not frozen | none | - |\n\
         | odd | x | maybe | none | - |\n"
    );
    let found = problems(&doc).join("\n");
    for expected in [
        "no key `no_such_key` in [Settings]",
        "no section [NoSuchSection]",
        "`builtin.no.such_case` is not a case",
        "a pinned row has no witness",
        "names its owner and phase",
        "status `maybe`",
        "missing row: `[Settings] propagation_mode`",
        "missing row: a frozen row for the shipped section `[Units]`",
    ] {
        assert!(found.contains(expected), "expected {expected:?} in:\n{found}");
    }
    let broken = format!("| A | B |\n|---|---|\n| 1 | 2 |\n\n{HEADER}\n{SEPARATOR}\n| a | b | frozen |\n");
    let found = problems(&broken).join("\n");
    assert!(found.contains("a table outside the format") && found.contains("3 cells"), "{found}");
}
