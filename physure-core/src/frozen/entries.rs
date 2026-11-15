//! Everything a catalog determines, as a sorted list of entries, and the semantic difference of two such lists
//! (spec §8, `phs conf diff` and `phs conf show`).
//!
//! A line diff of two frozen files says which text changed; this says which *meaning* changed: the unit whose scale
//! moved, the constant whose value did, the atomic weight of an edition, a setting. Completeness is the point:
//! if two catalogs differ in anything the registry holds, their entries differ. [`entries`] destructures
//! `UnitRegistry` without `..`, so a field added to the registry does not compile until this module accounts for
//! it, and a test mutates every field and checks that the entries notice.

use std::collections::{BTreeMap, HashMap};

use super::load::FrozenCatalog;
use crate::units::conf::{replay_source, EffectiveSettings, ProcessSettings, DEFAULT_PHYSURE_CONF};
use crate::units::registry::{ConstantMetadata, UnitMetadata};
use crate::units::UnitRegistry;

/// One thing a catalog says (to be compared between catalogs built by **the same engine**: a unit's value includes
/// its `id`, a hash that Rust does not promise to keep across releases, so entries are not a file format): a `kind` (`unit_facts`, `constant`, `setting`, ...), the `name` it is about and its
/// `value`, exact (floating-point numbers as Rust prints them, which round-trips).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub kind: &'static str,
    pub name: String,
    pub value: String,
}

/// How an entry differs between two catalogs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Added(Entry),
    Removed(Entry),
    Changed { kind: &'static str, name: String, old: String, new: String },
}

impl Change {
    pub fn kind(&self) -> &'static str {
        match self {
            Change::Added(e) | Change::Removed(e) => e.kind,
            Change::Changed { kind, .. } => kind,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Change::Added(e) | Change::Removed(e) => &e.name,
            Change::Changed { name, .. } => name,
        }
    }
}

fn push(out: &mut Vec<Entry>, kind: &'static str, name: impl Into<String>, value: impl Into<String>) {
    out.push(Entry { kind, name: name.into(), value: value.into() });
}

/// Every entry of a catalog, sorted by kind and name. `settings` is what the sources gave
/// ([`FrozenCatalog::process_settings`]); each setting is listed with the value it ends up with, the compiled default
/// included, so a file that does not mention one differs from a file that sets it to another value.
pub fn entries(registry: &UnitRegistry, constants: &HashMap<String, String>, settings: &ProcessSettings) -> Vec<Entry> {
    // no `..`: a new field of the registry must be accounted for here
    let UnitRegistry {
        base_units,
        derived_units,
        aliases,
        prefixes,
        prefix_exact,
        prefix_names,
        symbol_aliases,
        base_dimension,
        dimension_base,
        recipes,
        recipe_aliases,
        no_prefix,
        unit_meta,
        categories,
        constants_meta,
        atomic_weights,
        atomic_weight_tables,
        atomic_weight_selection,
    } = registry;
    let mut out = Vec::new();
    for (name, unit) in base_units {
        push(&mut out, "base_unit", name, format!("{unit:?}"));
    }
    for (name, unit) in derived_units {
        push(&mut out, "derived_unit", name, format!("{unit:?}"));
    }
    for (name, target) in aliases {
        push(&mut out, "alias", name, target);
    }
    // what the registry answers for every name, which also follows aliases and recipes
    for name in registry.names() {
        let value = match registry.facts(&name) {
            Ok(facts) => format!("{:?}; {facts:?}", registry.get_unit(&name)),
            Err(e) => format!("{:?}; error {e}", registry.get_unit(&name)),
        };
        push(&mut out, "unit", name, value);
    }
    for (name, factor) in prefixes {
        push(&mut out, "prefix", name, format!("{factor:?}"));
    }
    for (name, exact) in prefix_exact {
        push(&mut out, "prefix_exact", name, format!("{exact:?}"));
    }
    for (name, symbol) in prefix_names {
        push(&mut out, "prefix_name", name, symbol);
    }
    for (symbol, names) in symbol_aliases {
        push(&mut out, "unit_names", symbol, format!("{names:?}"));
    }
    for (symbol, dimension) in base_dimension {
        push(&mut out, "base_dimension", symbol, dimension);
    }
    for (dimension, base) in dimension_base {
        push(&mut out, "dimension_base", dimension, base);
    }
    for name in recipes {
        push(&mut out, "recipe", name, "");
    }
    for (key, names) in recipe_aliases {
        push(&mut out, "recipe_names", format!("{key:?}"), format!("{names:?}"));
    }
    for name in no_prefix {
        push(&mut out, "no_prefix", name, "");
    }
    for (name, meta) in unit_meta {
        let UnitMetadata { category, latex_symbol, description } = meta;
        push(&mut out, "unit_meta", name, format!("{category:?} {latex_symbol:?} {description:?}"));
    }
    for (category, members) in categories {
        // in the order the catalog lists them: `get_category_units` hands it out as it is
        push(&mut out, "category", category, format!("{members:?}"));
    }
    for (name, value) in constants {
        push(&mut out, "constant", name, value);
    }
    for (name, meta) in constants_meta {
        let ConstantMetadata { value, description, latex_symbol } = meta;
        push(&mut out, "constant_meta", name, format!("{value} {description:?} {latex_symbol:?}"));
    }
    for (symbol, weight) in atomic_weights {
        push(&mut out, "atomic_weight", symbol, format!("{weight:?}"));
    }
    for (edition, table) in atomic_weight_tables {
        for (symbol, weight) in table {
            push(&mut out, "atomic_weight_table", format!("{edition}/{symbol}"), format!("{weight:?}"));
        }
        push(&mut out, "atomic_weight_edition", edition, table.len().to_string());
    }
    push(&mut out, "atomic_weight_selection", "atomic_weights", atomic_weight_selection);
    let EffectiveSettings {
        propagation_mode,
        angle_mode,
        parallel_threshold,
        max_loop_iterations,
        max_pipeline_steps,
        approx_rel_tol,
        monte_carlo_samples,
        max_while_iterations,
        radian_grant,
        steradian_grant,
        ignored,
    } = settings.effective();
    push(&mut out, "setting", "propagation_mode", propagation_mode.name());
    push(&mut out, "setting", "angle_mode", angle_mode.name());
    push(&mut out, "setting", "parallel_threshold", parallel_threshold.to_string());
    push(&mut out, "setting", "max_loop_iterations", max_loop_iterations.to_string());
    push(&mut out, "setting", "max_pipeline_steps", max_pipeline_steps.to_string());
    push(&mut out, "setting", "approx_rel_tol", format!("{approx_rel_tol:?}"));
    push(&mut out, "setting", "monte_carlo_samples", monte_carlo_samples.to_string());
    push(&mut out, "setting", "max_while_iterations", max_while_iterations.to_string());
    push(&mut out, "setting", "radian_plain", radian_grant.to_string());
    push(&mut out, "setting", "steradian_plain", steradian_grant.to_string());
    for (key, value) in &ignored {
        push(&mut out, "setting_ignored", key, value);
    }
    out.sort();
    out
}

impl FrozenCatalog {
    /// The entries of the catalog this file's sources build.
    pub fn entries(&self) -> Vec<Entry> {
        entries(&self.registry, &self.constants, &self.process_settings())
    }
}

/// The entries of the catalog this library ships (what `phs conf update` would freeze), built strictly as a frozen
/// file's first source is. `Err` carries the problems if the shipped text does not replay, which a test forbids.
pub fn embedded_entries() -> Result<Vec<Entry>, Vec<String>> {
    let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
    replay_source(&mut registry, &mut constants, &mut collected, DEFAULT_PHYSURE_CONF, true)?;
    Ok(entries(&registry, &constants, &collected))
}

/// What differs between two sorted entry lists (from [`entries`]): in order of kind and name.
pub fn diff(old: &[Entry], new: &[Entry]) -> Vec<Change> {
    // a key listed twice keeps every value (joined), so that nothing is dropped from a hand-built list
    fn group(entries: &[Entry]) -> BTreeMap<(&'static str, String), Vec<&str>> {
        let mut by: BTreeMap<(&'static str, String), Vec<&str>> = BTreeMap::new();
        for e in entries {
            by.entry((e.kind, e.name.clone())).or_default().push(&e.value);
        }
        by
    }
    let (old_by, new_by) = (group(old), group(new));
    let entry = |k: &(&'static str, String), values: &[&str]| Entry { kind: k.0, name: k.1.clone(), value: values.join(" | ") };
    let mut changes = Vec::new();
    for (k, was) in &old_by {
        match new_by.get(k) {
            None => changes.push(Change::Removed(entry(k, was))),
            Some(now) if now != was => {
                changes.push(Change::Changed { kind: k.0, name: k.1.clone(), old: was.join(" | "), new: now.join(" | ") })
            }
            Some(_) => {}
        }
    }
    for (k, now) in &new_by {
        if !old_by.contains_key(k) {
            changes.push(Change::Added(entry(k, now)));
        }
    }
    changes.sort_by(|a, b| (a.kind(), a.name()).cmp(&(b.kind(), b.name())));
    changes
}

/// One line for the history: `unit +2 -0 ~1; constant +0 -0 ~3` (added, removed, changed per kind), or `no
/// differences`.
pub fn summary(changes: &[Change]) -> String {
    let mut counts: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for change in changes {
        let c = counts.entry(change.kind()).or_default();
        match change {
            Change::Added(_) => c.0 += 1,
            Change::Removed(_) => c.1 += 1,
            Change::Changed { .. } => c.2 += 1,
        }
    }
    if counts.is_empty() {
        return "no differences".to_string();
    }
    counts.iter().map(|(kind, (a, r, c))| format!("{kind} +{a} -{r} ~{c}")).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::conf::Setting;

    fn shipped() -> (UnitRegistry, HashMap<String, String>, ProcessSettings) {
        let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        replay_source(&mut registry, &mut constants, &mut collected, DEFAULT_PHYSURE_CONF, true).unwrap();
        (registry, constants, collected)
    }

    #[test]
    fn two_builds_of_the_same_text_give_the_same_entries_and_no_difference() {
        let (a, ca, sa) = shipped();
        let (b, cb, sb) = shipped();
        let (ea, eb) = (entries(&a, &ca, &sa), entries(&b, &cb, &sb));
        let unstable = diff(&ea, &eb);
        assert!(unstable.is_empty(), "the same text gave different entries: {:?}", &unstable[..unstable.len().min(5)]);
        assert!(ea.len() > 1500, "the shipped catalog has thousands of entries: {}", ea.len());
        assert!(diff(&ea, &eb).is_empty());
        assert_eq!(summary(&diff(&ea, &eb)), "no differences");
        assert_eq!(embedded_entries().unwrap(), ea);
    }

    /// The guarantee: whatever the registry holds, a difference in it is a difference in the entries. One mutation per
    /// field of `UnitRegistry` (the destructuring in `entries` makes a new field a compile error there; this test
    /// makes it a failure here until a mutation for it is added).
    #[test]
    fn every_field_of_the_registry_is_seen_by_the_entries() {
        let (registry, constants, settings) = shipped();
        let before = entries(&registry, &constants, &settings);
        let mutations: Vec<(&str, Box<dyn Fn(&mut UnitRegistry)>)> = vec![
            ("base_units", Box::new(|r| r.base_units.values_mut().next().unwrap().scale *= 3.0)),
            ("derived_units", Box::new(|r| r.derived_units.values_mut().next().unwrap().scale *= 3.0)),
            ("aliases", Box::new(|r| { r.aliases.insert("zz_alias".into(), "m".into()); })),
            ("prefixes", Box::new(|r| *r.prefixes.get_mut("k").unwrap() *= 3.0)),
            ("prefix_exact", Box::new(|r| { r.prefix_exact.remove("k"); })),
            ("prefix_names", Box::new(|r| { r.prefix_names.insert("zzkilo".into(), "k".into()); })),
            ("symbol_aliases", Box::new(|r| r.symbol_aliases.get_mut("m").unwrap().push("zzmeter".into()))),
            ("base_dimension", Box::new(|r| { r.base_dimension.insert("zz".into(), "Z".into()); })),
            ("dimension_base", Box::new(|r| { r.dimension_base.insert("Z".into(), "zz".into()); })),
            ("recipes", Box::new(|r| { r.recipes.insert("zz_recipe".into()); })),
            ("recipe_aliases", Box::new(|r| r.recipe_aliases.values_mut().next().unwrap().push("zz_recipe".into()))),
            ("no_prefix", Box::new(|r| { r.no_prefix.insert("zz_noprefix".into()); })),
            ("unit_meta", Box::new(|r| { r.unit_meta.entry("m".into()).or_default().description = Some("changed".into()); })),
            ("categories", Box::new(|r| r.categories.values_mut().next().unwrap().push("zz_member".into()))),
            ("constants_meta", Box::new(|r| r.constants_meta.values_mut().next().unwrap().value.push('1'))),
            ("atomic_weights", Box::new(|r| { r.atomic_weights.insert("H".into(), (1.0, 0.0)); })),
            ("atomic_weight_tables", Box::new(|r| { r.atomic_weight_tables.values_mut().next().unwrap().insert("Zz".into(), (1.0, 0.0)); })),
            ("atomic_weight_selection", Box::new(|r| r.atomic_weight_selection.push_str("-x"))),
        ];
        assert_eq!(mutations.len(), 18, "a field was added to UnitRegistry: add its mutation");
        for (field, mutate) in mutations {
            let mut changed = registry.clone();
            mutate(&mut changed);
            assert_ne!(changed, registry, "{field}: the mutation changed nothing");
            let after = entries(&changed, &constants, &settings);
            assert_ne!(after, before, "the entries do not see a change of {field}");
            assert!(!diff(&before, &after).is_empty(), "{field}");
        }
    }

    #[test]
    fn constants_and_settings_are_seen_too() {
        let (registry, mut constants, settings) = shipped();
        let before = entries(&registry, &constants, &settings);
        constants.insert("zz".into(), "1 m".into());
        assert_ne!(entries(&registry, &constants, &settings), before);
        let mut other = settings.clone();
        other.settings.push(Setting::ParallelThreshold(1));
        assert_ne!(entries(&registry, &HashMap::new(), &other), entries(&registry, &HashMap::new(), &settings));
        let mut grants = settings.clone();
        grants.compatibility = Some(vec!["steradian, plain = convert".into()]);
        assert_ne!(entries(&registry, &constants, &grants), entries(&registry, &constants, &settings));
        let mut ignored = settings.clone();
        ignored.settings.push(Setting::Ignored { key: "locale".into(), value: "x".into() });
        let seen = diff(&entries(&registry, &constants, &settings), &entries(&registry, &constants, &ignored));
        assert!(seen.iter().any(|c| c.kind() == "setting_ignored"), "{seen:?}");
    }

    #[test]
    fn approx_rel_tol_is_an_entry_and_a_change_of_it_is_seen() {
        let (registry, constants, settings) = shipped();
        let e = entries(&registry, &constants, &settings);
        assert!(e.iter().any(|x| x.kind == "setting" && x.name == "approx_rel_tol" && x.value == "0.001"), "{e:?}");
        let mut tighter = settings.clone();
        tighter.settings.push(Setting::ApproxRelTol(1e-6));
        let seen = diff(&e, &entries(&registry, &constants, &tighter));
        assert!(seen.iter().any(|c| c.kind() == "setting" && c.name() == "approx_rel_tol"), "{seen:?}");
        let default = ProcessSettings::default();
        let explicit = ProcessSettings { settings: vec![Setting::ApproxRelTol(crate::settings::DEFAULT_APPROX_REL_TOL)], compatibility: None };
        assert_eq!(entries(&registry, &constants, &default), entries(&registry, &constants, &explicit));
    }

    /// `name` is a `setting` entry with `shown` for the shipped catalog; a file that sets `changed` is seen by `diff`,
    /// and a file that writes the compiled default `default` has the entries of one that does not mention it.
    fn assert_setting_entry(name: &str, shown: &str, changed: Setting, default: Setting) {
        let (registry, constants, settings) = shipped();
        let e = entries(&registry, &constants, &settings);
        assert!(e.iter().any(|x| x.kind == "setting" && x.name == name && x.value == shown), "{name}: {e:?}");
        let mut other = settings.clone();
        other.settings.push(changed);
        let seen = diff(&e, &entries(&registry, &constants, &other));
        assert!(seen.iter().any(|c| c.kind() == "setting" && c.name() == name), "{seen:?}");
        let explicit = ProcessSettings { settings: vec![default], compatibility: None };
        assert_eq!(entries(&registry, &constants, &ProcessSettings::default()), entries(&registry, &constants, &explicit));
    }

    #[test]
    fn monte_carlo_samples_is_an_entry_and_a_change_of_it_is_seen() {
        assert_setting_entry(
            "monte_carlo_samples",
            "1000",
            Setting::MonteCarloSamples(5000),
            Setting::MonteCarloSamples(crate::settings::DEFAULT_MONTE_CARLO_SAMPLES),
        );
    }

    #[test]
    fn max_while_iterations_is_an_entry_and_a_change_of_it_is_seen() {
        assert_setting_entry(
            "max_while_iterations",
            "10000",
            Setting::MaxWhileIterations(20_000),
            Setting::MaxWhileIterations(crate::settings::DEFAULT_MAX_WHILE_ITERATIONS),
        );
    }

    #[test]
    fn a_setting_a_file_does_not_mention_has_its_compiled_default_in_the_entries() {
        let (registry, constants, _) = shipped();
        let settings = ProcessSettings::default();
        let explicit = ProcessSettings { settings: vec![Setting::MaxPipelineSteps(crate::settings::DEFAULT_MAX_PIPELINE_STEPS)], compatibility: None };
        assert_eq!(entries(&registry, &constants, &settings), entries(&registry, &constants, &explicit));
        let later_wins = ProcessSettings { settings: vec![Setting::MaxPipelineSteps(5), Setting::MaxPipelineSteps(6)], ..settings };
        let e = entries(&registry, &constants, &later_wins);
        assert!(e.iter().any(|x| x.kind == "setting" && x.name == "max_pipeline_steps" && x.value == "6"));
    }

    #[test]
    fn the_order_of_a_category_counts_and_a_repeated_key_is_not_dropped() {
        let build = |text: &str| {
            let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
            replay_source(&mut registry, &mut constants, &mut collected, DEFAULT_PHYSURE_CONF, true).unwrap();
            replay_source(&mut registry, &mut constants, &mut collected, text, false).unwrap();
            entries(&registry, &constants, &collected)
        };
        let (a, b) = (build("[Categories]
zzcat = [m, s, kg]
"), build("[Categories]
zzcat = [kg, s, m]
"));
        assert!(diff(&a, &b).iter().any(|c| c.kind() == "category" && c.name() == "zzcat"));
        let e = |value: &str| Entry { kind: "unit", name: "m".into(), value: value.into() };
        let seen = diff(&[e("1"), e("2")], &[e("2")]);
        assert_eq!(seen, vec![Change::Changed { kind: "unit", name: "m".into(), old: "1 | 2".into(), new: "2".into() }]);
    }

    #[test]
    fn diff_names_what_was_added_removed_and_changed() {
        let e = |kind, name: &str, value: &str| Entry { kind, name: name.into(), value: value.into() };
        let old = vec![e("constant", "a", "1"), e("constant", "b", "2"), e("unit", "m", "x")];
        let new = vec![e("constant", "a", "1"), e("constant", "b", "3"), e("constant", "c", "4")];
        let changes = diff(&old, &new);
        assert_eq!(
            changes,
            vec![
                Change::Changed { kind: "constant", name: "b".into(), old: "2".into(), new: "3".into() },
                Change::Added(e("constant", "c", "4")),
                Change::Removed(e("unit", "m", "x")),
            ]
        );
        assert_eq!(summary(&changes), "constant +1 -0 ~1; unit +0 -1 ~0");
    }
}
