use std::sync::OnceLock;
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::UnitRegistry;
use crate::ast::*;
use pest::Parser;
use super::{Rule, PhsParser};
use super::expressions::parse_expr;

fn unit_registry() -> &'static UnitRegistry {
    static REGISTRY: OnceLock<UnitRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| physure_core::units::conf::build_registry_from_conf().0)
}

/// True if `name` resolves to a registered unit. Used to tell a unit symbol apart from an
/// unbound variable when checking that a keyword call to an equation supplies every unknown:
/// in `"y = x * 2.0 m"` the `m` is metres, not a variable the caller forgot to pass.
pub(crate) fn is_known_unit_symbol(name: &str) -> bool {
    unit_registry().get_unit(name).is_some()
}

/// True if a `unit_term` pair (e.g. "kg", "r ^ 2", "m2", "a0", "Å") names a registered
/// unit. The optional `^exp` suffix is irrelevant to the lookup.
///
/// A trailing digit run is ambiguous: in "m2" it is an embedded exponent (metre squared),
/// in "a0" it is part of the symbol itself (the Bohr radius). `physure_core`'s unit parser
/// already resolves that ambiguity, so try the whole symbol first and fall back to the
/// digit-stripped stem — matching what evaluation will do with the same text.
///
/// The scan must not be restricted to ASCII: `unit_term` accepts any Unicode `LETTER`, so
/// an ASCII-only check silently rejects registered symbols like "Å" and "°" and hands them
/// to the expression parser, where they are not valid identifiers either.
fn unit_term_is_registered(text: &str, registry: &UnitRegistry) -> bool {
    let symbol: String = text
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '°' | '%' | '$'))
        .collect();
    if symbol.is_empty() {
        return false;
    }
    if registry.get_unit(&symbol).is_some() {
        return true;
    }
    let stem = symbol.trim_end_matches(|c: char| c.is_ascii_digit());
    stem.len() < symbol.len() && !stem.is_empty() && registry.get_unit(stem).is_some()
}

/// True if every `unit_term` inside `pair` (recursing through parenthesized groups) names
/// a registered unit. Used to decide whether a group like `(kg * s^2)` is consumed
/// wholesale as part of a quantity's unit.
fn unit_expr_all_valid(pair: pest::iterators::Pair<Rule>, registry: &UnitRegistry) -> bool {
    pair.into_inner().all(|child| match child.as_rule() {
        Rule::unit_term => unit_term_is_registered(child.as_str(), registry),
        Rule::unit_expr => unit_expr_all_valid(child, registry),
        _ => false,
    })
}

/// Splits a `unit_expr` pair into the longest valid leading run of registered units
/// (returned as a unit string) plus, if that run doesn't consume the whole match, the
/// operator connecting it to the raw source text of everything after it.
///
/// `unit_expr` is purely syntactic — any `ASCII_ALPHA+` chain joined by `*`/`/`/whitespace
/// parses as one, so pest alone can't tell a real unit symbol from a variable reference.
/// Concretely, `1.602e-19 C / r ^ 2` on one line (no statement boundary before `/ r`) would
/// otherwise have its `unit_expr` greedily swallow `C / r ^ 2` as the literal's unit even
/// though `r` is a bound function parameter, fabricating a bogus `r` dimension and silently
/// dropping the real division at evaluation time. Validating against the registry here lets
/// the leftover be handed back to the expression grammar as a normal variable reference.
fn split_unit_expr(pair: pest::iterators::Pair<Rule>, registry: &UnitRegistry) -> (Option<String>, Option<(BinaryOp, String)>) {
    let base_str = pair.as_str();
    let base_start = pair.as_span().start();
    let mut consumed_end_rel = 0usize;

    for child in pair.into_inner() {
        let span = child.as_span();
        let valid = match child.as_rule() {
            Rule::unit_term => unit_term_is_registered(child.as_str(), registry),
            Rule::unit_expr => unit_expr_all_valid(child, registry),
            _ => false,
        };
        if valid {
            consumed_end_rel = span.end() - base_start;
            continue;
        }

        let child_start_rel = span.start() - base_start;
        let gap = &base_str[consumed_end_rel..child_start_rel];
        let op = if gap.contains('/') { BinaryOp::Div } else { BinaryOp::Mul };
        // Start the remainder right after the operator (if any) rather than at the
        // invalid child's own span start, so a parenthesized group's "(" — which lies in
        // `gap`, before the child's span — stays attached to the remainder text instead
        // of being dropped (e.g. "kg / (r^2)" must give back "(r^2)", not "r^2)").
        let op_offset = gap.find(|c| c == '*' || c == '/').map(|i| i + 1).unwrap_or(0);
        let remainder = base_str[consumed_end_rel + op_offset..].trim().to_string();
        let unit = if consumed_end_rel == 0 {
            None
        } else {
            Some(base_str[..consumed_end_rel].trim().to_string())
        };
        return (unit, Some((op, remainder)));
    }

    (Some(base_str.trim().to_string()), None)
}

/// Parses text left over after `split_unit_expr` stops consuming a unit chain (e.g.
/// "r ^ 2" or "(r^2)") as a normal expression, so it participates in the AST as a variable
/// reference / arithmetic instead of a fabricated unit dimension.
fn parse_unit_leftover(text: &str) -> PhysureResult<Expr> {
    let mut pairs = PhsParser::parse(Rule::expr, text)
        .map_err(|e| PhysureError::Generic(format!("Parse error in quantity unit remainder '{}': {}", text, e)))?;
    let pair = pairs
        .next()
        .ok_or_else(|| PhysureError::Generic(format!("Empty quantity unit remainder '{}'", text)))?;
    parse_expr(pair)
}

/// Reads one half of an uncertainty — `0.5`, `0.5%` or `2 sigma` — into its value and whether
/// it was written as a percentage. `is_sigma` is shared across the halves because it describes
/// the whole measurement, not one side of it.
fn parse_uncertainty_val(raw: &str, is_sigma: &mut bool) -> PhysureResult<(f64, bool)> {
    let mut val_str = raw.trim().to_string();
    let is_percent = val_str.ends_with('%');
    if is_percent {
        val_str.pop();
    }
    if val_str.contains("sigma") || val_str.contains("σ") {
        *is_sigma = true;
        val_str = val_str.replace("sigma", "").replace("σ", "");
    }
    let value = val_str
        .trim()
        .parse::<f64>()
        .map_err(|_| PhysureError::Generic("Invalid uncertainty".to_string()))?;
    Ok((value, is_percent))
}

pub(crate) fn parse_quantity(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut magnitude = None;
    let mut magnitude_expr = None;
    let mut unit = None;
    let mut unit_leftover: Option<(BinaryOp, String)> = None;

    let mut is_sigma = false;
    // (value, was written as a percentage), upper half first.
    let mut halves: Vec<(f64, bool)> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::number => {
                let s = inner.as_str().trim();
                let mag = match s {
                    "inf" | "+inf" | "infinity" | "+infinity" | "oo" | "+oo" | "∞" | "+∞" => f64::INFINITY,
                    "-inf" | "-infinity" | "-oo" | "-∞" => f64::NEG_INFINITY,
                    _ => s.parse::<f64>().map_err(|_| PhysureError::Generic(format!("Invalid number: {}", s)))?,
                };
                magnitude = Some(mag);
            }
            Rule::expr => {
                magnitude_expr = Some(parse_expr(inner)?);
            }
            Rule::uncertainty => {
                // One value for `+/- 0.5`, two for the asymmetric `+/- (0.5, 0.4)`, upper
                // first. `uncertainty_pair` nests them, so the halves are collected by rule
                // rather than by position.
                for u_inner in inner.into_inner() {
                    match u_inner.as_rule() {
                        Rule::uncertainty_val => halves.push(parse_uncertainty_val(u_inner.as_str(), &mut is_sigma)?),
                        Rule::uncertainty_pair => {
                            for half in u_inner.into_inner() {
                                if half.as_rule() == Rule::uncertainty_val {
                                    halves.push(parse_uncertainty_val(half.as_str(), &mut is_sigma)?);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Rule::unit_expr => {
                let (u, leftover) = split_unit_expr(inner, unit_registry());
                unit = u;
                unit_leftover = leftover;
            }
            _ => {}
        }
    }

    // `+/- 0.5%` is a *relative* uncertainty. The percent sign was stripped and then never
    // applied, so `9.81 +/- 0.5% m/s^2` claimed ±0.5 instead of ±0.049 — twenty times too
    // wide, and nothing in the output said so.
    if halves.iter().any(|(_, is_percent)| *is_percent) {
        let base = magnitude.or(match &magnitude_expr {
            Some(Expr::Quantity(q)) => Some(q.magnitude),
            _ => None,
        });
        match base {
            Some(mag) => {
                for (value, is_percent) in halves.iter_mut() {
                    if *is_percent {
                        *value = mag.abs() * *value / 100.0;
                    }
                }
            }
            // Anything else only knows its magnitude at run time, and a percentage of an
            // unknown is not a number: say so instead of inventing one.
            None => {
                return Err(PhysureError::Generic(
                    "A percent uncertainty needs a literal magnitude to apply to; use an absolute value like `+/- 0.05`".into(),
                ))
            }
        }
    }

    // `+/- (0.5, 0.4)` is +0.5 and -0.4: upper half first, in the order the operator reads.
    // That convention lives here and nowhere else — swapping these two lines (and the
    // matching note in `phs.pest`) is the whole change if the other order ever wins.
    let uncertainty = halves.first().map(|(v, _)| *v);
    let uncertainty_lower = halves.get(1).map(|(v, _)| *v);

    let quantity_expr = if let Some(mag) = magnitude {
        Expr::Quantity(QuantityNode {
            magnitude: mag,
            uncertainty,
            uncertainty_lower,
            is_sigma,
            unit,
        })
    } else if let Some(mag_expr) = magnitude_expr {
        if let Some(u) = unit {
            if let Expr::Quantity(mut q) = mag_expr {
                q.unit = Some(u);
                q.is_sigma = is_sigma;
                q.uncertainty = uncertainty.or(q.uncertainty);
                q.uncertainty_lower = uncertainty_lower.or(q.uncertainty_lower);
                Expr::Quantity(q)
            } else {
                Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(mag_expr),
                    right: Box::new(Expr::Identifier(u)),
                }
            }
        } else {
            mag_expr
        }
    } else {
        return Err(PhysureError::Generic("Missing magnitude in quantity".to_string()));
    };

    match unit_leftover {
        Some((op, remainder)) => {
            let remainder_expr = parse_unit_leftover(&remainder)?;
            Ok(Expr::BinaryOp {
                op,
                left: Box::new(quantity_expr),
                right: Box::new(remainder_expr),
            })
        }
        None => Ok(quantity_expr),
    }
}

