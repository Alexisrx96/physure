//! Helpers shared by integration tests. Each test file that uses them declares `mod common;`.

#![allow(dead_code)] // each test binary uses a part

use physure_core::UnitRegistry;
use std::collections::HashMap;

/// Everything a catalog determines, as sorted lines, so two catalogs can be compared line by line and a difference
/// names what differs: for every name the registry knows, the unit it stands for (scale, zero point, level, exact
/// value, the `periodic` flag of `[Compatibility]`, as `Debug`) and its facts (dimension, kind, linearity, alias,
/// simplified form; the error text where there are none); the prefixes with their exact values and spelled-out
/// names; the names of each unit, the base unit of each dimension, the recipes with their names and the units that
/// take no prefix; the metadata of units and constants; the constants;
/// your atomic weights, every edition's table and the selection; the categories; and the process-wide settings that
/// can be read (they are not part of a registry, so they describe the process at the moment of the call).
pub fn describe(registry: &UnitRegistry, constants: &HashMap<String, String>) -> Vec<String> {
    let mut lines = Vec::new();
    for name in registry.names() {
        let facts = match registry.facts(&name) {
            Ok(facts) => format!("{facts:?}"),
            Err(e) => format!("error {e}"),
        };
        lines.push(format!("unit {name} => {:?}; {facts}", registry.get_unit(&name)));
    }
    for (name, factor) in &registry.prefixes {
        lines.push(format!("prefix {name} = {factor:?} exact {:?}", registry.prefix_exact.get(name)));
    }
    for (name, symbol) in &registry.prefix_names {
        lines.push(format!("prefix name {name} = {symbol}"));
    }
    for (symbol, names) in &registry.symbol_aliases {
        lines.push(format!("names of {symbol} = {names:?}"));
    }
    for (symbol, dimension) in &registry.base_dimension {
        lines.push(format!("base unit {symbol} of {dimension}"));
    }
    for (dimension, base) in &registry.dimension_base {
        lines.push(format!("dimension {dimension} has base {base}"));
    }
    for name in &registry.recipes {
        lines.push(format!("recipe {name}"));
    }
    for (key, names) in &registry.recipe_aliases {
        lines.push(format!("recipe names of {key:?} = {names:?}"));
    }
    for name in &registry.no_prefix {
        lines.push(format!("no prefix {name}"));
    }
    for (name, meta) in &registry.unit_meta {
        lines.push(format!("unit meta {name} = {:?} {:?} {:?}", meta.category, meta.latex_symbol, meta.description));
    }
    for (name, value) in constants {
        lines.push(format!("constant {name} = {value}"));
    }
    for (name, meta) in &registry.constants_meta {
        lines.push(format!("constant meta {name} = {} {:?} {:?}", meta.value, meta.description, meta.latex_symbol));
    }
    for (symbol, weight) in &registry.atomic_weights {
        lines.push(format!("your atomic weight {symbol} = {weight:?}"));
    }
    for (edition, table) in &registry.atomic_weight_tables {
        for (symbol, weight) in table {
            lines.push(format!("atomic weight {edition} {symbol} = {weight:?}"));
        }
    }
    lines.push(format!("atomic weight selection = {}", registry.atomic_weight_selection));
    for (category, members) in &registry.categories {
        let mut members = members.clone();
        members.sort();
        lines.push(format!("category {category} = {members:?}"));
    }
    lines.extend(settings_lines());
    lines.sort();
    lines
}

/// The process-wide settings a conf can change, as lines.
pub fn settings_lines() -> Vec<String> {
    vec![
        format!("setting parallel_threshold = {}", physure_core::settings::parallel_threshold()),
        format!("setting max_loop_iterations = {}", physure_core::settings::max_loop_iterations()),
        format!("setting max_pipeline_steps = {}", physure_core::settings::max_pipeline_steps()),
        format!("setting approx_rel_tol = {:?}", physure_core::settings::approx_rel_tol()),
        format!("setting monte_carlo_samples = {}", physure_core::settings::monte_carlo_samples()),
        format!("setting max_while_iterations = {}", physure_core::settings::max_while_iterations()),
        format!("setting propagation_mode = {}", physure_core::uncertainty::propagation_mode().name()),
        format!("setting angle_mode = {}", physure_core::units::angle_mode::angle_mode().name()),
        format!("setting radian plain = {}", physure_core::units::angle_mode::marker_converts_to_plain("rad")),
        format!("setting steradian plain = {}", physure_core::units::angle_mode::marker_converts_to_plain("sr")),
    ]
}

/// Every setting of [`settings_lines`] set to a value no catalog uses.
pub fn set_sentinel_settings() {
    physure_core::settings::set_parallel_threshold(777);
    physure_core::settings::set_max_loop_iterations(778);
    physure_core::settings::set_max_pipeline_steps(779);
    physure_core::settings::set_approx_rel_tol(0.125);
    physure_core::settings::set_monte_carlo_samples(780);
    physure_core::settings::set_max_while_iterations(781);
    physure_core::uncertainty::set_propagation_mode(physure_core::uncertainty::PropagationMode::Unscented);
    physure_core::units::angle_mode::set_angle_mode(physure_core::units::angle_mode::AngleMode::Dimension);
    physure_core::units::angle_mode::set_marker_grants(false, false);
}

/// The settings as the library compiles them (what a frozen file's activation starts from).
pub fn set_compiled_defaults() {
    physure_core::settings::set_parallel_threshold(10_000);
    physure_core::settings::set_max_loop_iterations(1_000_000);
    physure_core::settings::set_max_pipeline_steps(1_000);
    physure_core::settings::set_approx_rel_tol(1e-3);
    physure_core::settings::set_monte_carlo_samples(1_000);
    physure_core::settings::set_max_while_iterations(10_000);
    physure_core::uncertainty::set_propagation_mode(physure_core::uncertainty::PropagationMode::Correlated);
    physure_core::units::angle_mode::set_angle_mode(physure_core::units::angle_mode::AngleMode::Dimensionless);
    physure_core::units::angle_mode::set_marker_grants(true, true);
}
