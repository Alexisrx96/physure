use crate::uncertainty::{PropagationMode, set_propagation_mode};
use crate::units::angle_mode::{set_angle_mode, AngleMode};
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
    let mut periodic_names: Vec<String> = Vec::new();
    let mut saw_periodic = false;
    let mut compatibility_rows: Vec<String> = Vec::new();
    let mut has_compatibility = false;

    // Pass 1: Extract Prefixes, Constants, Dimensions, and Base Units
    for line in conf_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current_section = &line[1..line.len() - 1];
            has_compatibility |= current_section == "Compatibility";
            continue;
        }

        match current_section {
            "Settings" if apply_settings => apply_setting(line),
            "Prefixes" => read_prefix(line, registry, problems),
            "Units" => {
                unit_lines.push(line);
                declare_base_unit(line, registry, &mut dim_to_base);
            }
            "Constants" => read_constant(line, registry, constants),
            "AtomicWeights" => read_atomic_weight(line, registry, problems),
            "Categories" => read_category(line, registry),
            "Compatibility" => {
                read_compatibility_line(line, &mut periodic_names, &mut saw_periodic, &mut compatibility_rows)
            }
            _ => {}
        }
    }

    register_fallback_bases(registry, &mut dim_to_base);

    // Pass 2: Parse and register all units using the dynamic dimension map
    for line in unit_lines {
        if let Err(reason) = parse_unit_line(line, registry, &dim_to_base) {
            problems.push(format!("{line}: {reason}"));
        }
    }

    mark_periodic(registry, &periodic_names, problems);

    // A section replaces the rows when it has rows, or has no `periodic` line at all (an empty section is an explicit
    // "none"). A section that only has a `periodic` line, even an empty one, leaves them alone: a project that adds a
    // unit of its own must not lose the grants.
    if apply_settings && has_compatibility && (!compatibility_rows.is_empty() || !saw_periodic) {
        apply_compatibility(&compatibility_rows);
    }
}

/// One line of `[Settings]`. A value that does not read is reported on stderr and the default stays.
fn apply_setting(line: &str) {
    let Some((key, raw)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) else {
        return;
    };
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
        "angle_mode" => match raw.parse::<AngleMode>() {
            Ok(mode) => {
                set_angle_mode(mode);
            }
            // Same reasoning as `propagation_mode`: a conf is read before there is anywhere to report to, and the
            // mode it names is not worth refusing to start over.
            Err(why) => eprintln!("physure.conf: {why}; keeping the angle mode in force"),
        },
        "parallel_threshold" => apply_count_setting(key, raw, |n| {
            crate::settings::set_parallel_threshold(n);
        }),
        "max_loop_iterations" => apply_count_setting(key, raw, |n| {
            crate::settings::set_max_loop_iterations(n);
        }),
        "max_pipeline_steps" => apply_count_setting(key, raw, |n| {
            crate::settings::set_max_pipeline_steps(n);
        }),
        _ => {}
    }
}

fn apply_count_setting(key: &str, raw: &str, set: impl FnOnce(usize)) {
    match raw.parse::<usize>() {
        Ok(n) => set(n),
        Err(_) => eprintln!("physure.conf: invalid {key} '{raw}'; keeping default"),
    }
}

/// One line of `[Prefixes]`: `name = symbol, factor`.
fn read_prefix(line: &str, registry: &mut UnitRegistry, problems: &mut Vec<String>) {
    let Some((name, val_part)) = line.split_once('=') else {
        return;
    };
    let parts: Vec<&str> = val_part.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        problems.push(format!("{line}: a prefix is `name = symbol, factor`"));
        return;
    }
    let symbol = parts[0];
    if let Some((factor, exact)) = parse_number(parts[1]) {
        registry.add_prefix_exact(symbol.to_string(), factor, exact);
        registry.add_prefix_name(name.trim().to_string(), symbol.to_string());
    } else {
        problems.push(format!("{line}: the factor of a prefix is a number, not '{}'", parts[1]));
    }
}

/// A line of `[Units]` that declares the base unit of a dimension (scale 1 with a single dimension symbol).
fn declare_base_unit(line: &str, registry: &mut UnitRegistry, dim_to_base: &mut HashMap<String, String>) {
    let Some((left, aliases_str)) = split_unit_line(line) else {
        return;
    };
    let parts: Vec<&str> = left.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        return;
    }
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

/// One line of `[Constants]`: `name = expression, description, latex`.
fn read_constant(line: &str, registry: &mut UnitRegistry, constants: &mut HashMap<String, String>) {
    let Some((name, val_part)) = line.split_once('=') else {
        return;
    };
    let name = name.trim();
    let parts: Vec<&str> = val_part.split(',').map(|s| s.trim()).collect();
    let expr = parts[0];
    let desc = parts.get(1).map(|s| s.to_string());
    let latex = parts.get(2).map(|s| s.to_string());
    constants.insert(name.to_string(), expr.to_string());
    registry.constants_meta.insert(name.to_string(), crate::units::registry::ConstantMetadata {
        value: expr.to_string(),
        description: desc,
        latex_symbol: latex,
    });
}

/// One line of `[AtomicWeights]`, in g/mol. Two forms:
///
/// - `symbol = weight, half-width [, standard]`: by default the second number is the half-width of the interval
///   CIAAW publishes (its "+-"), read as a uniform distribution, whose standard uncertainty is
///   `half-width / sqrt(3)` (IUPAC, "Interpretation and use of standard atomic weights"); that is what the registry
///   stores. A third field `standard` says the number already is a standard uncertainty (it is stored as given);
///   `interval` spells out the default.
/// - `symbol = [a, b]`: an interval with no single value (CIAAW prints these for elements whose composition varies in
///   nature). The weight is the midpoint `(a + b) / 2` and the standard uncertainty is `(b - a) / sqrt(12)`, the
///   same rule with the half-width `(b - a) / 2`. It takes no third field.
///
/// A weight is positive and finite and the number is finite and not negative (`0 < a < b` for an interval);
/// anything else is a problem, because a molar mass built from it would be a confident wrong answer.
fn read_atomic_weight(line: &str, registry: &mut UnitRegistry, problems: &mut Vec<String>) {
    let parsed = (|| {
        let (symbol, rest) = line.split_once('=')?;
        let symbol = symbol.trim();
        let mut chars = symbol.chars();
        let is_symbol = chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && symbol.len() <= 3
            && chars.all(|c| c.is_ascii_lowercase());
        let (weight, std) = match rest.trim().strip_prefix('[') {
            Some(interval) => read_weight_interval(interval)?,
            None => read_weight_and_number(rest)?,
        };
        is_symbol.then(|| (symbol.to_string(), (weight, std)))
    })();
    match parsed {
        Some((symbol, value)) => {
            registry.atomic_weights.insert(symbol, value);
        }
        None => problems.push(format!(
            "{line}: expected `symbol = weight, half-width` (or `, standard` after a standard uncertainty) or `symbol = [a, b]`, with a positive weight and a non-negative number"
        )),
    }
}

/// `weight, number [, interval|standard]` of an `[AtomicWeights]` line: the weight and its standard uncertainty.
fn read_weight_and_number(text: &str) -> Option<(f64, f64)> {
    let (weight, rest) = text.split_once(',')?;
    let (number, kind) = rest.split_once(',').map_or((rest, "interval"), |(number, kind)| (number, kind.trim()));
    let weight: f64 = weight.trim().parse().ok()?;
    let number: f64 = number.trim().parse().ok()?;
    let std = match kind {
        "interval" => number / 3f64.sqrt(),
        "standard" => number,
        _ => return None,
    };
    (weight.is_finite() && weight > 0.0 && number.is_finite() && number >= 0.0).then_some((weight, std))
}

/// `a, b]` (what follows the `[`) of an `[AtomicWeights]` line: the midpoint and `(b - a) / sqrt(12)`.
fn read_weight_interval(text: &str) -> Option<(f64, f64)> {
    let (low, high) = text.strip_suffix(']')?.split_once(',')?;
    let (low, high): (f64, f64) = (low.trim().parse().ok()?, high.trim().parse().ok()?);
    (low.is_finite() && high.is_finite() && 0.0 < low && low < high).then(|| ((low + high) / 2.0, (high - low) / 12f64.sqrt()))
}

/// One line of `[Categories]`: `name = [unit, unit, ...]`.
fn read_category(line: &str, registry: &mut UnitRegistry) {
    let Some((cat_name, list_str)) = line.split_once('=') else {
        return;
    };
    let cat_name = cat_name.trim();
    let list_str = list_str.trim().trim_start_matches('[').trim_end_matches(']').trim();
    let items: Vec<String> = list_str
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    registry.categories.insert(cat_name.to_string(), items);
}

/// One line of `[Compatibility]`. `periodic = hertz` names units that the SI reserves for periodic phenomena (and
/// sets `saw_periodic`, even when no name follows); any other line is a row, `radian, plain = convert`, kept for
/// `apply_compatibility`.
fn read_compatibility_line(line: &str, periodic: &mut Vec<String>, saw_periodic: &mut bool, rows: &mut Vec<String>) {
    match line.split_once('=') {
        Some((key, names)) if key.trim() == "periodic" => {
            *saw_periodic = true;
            periodic.extend(names.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_string));
        }
        _ => rows.push(line.to_string()),
    }
}

/// The marker a row of `[Compatibility]` lets a conversion read as 1.
enum Marker {
    Radian,
    Steradian,
}

/// A row is `unit, plain = convert`, with the unit spelled `radian`/`rad` or `steradian`/`sr`, in either order.
/// Anything else is not supported yet: a sum or a comparison never reads a marker as 1 (Python decides those from
/// `Dimension`, which stays strict), and two markers do not convert into each other.
fn read_compatibility_row(row: &str) -> Result<Marker, String> {
    let Some((kinds, operations)) = row.split_once('=') else {
        return Err(format!("[Compatibility] '{row}' is not `kind, kind = operations`"));
    };
    let operations: Vec<&str> = operations.split_whitespace().collect();
    if operations != ["convert"] {
        return Err(format!(
            "[Compatibility] '{row}': the operation must be exactly `convert` (`add` and `compare` are not supported yet)"
        ));
    }
    let kinds: Vec<&str> = kinds.split(',').map(str::trim).collect();
    let marker = match kinds.as_slice() {
        [marker, "plain"] | ["plain", marker] => *marker,
        _ => return Err(format!("[Compatibility] '{row}': a row pairs a marker unit with `plain`")),
    };
    match marker {
        "radian" | "rad" => Ok(Marker::Radian),
        "steradian" | "sr" => Ok(Marker::Steradian),
        other => Err(format!("[Compatibility] '{row}': '{other}' is not a marker unit (radian or steradian)")),
    }
}

/// Replaces the process-wide grants by the rows of a `[Compatibility]` section. A row that cannot be read is
/// reported on stderr and grants nothing.
fn apply_compatibility(rows: &[String]) {
    let (mut radian, mut steradian) = (false, false);
    for row in rows {
        match read_compatibility_row(row) {
            Ok(Marker::Radian) => radian = true,
            Ok(Marker::Steradian) => steradian = true,
            Err(why) => eprintln!("physure.conf: {why}; the row is ignored"),
        }
    }
    crate::units::angle_mode::set_marker_grants(radian, steradian);
}

/// Flags the units that `[Compatibility] periodic` names, once they are registered. A name is any spelling of the
/// unit (`hertz`); the flag goes on the unit the registry keeps, so that its prefixes (built from a copy of it) and
/// its aliases carry it. A name that does not name a unit defined in `[Units]` (a base unit, or a prefixed spelling,
/// cannot be flagged) is a problem.
fn mark_periodic(registry: &mut UnitRegistry, names: &[String], problems: &mut Vec<String>) {
    for name in names {
        let symbol = registry.resolve_symbol(name);
        match registry.derived_units.get_mut(&symbol) {
            Some(unit) => unit.periodic = true,
            None => problems.push(format!(
                "[Compatibility] periodic: '{name}' does not name a unit defined in [Units] (a base unit or a prefixed spelling cannot be flagged)"
            )),
        }
    }
}

/// The SI base units and the standard dimension symbols, for a conf that does not map them itself.
fn register_fallback_bases(registry: &mut UnitRegistry, dim_to_base: &mut HashMap<String, String>) {
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

    for (dimension, base) in dim_to_base.iter() {
        registry.base_dimension.entry(base.clone()).or_insert_with(|| dimension.clone());
    }

    registry.dimension_base = dim_to_base.clone();
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

/// A unit line cut into its name, its values and the names it lists in brackets: `name = values [alias, alias]`.
fn cut_unit_line(line: &str) -> Option<(&str, &str, Vec<String>)> {
    let (left, aliases_str) = match line.split_once('[') {
        Some((l, r)) => {
            let r = r.trim_end_matches(']').trim();
            (l.trim(), r)
        }
        None => (line, ""),
    };

    let (key, val_part) = left.split_once('=').map(|(k, v)| (k.trim(), v.trim()))?;

    let aliases: Vec<String> = aliases_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Some((key, val_part, aliases))
}

fn split_unit_line(line: &str) -> Option<(&str, Vec<String>)> {
    let (key, val_part, mut aliases) = cut_unit_line(line)?;

    if !aliases.contains(&key.to_string()) {
        aliases.insert(0, key.to_string());
    }

    Some((val_part, aliases))
}

fn is_single_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The fields of a unit line's values, without the flags (`noprefix`, `own_kind`); a single field with spaces in it
/// is read as the fields separated by them.
fn unit_value_fields(val_part: &str) -> Vec<&str> {
    let parts: Vec<&str> = val_part
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !is_flag(s))
        .collect();

    if parts.len() == 1 && parts[0].contains(' ') {
        return parts[0]
            .split_whitespace()
            .filter(|s| !s.is_empty() && !is_flag(s))
            .collect();
    }
    parts
}

fn parse_unit_line(line: &str, registry: &mut UnitRegistry, dim_to_base: &HashMap<String, String>) -> Result<(), String> {
    let Some((key, val_part, mut aliases)) = cut_unit_line(line) else {
        return Err("a unit line is `name = factor, dimension, [names]`".to_string());
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

    let no_prefix = val_part.split(',').any(|field| field.trim() == "noprefix");

    let parts = unit_value_fields(val_part);

    if parts.is_empty() {
        return Err("the line has no value".to_string());
    }

    // `pH = Log, -1, 1.0, 1, noprefix`: a level is `Log, factor, reference, dimension`. Its dimension
    // is "1" when it is the logarithm of a ratio, or the dimensions of a quantity (`dBW`).
    if parts[0].eq_ignore_ascii_case("log") {
        register_level(&parts, &symbol, registry, dim_to_base)?;
        register_aliases(registry, key, &symbol, &aliases, &names, no_prefix);
        return Ok(());
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
    let (offset, exact_offset, dim_index) = zero_point(&parts);
    let scale = UnitScale { factor, exact, offset, exact_offset };

    let dim_str = if parts.len() > dim_index { parts[dim_index] } else { "1" };

    let base_symbol = dim_to_base.get(dim_str).cloned().unwrap_or_default();

    // Dimension "1" is tested before `base_symbol`, which maps it to the `unity` unit:
    // going through that mapping would make every scaled ratio (`%`, `ppm`) a quantity of
    // dimension "unity", so `2 m * 3 %` printed "m * unity" instead of plain metres.
    if dim_str == "1" || dim_str.is_empty() {
        let mut u = match &scale.exact {
            Some(e) => RationalUnit::new_from_dimensions([]).with_exact_scale(e.clone()),
            None => RationalUnit::new_from_dimensions([]).with_scale(scale.factor),
        };
        u.display_name = Some(symbol.clone());
        registry.add_derived_unit(symbol.clone(), u);
    } else if !base_symbol.is_empty() {
        register_on_base(registry, &symbol, &base_symbol, scale);
    } else {
        let recipe_str = recipe_field(&parts, dim_index + 1);
        register_recipe_unit(registry, &symbol, &names, (dim_str, recipe_str), &scale, dim_to_base)?;
    }

    register_aliases(registry, key, &symbol, &aliases, &names, no_prefix);
    Ok(())
}

/// A unit's scale as its line writes it: the factor, and the zero point of an affine unit; each with its exact
/// rational when it has one.
struct UnitScale {
    factor: f64,
    exact: Option<crate::units::exact::ExactScale>,
    offset: f64,
    exact_offset: Option<crate::units::exact::ExactScale>,
}

/// The zero point of an affine unit (`celsius = 1.0, 273.15, O`) and the index of the dimension field after it.
fn zero_point(parts: &[&str]) -> (f64, Option<crate::units::exact::ExactScale>, usize) {
    if parts.len() > 2 {
        if let Some((parsed, exact)) = parse_number(parts[1]) {
            return (parsed, exact, 2);
        }
    }
    (0.0, None, 1)
}

/// The recipe field of a unit line (`N = 1.0, F, kg*m/s^2`): empty when the line has none or it is a flag.
fn recipe_field<'a>(parts: &[&'a str], index: usize) -> &'a str {
    match parts.get(index) {
        Some(field) if !field.is_empty() && !is_flag(field) => field,
        _ => "",
    }
}

/// A level `Log, factor, reference, dimension`: `1` as the dimension is a level of a plain ratio, any other
/// dimension is a level of a quantity (`dBW`).
fn register_level(
    parts: &[&str],
    symbol: &str,
    registry: &mut UnitRegistry,
    dim_to_base: &HashMap<String, String>,
) -> Result<(), String> {
    let number = |index: usize| parts.get(index).and_then(|text| text.parse::<f64>().ok());
    let (Some(factor), Some(reference), Some(dimension)) = (number(1), number(2), parts.get(3).copied()) else {
        return Err("a level is `Log, factor, reference, dimension`".to_string());
    };
    let quantity = if dimension == "1" {
        RationalUnit::dimensionless()
    } else {
        let normalized = normalize_dimension_symbols(dimension, dim_to_base);
        crate::units::parser::Parser::parse_expression_with_registry(&normalized, registry)
            .map_err(|_| format!("cannot read the dimension '{dimension}' of the level {symbol}"))?
    };
    registry.add_level(symbol.to_string(), quantity, factor, reference);
    Ok(())
}

/// A unit on the base unit of its dimension: affine with a zero point, the base unit itself at scale 1, or scaled.
fn register_on_base(registry: &mut UnitRegistry, symbol: &str, base_symbol: &str, scale: UnitScale) {
    let UnitScale { factor, exact, offset, exact_offset } = scale;
    if offset != 0.0 {
        registry.add_affine_unit_exact(symbol.to_string(), base_symbol, factor, exact, offset, exact_offset);
    } else if factor == 1.0 && symbol == base_symbol {
        registry.add_base_unit(symbol.to_string());
    } else {
        registry.add_scaled_unit_exact(symbol.to_string(), base_symbol, factor, exact);
    }
}

/// A unit defined by a recipe or by its dimension spelled out (`dimension`, `recipe`): the unit that text reads
/// as, rescaled by the line's factor.
fn register_recipe_unit(
    registry: &mut UnitRegistry,
    symbol: &str,
    names: &[String],
    (dim_str, recipe_str): (&str, &str),
    scale: &UnitScale,
    dim_to_base: &HashMap<String, String>,
) -> Result<(), String> {
    let parsed_unit_opt = if !recipe_str.is_empty() {
        crate::units::parser::Parser::parse_expression_with_registry(recipe_str, registry).ok()
    } else {
        None
    };

    let parsed_unit_opt = parsed_unit_opt.or_else(|| {
        let normalized_dim = normalize_dimension_symbols(dim_str, dim_to_base);
        crate::units::parser::Parser::parse_expression_with_registry(&normalized_dim, registry).ok()
    });

    let Some(parsed_unit) = parsed_unit_opt else {
        return Err(format!("cannot read the dimension '{dim_str}' of {symbol}, or its recipe"));
    };
    let rescaled = match (&parsed_unit.exact, &scale.exact) {
        (Some(p), Some(e)) => {
            let combined = p.mul(e);
            parsed_unit.with_exact_scale(combined)
        }
        _ => {
            let product = parsed_unit.scale * scale.factor;
            parsed_unit.with_scale(product)
        }
    };
    // Carry the zero point through here too: dropping it would silently turn an
    // affine definition into a purely multiplicative one.
    let mut u = rescaled.with_offset(scale.offset);
    if let Some(e) = &scale.exact_offset {
        u = u.with_exact_offset(e.clone());
    }
    u.display_name = Some(symbol.to_string());
    // A recipe at scale 1 is a name for its base symbols (`N` is `kg*m/s^2`).
    if !recipe_str.is_empty() && scale.factor == 1.0 && scale.offset == 0.0 {
        registry.note_recipe(symbol, &u.dimensions, names);
    }
    registry.add_derived_unit(symbol.to_string(), u);
    Ok(())
}

/// Makes the key of a unit line and each of its aliases another name for `symbol`.
fn register_aliases(
    registry: &mut UnitRegistry,
    key: &str,
    symbol: &str,
    aliases: &[String],
    names: &[String],
    no_prefix: bool,
) {
    for name in names.iter().map(String::as_str).chain([key, symbol]) {
        if no_prefix {
            registry.no_prefix.insert(name.to_string());
        } else {
            registry.no_prefix.remove(name);
        }
    }
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

/// Runs `f` against the catalog that unit text is read against when nothing says otherwise: the embedded
/// catalog plus the `physure.conf` of the working directory, if there is one.
///
/// It is looked for on every call, so a file that appears, changes or goes away is seen. Without one,
/// `f` gets the embedded catalog itself, by reference: no copy is made, which is what turning a unit
/// string into a unit costs most when the catalog is copied first. With one, the merged registry is built
/// as `build_registry_from_conf` builds it.
pub fn with_conf_registry<R>(f: impl FnOnce(&UnitRegistry) -> R) -> R {
    if Path::new("physure.conf").is_file() {
        let (registry, _) = build_registry_from_conf();
        f(&registry)
    } else {
        f(&embedded_catalog().0)
    }
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
    fn the_embedded_conf_sets_the_default_angle_mode() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        // Start from the mode that is NOT the default, so only the conf line can bring `Dimensionless` back.
        set_angle_mode(AngleMode::Dimension);
        parse_physure_conf(DEFAULT_PHYSURE_CONF, &mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimensionless);
    }

    #[test]
    fn parses_angle_mode_from_settings_section() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        set_angle_mode(AngleMode::Dimensionless);
        parse_physure_conf("[Settings]\nangle_mode = dimension\n", &mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimension);
    }

    #[test]
    fn a_bad_angle_mode_keeps_the_one_in_force() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        // Not the default, so a handler that falls back to the default on a bad value is caught.
        set_angle_mode(AngleMode::Dimension);
        parse_physure_conf("[Settings]\nangle_mode = degrees\n", &mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimension);
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

    #[test]
    fn a_conf_can_name_a_unit_of_its_own_as_periodic() {
        let (mut reg, mut constants) = build_registry_from_conf();
        extend_registry(
            &mut reg,
            &mut constants,
            "[Units]\nmyfreq = 1.0, T^-1, 1/s, [mf]\n[Compatibility]\nperiodic = myfreq\n",
        )
        .unwrap();
        assert!(reg.get_unit("mf").unwrap().periodic, "an alias carries the flag");
        assert!(reg.get_unit("kmf").unwrap().periodic, "a prefixed form carries the flag");
        assert!(!reg.get_unit("m").unwrap().periodic, "a unit the section does not name does not");
    }

    #[test]
    fn a_periodic_name_that_is_no_unit_is_a_problem_and_the_registry_stays_as_it_was() {
        let (mut reg, mut constants) = build_registry_from_conf();
        let (before, constants_before) = (reg.clone(), constants.clone());
        let problems = extend_registry(
            &mut reg,
            &mut constants,
            "[Units]\nmyfreq = 1.0, T^-1, 1/s, [mf]\n[Compatibility]\nperiodic = myfreq, nosuchunit\n",
        )
        .unwrap_err();
        assert!(problems.iter().any(|problem| problem.contains("nosuchunit")), "{problems:?}");
        assert_eq!(reg, before, "the unit that was fine is rolled back with the one that was not");
        assert_eq!(constants, constants_before);
        assert!(reg.get_unit("myfreq").is_none());
    }

    #[test]
    fn a_strict_reading_ignores_the_lines_of_compatibility_that_are_not_periodic() {
        let (mut reg, mut constants) = build_registry_from_conf();
        let before = reg.clone();
        // No `=`, a row, and a `periodic` that names nothing: none of them is a problem.
        extend_registry(
            &mut reg,
            &mut constants,
            "[Compatibility]\nthis line has no equals sign\nradian, plain = convert\nperiodic =\n",
        )
        .unwrap();
        assert_eq!(reg, before);
    }

    fn marker_grants() -> (bool, bool) {
        use crate::units::angle_mode::marker_converts_to_plain;
        (marker_converts_to_plain("rad"), marker_converts_to_plain("sr"))
    }

    /// The grants after reading `conf` with the grants first set to `start`. The caller holds `TEST_LOCK` and a
    /// `RestoreProcessState` guard.
    fn grants_after(conf: &str, start: (bool, bool)) -> (bool, bool) {
        crate::units::angle_mode::set_marker_grants(start.0, start.1);
        parse_physure_conf(conf, &mut UnitRegistry::new(), &mut HashMap::new());
        marker_grants()
    }

    #[test]
    fn the_embedded_conf_lets_a_conversion_read_both_markers_as_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        // Start from "none", so only the rows of the embedded conf can bring both back.
        assert_eq!(grants_after(DEFAULT_PHYSURE_CONF, (false, false)), (true, true));
    }

    #[test]
    fn a_compatibility_section_replaces_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        assert_eq!(grants_after("[Compatibility]\nradian, plain = convert\n", (true, true)), (true, false));
        assert_eq!(grants_after("[Compatibility]\nplain, steradian = convert\n", (true, true)), (false, true));
    }

    #[test]
    fn an_empty_compatibility_section_grants_nothing() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        assert_eq!(grants_after("[Compatibility]\n", (true, true)), (false, false));
    }

    #[test]
    fn a_conf_without_the_section_keeps_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        // Not the default, so a conf that resets the rows to the default is caught.
        assert_eq!(grants_after("[Settings]\nparallel_threshold = 10000\n", (true, false)), (true, false));
    }

    #[test]
    fn a_section_that_only_names_a_periodic_unit_keeps_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        assert_eq!(grants_after("[Compatibility]\nperiodic = hertz\n", (true, false)), (true, false));
        // A `periodic` line that names nothing is still a `periodic` line, not an empty section.
        assert_eq!(grants_after("[Compatibility]\nperiodic =\n", (true, false)), (true, false));
    }

    #[test]
    fn a_section_with_rows_and_a_periodic_unit_replaces_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        // The shape of the embedded conf: rows and `periodic` in one section. The periodic name must not keep the
        // old rows alive.
        let conf = "[Compatibility]\nradian, plain = convert\nperiodic = hertz\n";
        assert_eq!(grants_after(conf, (true, true)), (true, false));
    }

    #[test]
    fn a_row_that_cannot_be_read_grants_nothing() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        for row in [
            // A sum or a comparison never reads a marker as 1, and the operation must be exactly `convert`.
            "radian, plain = convert add",
            "radian, plain = compare",
            "radian, plain = add compare convert",
            "radian, plain =",
            "radian, plain = convert convert",
            // Two markers do not convert into each other, and a plain unit is not a marker.
            "radian, steradian = convert",
            "metre, plain = convert",
            "plain, plain = convert",
            "radian = convert",
            // No `=` at all.
            "radian plain convert",
        ] {
            let conf = format!("[Compatibility]\n{row}\n");
            assert_eq!(grants_after(&conf, (true, true)), (false, false), "{row}");
        }
    }

    #[test]
    fn a_row_that_cannot_be_read_does_not_cancel_the_one_beside_it() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let conf = "[Compatibility]\nradian, plain = convert\nradian, plain = add\n";
        assert_eq!(grants_after(conf, (false, false)), (true, false));
    }

    #[test]
    fn a_system_conf_does_not_change_the_grants_but_can_name_a_periodic_unit() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let (mut reg, mut constants) = build_registry_from_conf();
        // Not the default, and not what the rows below would give: applying them would make this (true, true).
        crate::units::angle_mode::set_marker_grants(true, false);
        extend_registry(
            &mut reg,
            &mut constants,
            "[Units]\nmyfreq = 1.0, T^-1, 1/s, [mf]\n[Compatibility]\nperiodic = myfreq\nradian, plain = convert\nsteradian, plain = convert\n",
        )
        .unwrap();
        assert!(reg.get_unit("mf").unwrap().periodic);
        assert_eq!(marker_grants(), (true, false));
        extend_registry(&mut reg, &mut constants, "[Compatibility]\n").unwrap();
        assert_eq!(marker_grants(), (true, false));
    }

    fn row_error(row: &str) -> String {
        match read_compatibility_row(row) {
            Ok(_) => panic!("'{row}' should not be a readable row"),
            Err(why) => why,
        }
    }

    #[test]
    fn a_row_that_cannot_be_read_says_why() {
        assert!(row_error("radian plain convert").contains("kind, kind = operations"));
        assert!(row_error("radian, plain = add").contains("not supported yet"));
        assert!(row_error("radian, steradian = convert").contains("pairs a marker unit with `plain`"));
        assert!(row_error("metre, plain = convert").contains("not a marker unit"));
    }

    #[test]
    fn a_row_names_its_marker_in_either_order_and_either_spelling() {
        assert!(matches!(read_compatibility_row("radian, plain = convert"), Ok(Marker::Radian)));
        assert!(matches!(read_compatibility_row("rad, plain = convert"), Ok(Marker::Radian)));
        assert!(matches!(read_compatibility_row("plain, steradian = convert"), Ok(Marker::Steradian)));
        assert!(matches!(read_compatibility_row("sr, plain = convert"), Ok(Marker::Steradian)));
    }
}
