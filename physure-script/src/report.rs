//! The structured answer of a sandboxed run, for callers that are not a person at a terminal (an agent, an MCP
//! server): [`evaluate`] parses a program, runs the static checker, evaluates it statement by statement under the
//! [`Limits`] and a Monte Carlo seed, and returns a [`Report`] that serializes to one JSON document.

use crate::ast::Statement;
use crate::check::{check_program, Severity};
use crate::{run_sandboxed, Limits, PhsInterpreter, PhsValue};
use physure_core::units::{formula_text, with_current_catalog, UnitKind, UnitRegistry};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// The seed `phs eval` uses when none is given: the same program gives the same answer.
pub const DEFAULT_SEED: u64 = 0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// False when any finding is an error.
    pub ok: bool,
    pub results: Vec<Item>,
    pub diagnostics: Vec<Finding>,
}

/// The value of one statement.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    /// 1-based line of the statement.
    pub line: usize,
    /// The variable an assignment sets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Value {
    /// `quantity`, `number`, `bool`, `string`, `vector`, `matrix`, `function`, `equation`, `range`, `plot` or `sigma`.
    pub kind: &'static str,
    /// What the interpreter prints for the value, complete.
    pub text: String,
    /// A quantity's magnitude, or a number, in the unit named by `unit` (`null` when it is not finite). The unit is
    /// the one the interpreter prints, an expression the unit parser reads back; `""` and dimension `1` for no unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// The physical dimension of a quantity as the core writes it: `L·T⁻¹`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    /// The elements of a vector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    /// 1-based, 0 when the source of the message has no line.
    pub line: usize,
    /// `error` or `warning`.
    pub severity: &'static str,
    /// The checker's code, or `syntax`, `runtime`, `limit` (depth, stack, time or output) or `forbidden`.
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitDescription {
    pub unit: String,
    pub dimension: String,
    /// `point` for an absolute temperature scale, `delta` for everything else.
    pub kind: &'static str,
    pub linear: bool,
    pub alias: String,
    pub simplified: String,
}

/// Runs `source` in a sandbox under `limits`, with Monte Carlo draws seeded by `seed`.
pub fn evaluate(source: &str, limits: Limits, seed: u64) -> Report {
    let source = source.to_string();
    run_sandboxed(limits, move |interp| physure_core::uncertainty::with_seed(seed, || evaluate_in(&interp, &source)))
}

fn evaluate_in(interp: &PhsInterpreter, source: &str) -> Report {
    let program = match crate::parser::parse_phs(source) {
        Ok(program) => program,
        Err(error) => return syntax_report(error.to_string()),
    };
    with_current_catalog(|scoped| match scoped {
        Some(registry) => run_program(interp, &program, registry),
        None => run_program(interp, &program, &physure_core::units::conf::build_registry_from_conf().0),
    })
}

fn syntax_report(message: String) -> Report {
    // pest writes the position as ` --> line:column`
    let line = message
        .split("--> ")
        .nth(1)
        .and_then(|rest| rest.split(':').next())
        .and_then(|number| number.trim().parse().ok())
        .unwrap_or(0);
    Report { ok: false, results: Vec::new(), diagnostics: vec![Finding { line, severity: "error", code: "syntax".into(), message }] }
}

fn run_program(interp: &PhsInterpreter, program: &crate::ast::Program, registry: &UnitRegistry) -> Report {
    let mut diagnostics: Vec<Finding> = check_program(program, registry)
        .into_iter()
        .map(|d| Finding {
            line: d.line,
            severity: if d.severity == Severity::Error { "error" } else { "warning" },
            code: d.code.to_string(),
            message: d.message,
        })
        .collect();
    // the interpreter refuses what the checker already reported: say it once
    let reported: HashSet<usize> = diagnostics.iter().filter(|d| d.severity == "error").map(|d| d.line).collect();
    let cap = interp.limits.max_output_bytes;
    let mut env = HashMap::new();
    let mut results = Vec::new();
    let mut bytes = 0usize;
    for (statement, &line) in program.statements.iter().zip(&program.lines) {
        match interp.eval_statement_with_env(statement, &mut env) {
            Ok(PhsValue::None) => {}
            Ok(value) => {
                let rendered = render(&value, registry);
                bytes += rendered.text.len();
                if let Some(cap) = cap.filter(|cap| bytes > *cap) {
                    diagnostics.push(error(line, "limit", format!("output limit of {cap} bytes reached; the results from line {line} on are omitted")));
                    break;
                }
                let name = match statement {
                    Statement::Assignment(node) => Some(node.name.clone()),
                    _ => None,
                };
                results.push(Item { line, name, value: rendered });
            }
            Err(failure) => {
                let message = failure.to_string();
                let code = classify(&message);
                if code == "runtime" && reported.contains(&line) {
                    continue;
                }
                diagnostics.push(error(line, code, message));
                if code == "limit" {
                    break;
                }
            }
        }
    }
    diagnostics.sort_by_key(|d| d.line);
    let ok = !diagnostics.iter().any(|d| d.severity == "error");
    Report { ok, results, diagnostics }
}

fn error(line: usize, code: &str, message: String) -> Finding {
    Finding { line, severity: "error", code: code.to_string(), message }
}

fn classify(message: &str) -> &'static str {
    if ["call depth limit", "stack limit", "time limit"].iter().any(|prefix| message.contains(prefix)) {
        "limit"
    } else if message.contains("forbidden:") {
        "forbidden"
    } else {
        "runtime"
    }
}

fn render(value: &PhsValue, registry: &UnitRegistry) -> Value {
    let plain = |kind: &'static str| Value { kind, text: value.to_string(), value: None, uncertainty: None, unit: None, dimension: None, items: None };
    match value {
        PhsValue::Quantity(quantity) => {
            let (magnitude, uncertainty, unit) = quantity.display_parts();
            // the display's name for no unit at all
            let unit = if unit == "Dimensionless" { String::new() } else { unit };
            let dimension = if unit.is_empty() { Some("1".to_string()) } else { registry.facts(&unit).ok().map(|facts| formula_text(&facts.dimension)) };
            Value { value: Some(magnitude), uncertainty: Some(uncertainty), unit: Some(unit), dimension, ..plain("quantity") }
        }
        PhsValue::Number(number) => Value { value: Some(*number), ..plain("number") },
        PhsValue::Bool(_) => plain("bool"),
        PhsValue::String(_) => plain("string"),
        PhsValue::Vector(elements) => {
            let items: Vec<Value> = elements.iter().map(|element| render(element, registry)).collect();
            let text = format!("[{}]", items.iter().map(|item| item.text.as_str()).collect::<Vec<_>>().join(", "));
            Value { text, items: Some(items), ..plain("vector") }
        }
        PhsValue::Matrix(_) => plain("matrix"),
        PhsValue::Function(_) => plain("function"),
        PhsValue::Equation(..) => plain("equation"),
        PhsValue::Range(..) => plain("range"),
        PhsValue::Plot(_) => plain("plot"),
        PhsValue::Sigma(_) | PhsValue::SigmaBound(..) => plain("sigma"),
        PhsValue::None => plain("none"),
    }
}

/// The longest unit expression `describe_unit` and `convert` read: a caller from outside gets no say in how long
/// the unit parser works.
pub const MAX_UNIT_LEN: usize = 200;

fn check_unit_len(unit: &str) -> Result<(), String> {
    if unit.len() > MAX_UNIT_LEN {
        return Err(format!("unit expression of {} bytes is too long (the limit is {MAX_UNIT_LEN})", unit.len()));
    }
    Ok(())
}

/// `value` `from` expressed in `to`, as the core converts it (offsets of absolute temperatures included).
pub fn convert(value: f64, from: &str, to: &str) -> Result<Value, String> {
    check_unit_len(from)?;
    check_unit_len(to)?;
    let converted = physure_core::quantity::Quantity::new(value, from).and_then(|quantity| quantity.to(to)).map_err(|error| error.to_string())?;
    let value = PhsValue::Quantity(converted);
    Ok(with_current_catalog(|scoped| match scoped {
        Some(registry) => render(&value, registry),
        None => render(&value, &physure_core::units::conf::build_registry_from_conf().0),
    }))
}

/// What the core knows of a unit: its dimension, whether it is an absolute temperature scale, its shortest name.
pub fn describe_unit(symbol: &str) -> Result<UnitDescription, String> {
    check_unit_len(symbol)?;
    with_current_catalog(|scoped| match scoped {
        Some(registry) => describe(registry, symbol),
        None => describe(&physure_core::units::conf::build_registry_from_conf().0, symbol),
    })
}

fn describe(registry: &UnitRegistry, symbol: &str) -> Result<UnitDescription, String> {
    let facts = registry.facts(symbol).map_err(|_| format!("unknown unit '{symbol}'"))?;
    Ok(UnitDescription {
        unit: symbol.to_string(),
        dimension: formula_text(&facts.dimension),
        kind: if facts.kind == UnitKind::Point { "point" } else { "delta" },
        linear: facts.is_linear,
        alias: facts.alias,
        simplified: facts.simplified,
    })
}
