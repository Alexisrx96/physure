//! Which source of a frozen file set each entry of its catalog, and the `section.key` names entries are shown and
//! selected under (`phs conf show [SELECTOR] [--origin]`).
//!
//! The origins come from the one replay of `load` ([`replay_file_with`]): after each source, the entries of the
//! catalog so far are compared with those before it. An entry whose value a source changes (or adds) takes that
//! source's origin; an entry it leaves unchanged keeps the earlier one. A source can also *restate* a value, setting
//! it to what it already was: the entries cannot see that, so a line `key = ...` of the source under the matching
//! section is what tells it (see [`Origin::restated_by`]).

use std::collections::{HashMap, HashSet};
use std::fmt;

use super::entries::{entries, Entry};
use super::load::replay_file_with;
use super::model::FrozenFile;

/// Where an entry's value comes from.
///
/// `source` is the origin of the source that last **changed** the value (`embedded`, `overrides`, or a further
/// source's origin). `restated_by` is the last later source that sets the entry to the value it already had (a line
/// for that `section.key` in it, with the catalog unchanged); `None` when no later source mentions it. Shown as
/// `embedded` or `embedded (restated by overrides)`: the value is the shipped one, and the overrides say it too, so
/// removing the line from the overrides changes nothing, while a later upgrade of the embedded value would be masked
/// by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub source: String,
    pub restated_by: Option<String>,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.restated_by {
            None => f.write_str(&self.source),
            Some(later) => write!(f, "{} (restated by {later})", self.source),
        }
    }
}

/// The kinds of entries shown under `units`.
const UNIT_KINDS: [&str; 7] = ["unit", "base_unit", "derived_unit", "alias", "unit_meta", "unit_names", "no_prefix"];

/// The `section.key` an entry is shown under: the units and their kinds (`unit`, `base_unit`, `derived_unit`, `alias`,
/// `unit_meta`, `unit_names`, `no_prefix`) under `units`, `constant` under `constants`, `atomic_weight*` under
/// `atomic_weights`, `setting*` under `settings`, `category` under `categories`, each other kind under its own name.
/// The key is the entry's name (`edition/symbol` for the tables of atomic weights).
pub fn section_and_key(entry: &Entry) -> (String, String) {
    let section = match entry.kind {
        kind if UNIT_KINDS.contains(&kind) => "units",
        "constant" => "constants",
        kind if kind.starts_with("atomic_weight") => "atomic_weights",
        kind if kind.starts_with("setting") => "settings",
        "category" => "categories",
        kind => kind,
    };
    (section.to_string(), entry.name.clone())
}

/// The entries whose `section`, whose `section.key`, or whose key prefix (`section.key/...`, as in
/// `atomic_weights.standard-2024`) equals `selector`; `""` selects all. Nothing matching is an empty list.
pub fn select<'a, T>(entries: &'a [(Entry, T)], selector: &str) -> Vec<&'a (Entry, T)> {
    entries
        .iter()
        .filter(|(entry, _)| {
            if selector.is_empty() {
                return true;
            }
            let (section, key) = section_and_key(entry);
            let named = format!("{section}.{key}");
            selector == section || selector == named || named.strip_prefix(selector).is_some_and(|rest| rest.starts_with('/'))
        })
        .collect()
}

/// The `section.key` names a source's text sets, by its `key = value` lines: what tells a restated value.
fn declared(text: &str) -> HashSet<(String, String)> {
    let mut out = HashSet::new();
    let mut section = "";
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((key, _)) = line.split_once('=') else { continue };
        let key = key.trim().to_string();
        let shown: &[&str] = match section {
            "Units" => &["units"],
            "Constants" => &["constants"],
            "Settings" => &["settings"],
            "Categories" => &["categories"],
            "AtomicWeights" | "Chemistry" => &["atomic_weights"],
            "Prefixes" => &["prefix", "prefix_exact", "prefix_name"],
            table if table.starts_with("AtomicWeights:") => {
                out.insert(("atomic_weights".to_string(), format!("{}/{key}", &table["AtomicWeights:".len()..])));
                continue;
            }
            _ => &[],
        };
        for name in shown {
            out.insert((name.to_string(), key.clone()));
        }
    }
    out
}

/// Every entry of the catalog the file's sources build, in the order of [`entries`], with its [`Origin`]. Replays the
/// sources as they are, strictly (the replay of `load`): the caller decides whether a file that does not verify may
/// be replayed at all (the commands show only the configuration in force, which verified). `Err` is the origin of the
/// source that does not replay (or "the sources together") and its problems.
pub fn entries_with_origin(file: &FrozenFile) -> Result<Vec<(Entry, Origin)>, (String, Vec<String>)> {
    let mut known: HashMap<(&'static str, String), (String, Origin)> = HashMap::new();
    let mut result: Vec<(Entry, Origin)> = Vec::new();
    replay_file_with(file, |index, registry, constants, collected| {
        let source = file.sources()[index].origin();
        let restating = if index == 0 { HashSet::new() } else { declared(file.sources()[index].text()) };
        let now = entries(registry, constants, collected);
        let mut next = HashMap::with_capacity(now.len());
        result = Vec::with_capacity(now.len());
        for entry in now {
            let key = (entry.kind, entry.name.clone());
            let origin = match known.get(&key) {
                Some((value, origin)) if *value == entry.value => {
                    let mut origin = origin.clone();
                    if restating.contains(&section_and_key(&entry)) {
                        origin.restated_by = Some(source.to_string());
                    }
                    origin
                }
                _ => Origin { source: source.to_string(), restated_by: None },
            };
            next.insert(key, (entry.value.clone(), origin.clone()));
            result.push((entry, origin));
        }
        known = next;
    })?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_names_follow_the_sections_of_a_conf() {
        let text = "# c\n[Units]\nzz = 2.5, L\n[Constants]\n c1 = 1 m \n[AtomicWeights:acme-1]\nH = 1.0, 0\n[Chemistry]\natomic_weights = acme-1\n[Prefixes]\nzk = 1000\n[Compatibility]\nradian, plain = convert\nnot a pair\n";
        let seen = declared(text);
        for (section, key) in [
            ("units", "zz"),
            ("constants", "c1"),
            ("atomic_weights", "acme-1/H"),
            ("atomic_weights", "atomic_weights"),
            ("prefix", "zk"),
            ("prefix_exact", "zk"),
        ] {
            assert!(seen.contains(&(section.to_string(), key.to_string())), "{section}.{key}: {seen:?}");
        }
        assert_eq!(seen.len(), 7, "{seen:?}");
    }

    #[test]
    fn an_origin_reads_as_its_source_and_any_restatement() {
        assert_eq!(Origin { source: "overrides".into(), restated_by: None }.to_string(), "overrides");
        assert_eq!(
            Origin { source: "embedded".into(), restated_by: Some("overrides".into()) }.to_string(),
            "embedded (restated by overrides)"
        );
    }
}
