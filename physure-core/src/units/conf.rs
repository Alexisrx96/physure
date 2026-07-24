use crate::units::rational::RationalUnit;
use crate::units::registry::UnitRegistry;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const DEFAULT_PHYSURE_CONF: &str = include_str!("physure.conf");

/// Parses a `physure.conf` configuration string into a UnitRegistry and constants map.
/// All dimension symbols (L, M, T, I, O, N, J, A, etc.), prefixes, units, aliases,
/// and physical constants are extracted 100% dynamically from the configuration file.
pub fn parse_physure_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
) {
    let mut current_section = "";
    let mut dim_to_base: HashMap<String, String> = HashMap::new();
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
            "Prefixes" => {
                if let Some((_name, val_part)) = line.split_once('=') {
                    let parts: Vec<&str> = val_part.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        let symbol = parts[0];
                        if let Ok(factor) = parts[1].parse::<f64>() {
                            registry.add_prefix(symbol.to_string(), factor);
                        }
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
                        if factor == 1.0 && !dim_symbol.is_empty() && !symbol.is_empty() {
                            if is_single_ident(dim_symbol) {
                                dim_to_base.insert(dim_symbol.to_string(), symbol.clone());
                                registry.add_base_unit(symbol.clone());
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

    // Pass 2: Parse and register all units using the dynamic dimension map
    for line in unit_lines {
        parse_unit_line(line, registry, &dim_to_base);
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
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn parse_unit_line(line: &str, registry: &mut UnitRegistry, dim_to_base: &HashMap<String, String>) {
    let (left, aliases_str) = match line.split_once('[') {
        Some((l, r)) => {
            let r = r.trim_end_matches(']').trim();
            (l.trim(), r)
        }
        None => (line, ""),
    };

    let (key, val_part) = match left.split_once('=') {
        Some((k, v)) => (k.trim(), v.trim()),
        None => return,
    };

    let aliases: Vec<String> = if !aliases_str.is_empty() {
        aliases_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![]
    };

    if aliases.is_empty() {
        return;
    }

    let symbol = aliases[0].clone();

    let parts: Vec<&str> = val_part
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "noprefix")
        .collect();

    if parts.is_empty() {
        return;
    }

    let factor: f64 = match parts[0].parse() {
        Ok(f) => f,
        Err(_) => return,
    };

    let dim_str = if parts.len() >= 2 { parts[1] } else { "1" };

    let base_symbol = dim_to_base.get(dim_str).cloned().unwrap_or_default();

    if !base_symbol.is_empty() {
        if factor == 1.0 && symbol == base_symbol {
            registry.add_base_unit(symbol.clone());
        } else {
            registry.add_scaled_unit(symbol.clone(), &base_symbol, factor);
        }
    } else if dim_str == "1" || dim_str.is_empty() {
        let u = RationalUnit::new_from_dimensions([]);
        let mut u = u.with_scale(factor);
        u.display_name = Some(symbol.clone());
        registry.add_derived_unit(symbol.clone(), u);
    } else {
        let recipe_str = if parts.len() >= 3 && !parts[2].is_empty() && parts[2] != "noprefix" {
            parts[2]
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
            let new_scale = parsed_unit.scale * factor;
            let mut u = parsed_unit.with_scale(new_scale);
            u.display_name = Some(symbol.clone());
            registry.add_derived_unit(symbol.clone(), u);
        }
    }

    registry.register_alias(key.to_string(), symbol.clone());
    for alias in &aliases {
        if alias != &symbol {
            registry.register_alias(alias.clone(), symbol.clone());
        }
    }
}

/// Builds a UnitRegistry and constants map from master `physure.conf` + CWD user override `physure.conf`.
pub fn build_registry_from_conf() -> (UnitRegistry, HashMap<String, String>) {
    let mut reg = UnitRegistry::new();
    let mut constants = HashMap::new();

    // 1. Load embedded default master physure.conf
    parse_physure_conf(DEFAULT_PHYSURE_CONF, &mut reg, &mut constants);

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
