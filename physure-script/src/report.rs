//! The structured answer of a sandboxed run, for callers that are not a person at a terminal (an agent, an MCP
//! server): [`evaluate`] parses a program, runs the static checker, evaluates it statement by statement under the
//! [`Limits`] and a Monte Carlo seed, and returns a [`Report`] that serializes to one JSON document.

use crate::ast::Statement;
use crate::check::{check_program, Severity};
use crate::{run_sandboxed, Limits, PhsInterpreter, PhsValue};
use physure_core::units::conf::try_with_conf_registry;
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

/// [`evaluate`] with the loop-iteration and pipeline-step ceilings fixed for this run, opened on the thread that
/// evaluates (`settings::scoped_max_loop_iterations`, `settings::scoped_max_pipeline_steps`): the process-wide values,
/// which an activated configuration file sets from its `[Settings]`, do not apply. For text that comes with a file
/// received from elsewhere (a frozen file's witness scripts), whose settings must not raise its own bounds.
pub fn evaluate_with_ceilings(source: &str, limits: Limits, seed: u64, max_loop_iterations: usize, max_pipeline_steps: usize) -> Report {
    let source = source.to_string();
    run_sandboxed(limits, move |interp| {
        let _loops = physure_core::settings::scoped_max_loop_iterations(max_loop_iterations);
        let _steps = physure_core::settings::scoped_max_pipeline_steps(max_pipeline_steps);
        physure_core::uncertainty::with_seed(seed, || evaluate_in(&interp, &source))
    })
}

/// The first line of [`canonical_witness`]: the version of the format.
pub const WITNESS_FORMAT: &str = "phs-witness-1";

/// The canonical result of a witness script (format `phs-witness-1`), whose SHA-256 a frozen file records: what the
/// program computed, never how it was worded. One line per line of text, LF-terminated:
///
/// * `phs-witness-1`;
/// * `ok true` or `ok false`;
/// * per result item, in order: `item line=<n> name=<json> kind=<kind> value=<bits> uncertainty=<bits> unit=<json>
///   dimension=<json>`, where `<bits>` is the IEEE-754 bit pattern as 16 lowercase hexadecimal digits (non-finite
///   values included: they have bit patterns too) or `-` when absent, and `<json>` is a JSON string or `-`; a value
///   that has no number (a boolean, a string, a function, ...) adds ` text=<json>` (what the interpreter prints,
///   since nothing else says what it is); the elements of a vector follow as `element <depth> ...` lines in the same
///   form;
/// * per error finding, in order: `error <code>` (the code only: never the message, which may be reworded, and
///   never a warning).
pub fn canonical_witness(report: &Report) -> String {
    let mut text = format!("{WITNESS_FORMAT}\nok {}\n", report.ok);
    for item in &report.results {
        let name = item.name.as_deref().map_or_else(|| "-".to_string(), json_string);
        text.push_str(&format!("item line={} name={name} ", item.line));
        canonical_value(&item.value, 0, &mut text);
    }
    for finding in report.diagnostics.iter().filter(|d| d.severity == "error") {
        text.push_str(&format!("error {}\n", finding.code));
    }
    text
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

fn canonical_value(value: &Value, depth: usize, text: &mut String) {
    let bits = |x: Option<f64>| x.map_or_else(|| "-".to_string(), |x| format!("{:016x}", x.to_bits()));
    let words = |x: &Option<String>| x.as_deref().map_or_else(|| "-".to_string(), json_string);
    text.push_str(&format!(
        "kind={} value={} uncertainty={} unit={} dimension={}",
        value.kind,
        bits(value.value),
        bits(value.uncertainty),
        words(&value.unit),
        words(&value.dimension)
    ));
    if value.value.is_none() {
        text.push_str(&format!(" text={}", json_string(&value.text)));
    }
    text.push('\n');
    for element in value.items.iter().flatten() {
        text.push_str(&format!("element {} ", depth + 1));
        canonical_value(element, depth + 1, text);
    }
}

fn evaluate_in(interp: &PhsInterpreter, source: &str) -> Report {
    let program = match crate::parser::parse_phs(source) {
        Ok(program) => program,
        Err(error) => return syntax_report(error.to_string()),
    };
    with_current_catalog(|scoped| match scoped {
        Some(registry) => run_program(interp, &program, registry),
        None => try_with_conf_registry(|registry| run_program(interp, &program, registry))
            .unwrap_or_else(|e| configuration_report(e.to_string())),
    })
}

/// The report of a program that did not run because the configuration in force (a frozen one) cannot be used.
fn configuration_report(message: String) -> Report {
    Report { ok: false, results: Vec::new(), diagnostics: vec![Finding { line: 0, severity: "error", code: "configuration".into(), message }] }
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
    let mut diagnostics = static_findings(program, registry);
    // the interpreter refuses what the checker already reported: say it once
    let reported: HashSet<usize> = diagnostics.iter().filter(|d| d.severity == "error").map(|d| d.line).collect();
    let mut collected = Collected { results: Vec::new(), bytes: 0, cap: interp.limits.max_output_bytes };
    let mut env = HashMap::new();
    for (statement, &line) in program.statements.iter().zip(&program.lines) {
        let go_on = match interp.eval_statement_with_env(statement, &mut env) {
            Ok(PhsValue::None) => true,
            Ok(value) => collected.add(statement, line, &value, registry, &mut diagnostics),
            Err(failure) => record_failure(line, failure.to_string(), &reported, &mut diagnostics),
        };
        if !go_on {
            break;
        }
    }
    diagnostics.sort_by_key(|d| d.line);
    let ok = !diagnostics.iter().any(|d| d.severity == "error");
    Report { ok, results: collected.results, diagnostics }
}

/// What the static checker finds in the program, as report findings.
fn static_findings(program: &crate::ast::Program, registry: &UnitRegistry) -> Vec<Finding> {
    check_program(program, registry)
        .into_iter()
        .map(|d| Finding {
            line: d.line,
            severity: if d.severity == Severity::Error { "error" } else { "warning" },
            code: d.code.to_string(),
            message: d.message,
        })
        .collect()
}

/// The values rendered so far, and how much output they came to.
struct Collected {
    results: Vec<Item>,
    bytes: usize,
    cap: Option<usize>,
}

impl Collected {
    /// Adds the value of a statement; false when the output cap ends the run.
    fn add(&mut self, statement: &Statement, line: usize, value: &PhsValue, registry: &UnitRegistry, diagnostics: &mut Vec<Finding>) -> bool {
        let rendered = render(value, registry);
        self.bytes += rendered.text.len();
        if let Some(cap) = self.cap.filter(|cap| self.bytes > *cap) {
            diagnostics.push(error(line, "limit", format!("output limit of {cap} bytes reached; the results from line {line} on are omitted")));
            return false;
        }
        let name = match statement {
            Statement::Assignment(node) => Some(node.name.clone()),
            _ => None,
        };
        self.results.push(Item { line, name, value: rendered });
        true
    }
}

/// Records a statement's runtime error; false when it ends the run (a limit).
fn record_failure(line: usize, message: String, reported: &HashSet<usize>, diagnostics: &mut Vec<Finding>) -> bool {
    let code = classify(&message);
    if code == "runtime" && reported.contains(&line) {
        return true;
    }
    diagnostics.push(error(line, code, message));
    code != "limit"
}

fn error(line: usize, code: &str, message: String) -> Finding {
    Finding { line, severity: "error", code: code.to_string(), message }
}

fn classify(message: &str) -> &'static str {
    let ceilings = ["max_loop_iterations ceiling", "max_pipeline_steps ceiling"];
    if ["call depth limit", "stack limit", "time limit"].iter().chain(&ceilings).any(|prefix| message.contains(prefix)) {
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
    with_current_catalog(|scoped| match scoped {
        Some(registry) => Ok(render(&value, registry)),
        None => try_with_conf_registry(|registry| render(&value, registry)).map_err(|e| e.to_string()),
    })
}

/// What the core knows of a unit: its dimension, whether it is an absolute temperature scale, its shortest name.
pub fn describe_unit(symbol: &str) -> Result<UnitDescription, String> {
    check_unit_len(symbol)?;
    with_current_catalog(|scoped| match scoped {
        Some(registry) => describe(registry, symbol),
        None => try_with_conf_registry(|registry| describe(registry, symbol)).map_err(|e| e.to_string())?,
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
