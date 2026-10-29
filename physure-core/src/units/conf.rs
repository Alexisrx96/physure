use crate::uncertainty::{PropagationMode, set_propagation_mode};
use crate::units::rational::RationalUnit;
use crate::units::registry::UnitRegistry;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

pub const DEFAULT_PHYSURE_CONF: &str = include_str!("physure.conf");

/// Words that can stand among the fields of a unit line without being one of its values.
fn is_flag(field: &str) -> bool {
    matches!(field, "noprefix" | "own_kind")
}

/// Parses a `physure.conf` configuration string into a UnitRegistry and constants map.
/// All dimension symbols (L, M, T, I, O, N, J, A, etc.), prefixes, units, aliases,
/// and physical constants are extracted 100% dynamically from the configuration file.
pub fn parse_physure_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
) {
    // A line the parser cannot read is skipped, as it always was: this is the lenient reading of
    // the catalog and of the `physure.conf` of the working directory.
    parse_conf(conf_str, registry, constants, true, &mut Vec::new());
}

/// Adds the units, prefixes, constants and categories of `conf_str` to `registry`, all or nothing:
/// a line the parser cannot read is a problem, and if there is one the registry and the constants
/// are left as they were and every problem is returned. `[Settings]` is not read: the settings of a
/// conf are process-wide and a registry does not own them. This is the reading to use for the
/// definitions a unit system is made of.
pub fn extend_registry(
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    conf_str: &str,
) -> Result<(), Vec<String>> {
    let backup = (registry.clone(), constants.clone());
    let mut problems = Vec::new();
    parse_conf(conf_str, registry, constants, false, &mut problems);
    if problems.is_empty() {
        return Ok(());
    }
    (*registry, *constants) = backup;
    Err(problems)
}

fn parse_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    apply_settings: bool,
    problems: &mut Vec<String>,
) {
    let mut current_section = "";
    let mut dim_to_base: HashMap<String, String> = registry.dimension_base.clone();
    let mut unit_lines: Vec<&str> = Vec::new();

    // Pass 1: Extract Prefixes, Constants, Dimensions, and Base Units
    for line in conf_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current_section = &line[1..line.len() - 1];
            continue;
        }

        match current_section {
            "Settings" if apply_settings => {
                if let Some((key, raw)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
                    match key {
                        "propagation_mode" => match raw.parse::<PropagationMode>() {
                            Ok(mode) => {
                                set_propagation_mode(mode);
                            }
                            // A conf is read before there is anywhere to report to, and the mode
                            // it names is not worth refusing to start over. Saying so on stderr
                            // beats falling back to correlated with nothing on screen: that is
                            // the wrong answer that looks right.
                            Err(why) => eprintln!("physure.conf: {why}; keeping correlated"),
                        },
                        "parallel_threshold" => match raw.parse::<usize>() {
                            Ok(n) => {
                                crate::settings::set_parallel_threshold(n);
                            }
                            Err(_) => eprintln!(
                                "physure.conf: invalid parallel_threshold '{raw}'; keeping default"
                            ),
                        },
                        "max_loop_iterations" => match raw.parse::<usize>() {
                            Ok(n) => {
                                crate::settings::set_max_loop_iterations(n);
                            }
                            Err(_) => eprintln!(
                                "physure.conf: invalid max_loop_iterations '{raw}'; keeping default"
                            ),
                        },
                        "max_pipeline_steps" => match raw.parse::<usize>() {
                            Ok(n) => {
                                crate::settings::set_max_pipeline_steps(n);
                            }
                            Err(_) => eprintln!(
                                "physure.conf: invalid max_pipeline_steps '{raw}'; keeping default"
                            ),
                        },
                        _ => {}
                    }
                }
            }
            "Prefixes" => {
                if let Some((name, val_part)) = line.split_once('=') {
                    let parts: Vec<&str> = val_part.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        let symbol = parts[0];
                        if let Ok(factor) = parts[1].parse::<f64>() {
                            registry.add_prefix_exact(symbol.to_string(), factor, crate::units::exact::ExactScale::parse(parts[1]));
                            registry.add_prefix_name(name.trim().to_string(), symbol.to_string());
                        } else {
                            problems.push(format!("{line}: the factor of a prefix is a number, not '{}'", parts[1]));
                        }
                    } else {
                        problems.push(format!("{line}: a prefix is `name = symbol, factor`"));
                    }
                }
            }
            "Units" => {
                unit_lines.push(line);
                // Detect base unit declarations (scale 1.0 with a single dimension symbol)
                if let Some((left, aliases_str)) = split_unit_line(line) {
                    let parts: Vec<&str> = left.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        let factor: f64 = parts[0].parse().unwrap_or(0.0);
                        let dim_symbol = parts[1];
                        let symbol = aliases_str.first().cloned().unwrap_or_default();
                        // Dimension "1" is the dimensionless marker `unity = 1.0, 1, [1]`
                        // declares itself against -- `parse_unit_line` below already treats it
                        // as "no dimension" (so `%`/`ppm` don't inherit a spurious `unity`
                        // dimension). Without this guard here too, this pass ran first and
                        // called `add_base_unit` on it anyway, registering a real, poisoned
                        // `("unity", (1,1))` dimension in `base_units` that `get_unit` finds
                        // before it ever reaches `parse_unit_line`'s correct, empty-dimension
                        // registration in `derived_units` -- so `unity` could not convert to
                        // or add with `%`, `ppm`, or a bare number, despite all four being
                        // dimensionless.
                        if factor == 1.0 && !dim_symbol.is_empty() && !symbol.is_empty() && dim_symbol != "1" {
                            // Only the first declaration at scale 1 for a dimension symbol
                            // is that dimension's base unit, and it wins: `physure.conf`
                            // declares both `radian` and `steradian` against `A`, and
                            // overwriting made steradian the base of angle (`90 deg => rad`
                            // failed as a unit mismatch and `1 deg + 1 sr` was accepted).
                            // A later one is the same quantity under another name
                            // (`delta_celsius` is a kelvin: as a base unit of its own it
                            // could not convert to K) unless the catalog says it is a kind
                            // of its own: `steradian` is flagged `own_kind`.
                            let first = !dim_to_base.contains_key(dim_symbol);
                            let own_kind = parts.contains(&"own_kind");
                            if is_single_ident(dim_symbol) && (first || own_kind) {
                                dim_to_base.entry(dim_symbol.to_string()).or_insert_with(|| symbol.clone());
                                registry.add_base_unit(symbol.clone());
                                registry.base_dimension.insert(symbol.clone(), dim_symbol.to_string());
                            }
                        }
                    }
                }
            }
            "Constants" => {
                if let Some((name, val_part)) = line.split_once('=') {
                    let name = name.trim();
                    let parts: Vec<&str> = val_part.split(',').map(|s| s.trim()).collect();
                    if !parts.is_empty() {
                        let expr = parts[0];
                        let desc = if parts.len() >= 2 { Some(parts[1].to_string()) } else { None };
                        let latex = if parts.len() >= 3 { Some(parts[2].to_string()) } else { None };
                        constants.insert(name.to_string(), expr.to_string());
                        registry.constants_meta.insert(name.to_string(), crate::units::registry::ConstantMetadata {
                            value: expr.to_string(),
                            description: desc,
                            latex_symbol: latex,
                        });
                    }
                }
            }
            "Categories" => {
                if let Some((cat_name, list_str)) = line.split_once('=') {
                    let cat_name = cat_name.trim();
                    let list_str = list_str.trim().trim_start_matches('[').trim_end_matches(']').trim();
                    let items: Vec<String> = list_str
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    registry.categories.insert(cat_name.to_string(), items);
                }
            }
            _ => {}
        }
    }

    // Ensure SI base units are registered as base_units
    for &base in &["m", "s", "kg", "A", "K", "mol", "cd", "rad"] {
        if !registry.base_units.contains_key(base) {
            registry.add_base_unit(base.to_string());
        }
    }

    // Fallback defaults for standard dimension symbols if not explicitly mapped
    dim_to_base.entry("L".into()).or_insert_with(|| "m".into());
    dim_to_base.entry("M".into()).or_insert_with(|| "kg".into());
    dim_to_base.entry("T".into()).or_insert_with(|| "s".into());
    dim_to_base.entry("I".into()).or_insert_with(|| "A".into());
    dim_to_base.entry("O".into()).or_insert_with(|| "K".into());
    dim_to_base.entry("N".into()).or_insert_with(|| "mol".into());
    dim_to_base.entry("J".into()).or_insert_with(|| "cd".into());
    dim_to_base.entry("A".into()).or_insert_with(|| "rad".into());

    for (dimension, base) in &dim_to_base {
        registry.base_dimension.entry(base.clone()).or_insert_with(|| dimension.clone());
    }

    registry.dimension_base = dim_to_base.clone();

    // Pass 2: Parse and register all units using the dynamic dimension map
    for line in unit_lines {
        if let Err(reason) = parse_unit_line(line, registry, &dim_to_base) {
            problems.push(format!("{line}: {reason}"));
        }
    }
}

/// A positive number of the catalog as the double and, when it is a decimal or a fraction, the
/// exact rational. `None` when it is neither (a dimension symbol, `noprefix`, ...).
fn parse_number(text: &str) -> Option<(f64, Option<crate::units::exact::ExactScale>)> {
    let exact = crate::units::exact::ExactScale::parse(text);
    match text.parse::<f64>() {
        Ok(value) => Some((value, exact)),
        Err(_) => exact.map(|e| (e.to_f64(), Some(e))),
    }
}

fn split_unit_line(line: &str) -> Option<(&str, Vec<String>)> {
    let (left, aliases_str) = match line.split_once('[') {
        Some((l, r)) => {
            let r = r.trim_end_matches(']').trim();
            (l.trim(), r)
        }
        None => (line, ""),
    };

    let (key, val_part) = match left.split_once('=') {
        Some((k, v)) => (k.trim(), v.trim()),
        None => return None,
    };

    let mut aliases: Vec<String> = if !aliases_str.is_empty() {
        aliases_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![]
    };

    if !aliases.contains(&key.to_string()) {
        aliases.insert(0, key.to_string());
    }

    Some((val_part, aliases))
}

fn is_single_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn parse_unit_line(line: &str, registry: &mut UnitRegistry, dim_to_base: &HashMap<String, String>) -> Result<(), String> {
    let (left, aliases_str) = match line.split_once('[') {
        Some((l, r)) => {
            let r = r.trim_end_matches(']').trim();
            (l.trim(), r)
        }
        None => (line, ""),
    };

    let (key, val_part) = match left.split_once('=') {
        Some((k, v)) => (k.trim(), v.trim()),
        None => return Err("a unit line is `name = factor, dimension, [names]`".to_string()),
    };

    let mut aliases: Vec<String> = if !aliases_str.is_empty() {
        aliases_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![]
    };

    // The names in the order the catalog lists them: the first one is the unit's symbol in Python.
    let mut names = aliases.clone();
    if !names.iter().any(|name| name == key) {
        names.push(key.to_string());
    }

    if !aliases.contains(&key.to_string()) {
        aliases.insert(0, key.to_string());
    }

    let symbol = aliases[0].clone();

    let mut parts: Vec<&str> = val_part
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !is_flag(s))
        .collect();

    if parts.len() == 1 && parts[0].contains(' ') {
        parts = parts[0]
            .split_whitespace()
            .filter(|s| !s.is_empty() && !is_flag(s))
            .collect();
    }

    if parts.is_empty() {
        return Err("the line has no value".to_string());
    }

    // `pH = Log, -1, 1.0, 1, noprefix`: a level is `Log, factor, reference, dimension`, and
    // its dimension is the dimensionless "1", because it is the logarithm of a ratio.
    if parts[0].eq_ignore_ascii_case("log") {
        let number = |index: usize| parts.get(index).and_then(|text| text.parse::<f64>().ok());
        if let (Some(factor), Some(reference), Some("1")) = (number(1), number(2), parts.get(3).copied()) {
            registry.add_log_unit(symbol.clone(), factor, reference);
            register_aliases(registry, key, &symbol, &aliases, &names);
            return Ok(());
        }
        return Err("a level is `Log, factor, reference, dimension`".to_string());
    }

    // A number of the catalog is a decimal (`0.0254`, `1e-5`) or a fraction (`5/9`); the exact
    // rational is what conversions use, so that a ratio is rounded once.
    let (factor, exact) = match parse_number(parts[0]) {
        Some(parsed) => parsed,
        None => return Err(format!("'{}' is not a number", parts[0])),
    };

    // Absolute affine units carry a zero point between the factor and the dimension:
    // `celsius = 1.0, 273.15, O`. Only a 3+ field line can be one, so `unity = 1.0, 1` and
    // friends — where field 2 is the dimension "1" — keep reading field 2 as the dimension.
    let mut offset = 0.0;
    let mut exact_offset = None;
    let mut dim_index = 1;
    if parts.len() > 2 {
        if let Some((parsed, exact)) = parse_number(parts[1]) {
            offset = parsed;
            exact_offset = exact;
            dim_index = 2;
        }
    }

    let dim_str = if parts.len() > dim_index { parts[dim_index] } else { "1" };

    let base_symbol = dim_to_base.get(dim_str).cloned().unwrap_or_default();

    // Dimension "1" is tested before `base_symbol`, which maps it to the `unity` unit:
    // going through that mapping would make every scaled ratio (`%`, `ppm`) a quantity of
    // dimension "unity", so `2 m * 3 %` printed "m * unity" instead of plain metres.
    if dim_str == "1" || dim_str.is_empty() {
        let mut u = match &exact {
            Some(e) => RationalUnit::new_from_dimensions([]).with_exact_scale(e.clone()),
            None => RationalUnit::new_from_dimensions([]).with_scale(factor),
        };
        u.display_name = Some(symbol.clone());
        registry.add_derived_unit(symbol.clone(), u);
    } else if !base_symbol.is_empty() {
        if offset != 0.0 {
            registry.add_affine_unit_exact(symbol.clone(), &base_symbol, factor, exact, offset, exact_offset);
        } else if factor == 1.0 && symbol == base_symbol {
            registry.add_base_unit(symbol.clone());
        } else {
            registry.add_scaled_unit_exact(symbol.clone(), &base_symbol, factor, exact);
        }
    } else {
        let recipe_index = dim_index + 1;
        let recipe_str = if parts.len() > recipe_index
            && !parts[recipe_index].is_empty()
            && !is_flag(parts[recipe_index])
        {
            parts[recipe_index]
        } else {
            ""
        };

        let parsed_unit_opt = if !recipe_str.is_empty() {
            crate::units::parser::Parser::parse_expression_with_registry(recipe_str, registry).ok()
        } else {
            None
        };

        let parsed_unit_opt = parsed_unit_opt.or_else(|| {
            let normalized_dim = normalize_dimension_symbols(dim_str, dim_to_base);
            crate::units::parser::Parser::parse_expression_with_registry(&normalized_dim, registry).ok()
        });

        if let Some(parsed_unit) = parsed_unit_opt {
            let rescaled = match (&parsed_unit.exact, &exact) {
                (Some(p), Some(e)) => {
                    let combined = p.mul(e);
                    parsed_unit.with_exact_scale(combined)
                }
                _ => {
                    let product = parsed_unit.scale * factor;
                    parsed_unit.with_scale(product)
                }
            };
            // Carry the zero point through here too: dropping it would silently turn an
            // affine definition into a purely multiplicative one.
            let mut u = rescaled.with_offset(offset);
            if let Some(e) = exact_offset {
                u = u.with_exact_offset(e);
            }
            u.display_name = Some(symbol.clone());
            // A recipe at scale 1 is a name for its base symbols (`N` is `kg*m/s^2`).
            if !recipe_str.is_empty() && factor == 1.0 && offset == 0.0 {
                registry.note_recipe(&symbol, &u.dimensions, &names);
            }
            registry.add_derived_unit(symbol.clone(), u);
        } else {
            return Err(format!("cannot read the dimension '{dim_str}' of {symbol}, or its recipe"));
        }
    }

    register_aliases(registry, key, &symbol, &aliases, &names);
    Ok(())
}

/// Makes the key of a unit line and each of its aliases another name for `symbol`.
fn register_aliases(registry: &mut UnitRegistry, key: &str, symbol: &str, aliases: &[String], names: &[String]) {
    registry.note_aliases(symbol, names);
    registry.register_alias(key.to_string(), symbol.to_string());
    for alias in aliases {
        if alias != symbol {
            registry.register_alias(alias.clone(), symbol.to_string());
        }
    }
}

/// The embedded catalog, parsed once per process.
fn embedded_catalog() -> &'static (UnitRegistry, HashMap<String, String>) {
    static EMBEDDED: OnceLock<(UnitRegistry, HashMap<String, String>)> = OnceLock::new();
    EMBEDDED.get_or_init(|| {
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        parse_physure_conf(DEFAULT_PHYSURE_CONF, &mut reg, &mut constants);
        (reg, constants)
    })
}

/// The embedded catalog plus `extra`, the text of a system file or of a user conf, read strictly
/// (`extend_registry`) after it, so that a later definition wins. The `physure.conf` of the working
/// directory is not read: the caller decides what a system is made of. A line the parser cannot
/// read is an error that lists every such line, and `[Settings]` is not applied.
pub fn build_registry_with(extra: &str) -> Result<(UnitRegistry, HashMap<String, String>), Vec<String>> {
    let (mut reg, mut constants) = embedded_catalog().clone();
    extend_registry(&mut reg, &mut constants, extra)?;
    Ok((reg, constants))
}

/// Builds a UnitRegistry and constants map from master `physure.conf` + CWD user override `physure.conf`.
///
/// The embedded catalog is parsed once per process and every call gets a copy: parsing it is most
/// of the cost of turning a unit string into a unit, and it applied the `[Settings]` of the
/// catalog again on every call, undoing any setting that had been changed since.
pub fn build_registry_from_conf() -> (UnitRegistry, HashMap<String, String>) {
    // 1. Load embedded default master physure.conf
    let (mut reg, mut constants) = embedded_catalog().clone();

    // 2. Load user override physure.conf in CWD if present
    let local_conf = Path::new("physure.conf");
    if local_conf.is_file() {
        if let Ok(content) = fs::read_to_string(local_conf) {
            parse_physure_conf(&content, &mut reg, &mut constants);
        }
    }

    (reg, constants)
}

fn normalize_dimension_symbols(expr: &str, dim_to_base: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(c) = chars.next() {
        let is_standalone = match chars.peek() {
            Some(&next) => !next.is_alphanumeric() && next != '_',
            None => true,
        };
        if is_standalone {
            let key = c.to_string();
            if let Some(base_unit) = dim_to_base.get(&key) {
                out.push_str(base_unit);
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::registry::UnitRegistry;
    use std::collections::HashMap;

    /// Builds the cached catalog now: its first build applies the catalog's `[Settings]` to the
    /// process-wide globals, and a test that sets one and reads it back must not meet that.
    fn settle_catalog_settings() {
        let _ = build_registry_from_conf();
    }

    #[test]
    fn parses_parallel_threshold_from_settings_section() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = crate::settings::parallel_threshold();
        parse_physure_conf(
            "[Settings]\nparallel_threshold = 500\n",
            &mut reg,
            &mut constants,
        );
        assert_eq!(crate::settings::parallel_threshold(), 500);
        crate::settings::set_parallel_threshold(original);
    }

    #[test]
    fn parses_max_loop_iterations_from_settings_section() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = crate::settings::max_loop_iterations();
        parse_physure_conf(
            "[Settings]\nmax_loop_iterations = 250\n",
            &mut reg,
            &mut constants,
        );
        assert_eq!(crate::settings::max_loop_iterations(), 250);
        crate::settings::set_max_loop_iterations(original);
    }

    #[test]
    fn parses_max_pipeline_steps_from_settings_section() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = crate::settings::max_pipeline_steps();
        parse_physure_conf(
            "[Settings]\nmax_pipeline_steps = 42\n",
            &mut reg,
            &mut constants,
        );
        assert_eq!(crate::settings::max_pipeline_steps(), 42);
        crate::settings::set_max_pipeline_steps(original);
    }
}
