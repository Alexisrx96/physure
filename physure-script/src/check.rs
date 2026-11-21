//! Static dimensional checking of PHS: say, without running the script, that a sum, a comparison, a
//! conversion or a call mixes incompatible dimensions.
//!
//! PHS evaluates dynamically: `m + s` is an error only when the line runs. This pass gives every expression
//! a dimension from what is written (quantity literals and their units, assignments, annotated parameters)
//! and checks the operations that the interpreter would refuse, with the same rules (`binary_ops.rs`,
//! `builtins/core.rs`, `bind_param_value`). The units are read through the core's `UnitRegistry::facts`,
//! so the checker and the interpreter cannot disagree about what a unit is.
//!
//! **Soundness.** An error is reported only when the interpreter is certain to refuse the operation: both
//! sides are known quantities and their dimensions differ, a unit is not in the catalog, a name is
//! neither a variable, a constant nor a unit and meets a quantity, a name of `pi`, `π` or a constant of the catalog
//! is bound (`crate::globals`). A value the pass cannot type (an argument of a
//! function it has not seen, a vector, an imported name) is `Unknown` and silences every check that depends
//! on it. `min` or `max` of different dimensions (`mixed-extremum`) was the one warning while the interpreter
//! compared their base-unit magnitudes; the interpreter refuses it now (unit holes), so it is an error like the
//! others, and no finding is a warning today.
//!
//! One exception to "certain to refuse": the body of a function is checked with what it knows of it (the units of
//! annotated parameters, the literals) even if the function is never called, so `fn f(a: m) = max(a, 1 s)` and
//! `fn g(b) = max(1 m, 1 s)` are errors at check time although the interpreter, which never runs the body, accepts
//! the program. The body would be refused at every call, so the finding is of the same kind as every other one the
//! checker reports in a function body.
//!
//! **Functions.** A parameter with a unit (`fn f(m: kg, v: m/s)`) is checked at every call and typed in
//! the body. A parameter without one takes the dimension of the argument at each call site: the body is
//! checked again for those dimensions (memoised, with a guard against recursion), and a mismatch inside it
//! is reported at the line of the body, with the call that made it.
//!
//! **Where.** The AST carries the line of each statement and no column, so a diagnostic points at a line.

use crate::ast::{AssignmentNode, BinaryOp, Expr, FunctionDefNode, ImportSpecifier, Program, QuantityNode, Statement};
use crate::interpreter::helpers::strip_unit_comment;
use num_rational::Rational64;
use physure_core::error::PhysureResult;
use physure_core::units::{formula_text, UnitKind, UnitRegistry};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The interpreter would stop here.
    Error,
    /// The interpreter accepts it and it is almost certainly not what was meant. No finding is a warning today
    /// (`mixed-extremum` was, until the interpreter refused it); the severity stays in the schema of the report, of
    /// `phs check` and of the LSP.
    Warning,
}

/// A finding, at the line of the statement it was found in (1-based; 0 for an AST built without lines).
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub line: usize,
    pub severity: Severity,
    /// A short stable name: `dimension-mismatch`, `unknown-unit`, `unknown-name`, `bad-argument`,
    /// `bad-exponent`, `temperature-points`, `mixed-extremum`, `constant-shadowing`, `reserved-name`, `not-a-bool`.
    pub code: &'static str,
    pub message: String,
}

/// Whether the checker finding (`code`, `finding_message`) is the same problem as this interpreter error: the two
/// word a problem differently, so the match is by the wording of the problem's kind and by what names it (the
/// unit, the function, the binding), never by the kind alone. A message this does not recognise as the finding's
/// own is another problem, so it is reported too; a reworded message would be reported twice, which the guard test
/// `a_flagged_line_is_not_said_twice` (`tests/report.rs`) catches.
pub fn explains(code: &str, finding_message: &str, runtime_message: &str) -> bool {
    match code {
        "dimension-mismatch" => ["Unit mismatch", "with the plain number"].iter().any(|kind| runtime_message.contains(kind)),
        "bad-argument" => is_bad_argument(finding_message, runtime_message),
        // the same unit or name by name: `Unknown unit 'a'` does not say `'fooz' is neither a variable, a constant nor a unit`
        "unknown-unit" | "unknown-name" => between(finding_message, '\'').is_some_and(|name| {
            runtime_message.contains(&format!("Unknown unit '{name}'")) || runtime_message.contains(&format!("`{name}` has no value"))
        }),
        "bad-exponent" => runtime_message.contains("Exponent must be"),
        // the same function: `max of different dimensions (L, T): ...` and `` max of different dimensions: `1 m` is ... ``
        "mixed-extremum" => finding_message
            .split(' ')
            .next()
            .is_some_and(|function| runtime_message.contains(&format!("{function} of different dimensions:"))),
        // the same operation on the temperatures
        "temperature-points" => [
            ("cannot be added", "cannot add two absolute temperatures"),
            ("multiplied or divided", "cannot multiply or divide the absolute temperature"),
        ]
        .iter()
        .any(|(finding, runtime)| finding_message.contains(finding) && runtime_message.contains(runtime)),
        "not-a-bool" => condition_head(finding_message) == condition_head(runtime_message),
        // the same name, in the words both messages take from one place (`reserved.rs`, `globals.rs`)
        "reserved-name" | "constant-shadowing" => between(finding_message, '`')
            .is_some_and(|name| runtime_message.contains(&format!("`{name}` is ")) && runtime_message.contains("choose another")),
        _ => false,
    }
}

/// The first text between two `quote` characters of `message`.
fn between(message: &str, quote: char) -> Option<&str> {
    message.split(quote).nth(1).filter(|name| !name.is_empty())
}

/// A builtin that wants an angle or a dimensionless value (`sin expects an angle or a dimensionless value, got L`;
/// the interpreter says `exp expects a dimensionless value, got 's'`), or an annotated parameter that got another
/// unit (`the argument for parameter 'm' of 'kinetic' is T, ...` and `Argument for parameter 'm' of function
/// 'kinetic' has a unit incompatible ...`): the same function or the same parameter of the same function.
fn is_bad_argument(finding_message: &str, runtime_message: &str) -> bool {
    if let Some(function) = finding_message.split(" expects ").next().filter(|function| !function.contains(' ') && function.len() < finding_message.len()) {
        let wants = [" expects an angle or a dimensionless value", " expects a dimensionless value"];
        return wants.iter().any(|want| runtime_message.starts_with(&format!("{function}{want}")));
    }
    let mut names = finding_message.split('\'').skip(1).step_by(2);
    match (names.next(), names.next()) {
        (Some(parameter), Some(function)) => runtime_message.contains(&format!("Argument for parameter '{parameter}' of function '{function}'")),
        _ => false,
    }
}

/// What a condition message says before its detail: the construct and the kind of value, without the value
/// (`(5.0)`, `(of dimension L)`) or the advice after the colon.
fn condition_head(message: &str) -> String {
    let mut head = String::new();
    let mut depth = 0usize;
    for c in message.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => break,
            _ if depth == 0 => head.push(c),
            _ => {}
        }
    }
    head.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses `code` and checks it against the catalog that unit text is read against on this thread (the
/// embedded one, plus a `physure.conf` in the working directory, unless a `units::with_catalog` scope is set).
pub fn check_source(code: &str) -> PhysureResult<Vec<Diagnostic>> {
    let program = crate::parser::parse_phs(code)?;
    physure_core::units::with_current_catalog(|scoped| match scoped {
        Some(registry) => Ok(check_program(&program, registry)),
        None => physure_core::units::conf::try_with_conf_registry(|registry| check_program(&program, registry))
            .map_err(|e| physure_core::error::PhysureError::Generic(e.to_string())),
    })
}

/// Checks a parsed program against `registry`.
pub fn check_program(program: &Program, registry: &UnitRegistry) -> Vec<Diagnostic> {
    let mut checker = Checker::new(registry);
    checker.refuse_reserved_bindings(program);
    checker.prepare(&program.statements);
    let mut env = Env::new();
    checker.block(&program.statements, &program.lines, &mut env, 0);
    checker.diagnostics
}

// ---- dimensions

/// A dimension by dimension symbol (`L`, `M`, `T`, `A`...), whether it is an absolute temperature, whether it is a
/// level (`dB`, `pH`: a logarithmic unit), and the value when it is a dimensionless literal (an exponent is read from
/// it).
#[derive(Clone, Debug, PartialEq, Default)]
struct Dim {
    exps: BTreeMap<String, Rational64>,
    point: bool,
    level: bool,
    constant: Option<f64>,
}

impl Dim {
    fn dimensionless() -> Dim {
        Dim::default()
    }

    fn same_dimension(&self, other: &Dim) -> bool {
        self.exps == other.exps
    }

    /// The same dimension once the angle is set aside: what a conversion may read as 1 in `Dimensionless` mode.
    fn same_apart_from_angle(&self, other: &Dim) -> bool {
        let without_angle = |dim: &Dim| -> BTreeMap<String, Rational64> {
            dim.exps.iter().filter(|(symbol, _)| symbol.as_str() != "A").map(|(symbol, power)| (symbol.clone(), *power)).collect()
        };
        without_angle(self) == without_angle(other)
    }

    fn combine(&self, other: &Dim, sign: i64) -> Dim {
        let mut exps = self.exps.clone();
        for (symbol, power) in &other.exps {
            let entry = exps.entry(symbol.clone()).or_insert_with(|| Rational64::from_integer(0));
            *entry += *power * Rational64::from_integer(sign);
        }
        exps.retain(|_, power| *power != Rational64::from_integer(0));
        Dim { exps, ..Dim::default() }
    }

    fn pow(&self, power: Rational64) -> Dim {
        let exps = self.exps.iter().map(|(symbol, p)| (symbol.clone(), *p * power)).collect();
        Dim { exps, ..Dim::default() }
    }

    /// Dimensionless, or an angle: what `sin`, `cos`, `tan`, `exp`, `ln` and `log` take.
    fn is_plain_or_angle(&self) -> bool {
        self.exps.is_empty() || (self.exps.len() == 1 && self.exps.get("A") == Some(&Rational64::from_integer(1)))
    }

    /// Dimensionless and not a level: a plain number, or a ratio in a unit (`%`, `unity`). What compares with a
    /// plain number.
    fn is_ratio(&self) -> bool {
        self.exps.is_empty() && !self.level
    }

    /// What a comparison message calls it: a level or an absolute temperature by what it is, anything else by its
    /// dimension.
    fn described(&self) -> String {
        match (self.level, self.point) {
            (true, _) if self.exps.is_empty() => "a level".to_string(),
            (true, _) => format!("a level of {}", self.text()),
            (_, true) => "an absolute temperature".to_string(),
            _ => self.text(),
        }
    }

    fn text(&self) -> String {
        if self.exps.is_empty() {
            return "dimensionless".to_string();
        }
        let pairs: Vec<(String, (i64, i64))> =
            self.exps.iter().map(|(symbol, power)| (symbol.clone(), (*power.numer(), *power.denom()))).collect();
        formula_text(&pairs)
    }
}

/// What the pass knows of a value.
#[derive(Clone, Debug, PartialEq)]
enum Ty {
    Unknown,
    /// A number or a quantity, with its dimension.
    Qty(Dim),
    Bool,
    /// Text, with its content when it was written as a literal.
    Text(Option<String>),
    /// A vector literal `[...]` without a unit.
    Vector,
    /// A bare word that is not bound, not imported and not a unit: the interpreter refuses it (`` `foo` has no
    /// value ``).
    Symbol(String),
}

impl Ty {
    fn key(&self) -> String {
        match self {
            Ty::Unknown => "?".to_string(),
            Ty::Qty(dim) => format!("{}{}{}", dim.text(), if dim.point { "@" } else { "" }, if dim.level { "~" } else { "" }),
            Ty::Bool => "bool".to_string(),
            Ty::Text(_) => "text".to_string(),
            Ty::Vector => "vector".to_string(),
            Ty::Symbol(name) => format!("sym:{name}"),
        }
    }
}

/// A function the pass has seen defined.
struct Func {
    node: FunctionDefNode,
    /// The dimension of each annotated parameter, `None` where there is no annotation (or it was invalid).
    declared: Vec<Option<Dim>>,
    implicit_units: bool,
    /// What the body yields for the declared dimensions (`Unknown` while it is being checked).
    returns: RefCell<Ty>,
}

/// The variables in scope and the type of each.
type Env = HashMap<String, Ty>;

const MAX_DEPTH: usize = 6;

struct Checker<'a> {
    registry: &'a UnitRegistry,
    diagnostics: Vec<Diagnostic>,
    seen: HashSet<(usize, &'static str, String)>,
    /// Every name bound anywhere in the program: an identifier among them may be shadowed by the time it runs.
    assigned: HashSet<String>,
    /// A wildcard or module import: names may come from a file the pass does not read.
    dynamic_names: bool,
    line: usize,
    /// ` (called as f(...) at line N)` while a body is checked for the dimensions of a call.
    context: String,
    memo: HashMap<String, Ty>,
    active: Vec<String>,
    /// The functions defined so far, by name: a body sees the ones defined before it was reached.
    funcs: HashMap<String, Rc<Func>>,
}

impl<'a> Checker<'a> {
    fn new(registry: &'a UnitRegistry) -> Self {
        Checker {
            registry,
            diagnostics: Vec::new(),
            seen: HashSet::new(),
            assigned: HashSet::new(),
            dynamic_names: false,
            line: 0,
            context: String::new(),
            memo: HashMap::new(),
            active: Vec::new(),
            funcs: HashMap::new(),
        }
    }

    // ---- the names a program binds

    fn prepare(&mut self, statements: &[Statement]) {
        for statement in statements {
            match statement {
                Statement::Import(import) => match &import.specifier {
                    ImportSpecifier::Symbols(symbols) => {
                        for symbol in symbols {
                            self.assigned.insert(symbol.alias.clone().unwrap_or_else(|| symbol.name.clone()));
                        }
                    }
                    ImportSpecifier::Wildcard | ImportSpecifier::ModuleAlias(_) => self.dynamic_names = true,
                },
                Statement::FunctionDef(function) => {
                    self.assigned.insert(function.name.clone());
                    self.assigned.extend(function.params.iter().cloned());
                    self.prepare(&function.body_stmts);
                }
                Statement::Assignment(AssignmentNode { name, value, .. }) => {
                    self.assigned.insert(name.clone());
                    self.prepare_expr(value);
                }
                Statement::While { body, cond, .. } => {
                    self.prepare_expr(cond);
                    self.prepare(body);
                }
                Statement::Expr(e) | Statement::Return(e) => self.prepare_expr(e),
                Statement::GuardReturn { cond, value } => {
                    self.prepare_expr(cond);
                    self.prepare_expr(value);
                }
                Statement::Export(_) => {}
            }
        }
    }

    fn prepare_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::BinaryOp { left, right, .. } => {
                self.prepare_expr(left);
                self.prepare_expr(right);
            }
            Expr::FunctionCall { args, kwargs, .. } => {
                args.iter().for_each(|a| self.prepare_expr(a));
                kwargs.iter().for_each(|(_, a)| self.prepare_expr(a));
            }
            Expr::ForExpr { var, iterable, body } => {
                self.assigned.insert(var.clone());
                self.prepare_expr(iterable);
                self.prepare_expr(body);
            }
            Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => {}
        }
    }

    // ---- global identifiers

    /// A binding of a reserved name anywhere in a statement (function bodies included) is an error, as the
    /// interpreter says before the statement runs (`crate::globals`, `crate::reserved`): `pi`, `π` and the
    /// constants of the catalog (`constant-shadowing`), a statement word, a literal, a function of the core, and a
    /// function of a module the program has imported by then (`reserved-name`). The top-level statements are read
    /// in order, keeping what the imports have unlocked and the names the program holds, so an import over a held
    /// name or a remap is reported too. Each finding is at the line of the top-level statement that holds it.
    ///
    /// It does not follow an import nested in a function or a loop body (whether it runs is not known statically), a
    /// name a `.phs` import binds, or an assignment inside a loop; so it can miss what the interpreter refuses, and it
    /// does not report what the interpreter accepts.
    fn refuse_reserved_bindings(&mut self, program: &Program) {
        let mut unlocked = crate::reserved::Unlocked::new();
        let mut held: HashSet<String> = HashSet::new();
        for (index, statement) in program.statements.iter().enumerate() {
            self.line = program.lines.get(index).copied().unwrap_or(0);
            let mut bound = Vec::new();
            crate::globals::bindings_of(statement, &mut bound);
            for (name, what) in bound {
                if let Some(kind) = crate::reserved::static_kind(&name) {
                    self.report(Severity::Error, "reserved-name", crate::reserved::binding_error(&name, &what, kind));
                } else if crate::globals::is_global_in(&name, self.registry) {
                    let value = self.registry.constants_meta.get(&name).map(|meta| meta.value.clone());
                    let message = crate::globals::shadowing_error(&name, &what, value.as_deref());
                    self.report(Severity::Error, "constant-shadowing", message);
                } else if !what.starts_with(crate::globals::IMPORT_BINDING) {
                    if let Some((domain, _)) = unlocked.get(&name) {
                        self.report(Severity::Error, "reserved-name", crate::reserved::module_binding_error(&name, domain, &what));
                    }
                }
            }
            if let Statement::Import(import) = statement {
                if let (Some(domain), Some(members)) = (crate::reserved::domain_of(&import.path), crate::builtins::domain_members(&import.path)) {
                    match crate::reserved::plan_import(import, domain, members, &|name| held.contains(name), &unlocked) {
                        Ok(imports) => unlocked.extend(imports),
                        Err(crate::reserved::ImportError::Reserved(message)) => self.report(Severity::Error, "reserved-name", message),
                        // the interpreter says so when it runs the import; the checker has no code for it
                        Err(crate::reserved::ImportError::NoSuchFunction(_)) => {}
                    }
                }
            }
            held.extend(bound_at_top_level(statement).map(str::to_string));
        }
        self.line = 0;
    }

    /// The type of the global identifier `name` (`pi`, `π` or a constant of the catalog), `None` when it is not one.
    fn global(&mut self, name: &str) -> Option<Ty> {
        if crate::globals::PI_NAMES.contains(&name) {
            return Some(Ty::Qty(Dim { constant: Some(std::f64::consts::PI), ..Dim::dimensionless() }));
        }
        let value = self.registry.constants_meta.get(name)?.value.clone();
        // the interpreter reads it with `constant_quantity`; one that does not read is its error, not a type
        if physure_core::units::conf::constant_quantity(&value, self.registry).is_err() {
            return Some(Ty::Unknown);
        }
        let unit = value.trim().split_once(char::is_whitespace).map_or("", |(_, unit)| unit.trim());
        if unit.is_empty() {
            return Some(Ty::Qty(Dim::dimensionless()));
        }
        Some(match self.registry.facts(unit) {
            Ok(facts) => Ty::Qty(dim_of(&facts)),
            Err(_) => Ty::Unknown,
        })
    }

    // ---- reporting

    fn report(&mut self, severity: Severity, code: &'static str, message: String) {
        let message = format!("{message}{}", self.context);
        if self.seen.insert((self.line, code, message.clone())) {
            self.diagnostics.push(Diagnostic { line: self.line, severity, code, message });
        }
    }

    // ---- units

    /// The dimension of the unit text of a literal, an annotation or a conversion target. An unknown unit is
    /// reported and is `None`.
    fn unit_dim(&mut self, text: &str) -> Option<Dim> {
        let clean = strip_unit_comment(text);
        if clean.is_empty() {
            return Some(Dim::dimensionless());
        }
        match self.registry.facts(clean) {
            Ok(facts) => Some(dim_of(&facts)),
            Err(error) => {
                self.report(Severity::Error, "unknown-unit", format!("{error}"));
                None
            }
        }
    }

    // ---- statements

    /// Checks a block and returns the type it yields (the last expression, and every `return`, if they agree).
    fn block(&mut self, statements: &[Statement], lines: &[usize], env: &mut Env, depth: usize) -> Ty {
        let mut yielded: Vec<Ty> = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            if let Some(line) = lines.get(index) {
                self.line = *line;
            }
            let is_last = index + 1 == statements.len();
            if let Some(ty) = self.statement(statement, is_last, env, depth) {
                yielded.push(ty);
            }
        }
        join(yielded)
    }

    /// Checks one statement; the type it yields to its block, if it yields one.
    fn statement(&mut self, statement: &Statement, is_last: bool, env: &mut Env, depth: usize) -> Option<Ty> {
        match statement {
            Statement::Assignment(node) => {
                let ty = self.expr(&node.value, env, depth);
                env.insert(node.name.clone(), ty);
                None
            }
            Statement::Expr(e) => {
                let ty = self.expr(e, env, depth);
                is_last.then_some(ty)
            }
            Statement::Return(e) => Some(self.expr(e, env, depth)),
            Statement::GuardReturn { cond, value } => {
                let ty = self.expr(cond, env, depth);
                self.condition(&ty, "the condition of `if ... then return`");
                Some(self.expr(value, env, depth))
            }
            Statement::While { cond, body, body_lines } => {
                self.while_loop(cond, body, body_lines, env, depth);
                None
            }
            Statement::FunctionDef(node) => {
                // a variable of the same name no longer is what a call reaches
                env.remove(&node.name);
                self.define(node, depth);
                None
            }
            Statement::Import(import) => {
                bind_imported(&import.specifier, env);
                None
            }
            Statement::Export(_) => None,
        }
    }

    fn while_loop(&mut self, cond: &Expr, body: &[Statement], body_lines: &[usize], env: &mut Env, depth: usize) {
        let ty = self.expr(cond, env, depth);
        self.condition(&ty, "the condition of `while`");
        let mut inner = env.clone();
        self.block(body, body_lines, &mut inner, depth);
        // the body may or may not have run: what it assigned is no longer known
        let mut bound = HashSet::new();
        collect_assigned(body, &mut bound);
        for name in bound {
            env.insert(name, Ty::Unknown);
        }
    }

    /// Registers a function and checks its body once, for the dimensions it declares.
    fn define(&mut self, node: &FunctionDefNode, depth: usize) {
        let declared: Vec<Option<Dim>> = node
            .params
            .iter()
            .enumerate()
            .map(|(i, _)| node.param_units.get(i).and_then(|u| u.as_deref()).and_then(|text| self.unit_dim(text)))
            .collect();
        let implicit_units = node.decorators.iter().any(|d| d.name == "implicit_units");
        let func = Rc::new(Func { node: node.clone(), declared, implicit_units, returns: RefCell::new(Ty::Unknown) });
        self.funcs.insert(node.name.clone(), func.clone());

        let types: Vec<Ty> = func.declared.iter().map(|d| d.clone().map(Ty::Qty).unwrap_or(Ty::Unknown)).collect();
        let returns = self.check_body(&func, &types, depth);
        *func.returns.borrow_mut() = returns;
    }

    /// Checks the body of `func` with `types` for its parameters and returns what it yields.
    fn check_body(&mut self, func: &Func, types: &[Ty], depth: usize) -> Ty {
        let mut body_env = Env::new();
        for (name, ty) in func.node.params.iter().zip(types) {
            body_env.insert(name.clone(), ty.clone());
        }
        let outer_line = self.line;
        // `@requires` sees the parameters, `@ensures` also `result`; both run where the function is called, and
        // are reported at the line of the definition (or of the call, for a body checked for a call's dimensions)
        let contracts: Vec<(&str, &Expr)> = func
            .node
            .decorators
            .iter()
            .filter(|d| d.name == "requires" || d.name == "ensures")
            .filter_map(|d| d.args.first().map(|cond| (d.name.as_str(), cond)))
            .collect();
        for (name, cond) in contracts.iter().filter(|(name, _)| *name == "requires") {
            let ty = self.expr(cond, &body_env, depth);
            self.condition(&ty, &format!("the condition of `@{name}`"));
        }
        let result = self.block(&func.node.body_stmts, &func.node.body_lines, &mut body_env, depth);
        self.line = outer_line;
        if contracts.iter().any(|(name, _)| *name == "ensures") {
            let mut result_env = body_env.clone();
            result_env.insert("result".to_string(), result.clone());
            for (name, cond) in contracts.iter().filter(|(name, _)| *name == "ensures") {
                let ty = self.expr(cond, &result_env, depth);
                self.condition(&ty, &format!("the condition of `@{name}`"));
            }
        }
        result
    }

    /// A condition (of `if`, `? :`, `while`, a guard, a contract, and the operands of `and`, `or`, `not`) whose type
    /// is known and is not a Bool is the interpreter's type error (`helpers::not_a_bool`), in the same words. An
    /// unknown type says nothing.
    fn condition(&mut self, ty: &Ty, context: &str) {
        use crate::interpreter::helpers::{not_a_bool_statically, KnownCondition};
        let known = match ty {
            Ty::Unknown | Ty::Bool => return,
            Ty::Qty(dim) if dim.is_ratio() && !dim.point => KnownCondition::Number,
            Ty::Qty(dim) => {
                let unit = if dim.level || dim.point || dim.exps.is_empty() { None } else { self.unit_of(dim) };
                let described = if dim.level || dim.point { dim.described() } else { format!("of dimension {}", dim.text()) };
                KnownCondition::Quantity { described, unit }
            }
            Ty::Text(text) => KnownCondition::Text(text.as_deref()),
            Ty::Vector => KnownCondition::Vector,
            Ty::Symbol(name) => KnownCondition::Unbound(name),
        };
        let message = not_a_bool_statically(context, known);
        self.report(Severity::Error, "not-a-bool", message);
    }

    // ---- expressions

    fn expr(&mut self, expr: &Expr, env: &Env, depth: usize) -> Ty {
        match expr {
            Expr::Quantity(node) => self.quantity(node),
            Expr::Bool(_) => Ty::Bool,
            Expr::Str(text) => Ty::Text(Some(text.clone())),
            Expr::Identifier(name) => self.identifier(name, env),
            Expr::BinaryOp { op, left, right } => self.binary(*op, left, right, env, depth),
            Expr::FunctionCall { name, args, kwargs } => self.call(name, args, kwargs, env, depth),
            Expr::ForExpr { iterable, body, var } => {
                self.expr(iterable, env, depth);
                let mut inner = env.clone();
                inner.insert(var.clone(), Ty::Unknown);
                self.expr(body, &inner, depth);
                Ty::Unknown
            }
        }
    }

    fn quantity(&mut self, node: &QuantityNode) -> Ty {
        if node.is_sigma || node.uncertainty_lower.is_some() {
            return Ty::Unknown;
        }
        let unit = node.unit.as_deref().map(strip_unit_comment).unwrap_or("");
        if unit.is_empty() {
            let constant = if node.uncertainty.is_none() { Some(node.magnitude) } else { None };
            return Ty::Qty(Dim { constant, ..Dim::dimensionless() });
        }
        match self.unit_dim(unit) {
            Some(dim) => Ty::Qty(dim),
            None => Ty::Unknown,
        }
    }

    fn identifier(&mut self, name: &str, env: &Env) -> Ty {
        if name.starts_with('`') || (name.contains('{') && name.contains('}')) {
            return Ty::Unknown;
        }
        if let Some(ty) = env.get(name) {
            return ty.clone();
        }
        if self.funcs.contains_key(name) {
            return Ty::Unknown;
        }
        // `pi` and the constants of the catalog come before units, and a binding of one is an error (reported
        // apart), so no later binding can make it something else
        if let Some(ty) = self.global(name) {
            return ty;
        }
        // bound somewhere (a later assignment, an import, a parameter of another function): it may be
        // bound by the time this runs, and a binding beats a unit
        if self.assigned.contains(name) || self.dynamic_names {
            return Ty::Unknown;
        }
        if self.registry.get_unit(name).is_some() {
            return match self.registry.facts(name) {
                Ok(facts) => Ty::Qty(dim_of(&facts)),
                Err(_) => Ty::Unknown,
            };
        }
        Ty::Symbol(name.to_string())
    }

    fn binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, env: &Env, depth: usize) -> Ty {
        if op == BinaryOp::Convert {
            let from = self.expr(left, env, depth);
            let target_text = crate::codegen::expr_to_unit_string(right);
            let clean = strip_unit_comment(&target_text);
            if clean.is_empty() {
                return from;
            }
            let Some(target) = self.unit_dim(clean) else { return Ty::Unknown };
            // a Bool or text has no unit to convert: the interpreter refuses it (`convert_value_to_unit`)
            let no_unit = match &from {
                Ty::Bool => Some("a Bool"),
                Ty::Text(_) => Some("a String"),
                _ => None,
            };
            if let Some(kind) = no_unit {
                self.report(
                    Severity::Error,
                    "bad-argument",
                    format!("cannot convert {kind} to `{clean}`: only a quantity or a number, or a vector, matrix or range of them, has a unit to convert"),
                );
                return Ty::Unknown;
            }
            // only a quantity has a dimension to compare; a name with no value (`Ty::Symbol`) is refused by the
            // interpreter before the conversion (`` `foo` has no value ``), and this pass says nothing of it here
            if let Ty::Qty(dim) = &from {
                let angle_only = physure_core::units::angle_mode::angle_mode()
                    == physure_core::units::angle_mode::AngleMode::Dimensionless
                    && dim.same_apart_from_angle(&target);
                if !dim.same_dimension(&target) && !angle_only {
                    self.report(
                        Severity::Error,
                        "dimension-mismatch",
                        format!("cannot convert {} to '{}' ({})", dim.text(), clean, target.text()),
                    );
                }
            }
            return Ty::Qty(Dim { constant: None, ..target });
        }
        let l = self.expr(left, env, depth);
        let r = self.expr(right, env, depth);
        if op == BinaryOp::Range {
            return Ty::Unknown;
        }
        match (&l, &r) {
            (Ty::Qty(a), Ty::Qty(b)) => self.arithmetic(op, a, b),
            (Ty::Qty(_), Ty::Symbol(name)) | (Ty::Symbol(name), Ty::Qty(_)) => {
                self.unknown_name(name);
                Ty::Unknown
            }
            _ => Ty::Unknown,
        }
    }

    fn unknown_name(&mut self, name: &str) {
        self.report(
            Severity::Error,
            "unknown-name",
            format!("'{name}' is neither a variable, a constant nor a unit, and meets a quantity here"),
        );
    }

    fn arithmetic(&mut self, op: BinaryOp, a: &Dim, b: &Dim) -> Ty {
        match op {
            BinaryOp::Add | BinaryOp::Sub => self.add_sub(op, a, b),
            BinaryOp::Mul | BinaryOp::Div => self.mul_div(op, a, b),
            BinaryOp::Pow => self.power(a, b),
            BinaryOp::Convert | BinaryOp::Range => Ty::Unknown,
        }
    }

    fn add_sub(&mut self, op: BinaryOp, a: &Dim, b: &Dim) -> Ty {
        let word = if op == BinaryOp::Add { "add" } else { "subtract" };
        if !a.same_dimension(b) {
            self.report(
                Severity::Error,
                "dimension-mismatch",
                format!("cannot {word}: the left side is {}, the right side is {}", a.text(), b.text()),
            );
            return Ty::Unknown;
        }
        let Some(point) = self.temperature_point(op, a.point, b.point) else {
            return Ty::Unknown;
        };
        let constant = match (a.constant, b.constant, op) {
            (Some(x), Some(y), BinaryOp::Add) => Some(x + y),
            (Some(x), Some(y), _) => Some(x - y),
            _ => None,
        };
        // the sum of two levels is a level (of the summed ratios), as the core reads it
        Ty::Qty(Dim { exps: a.exps.clone(), point, level: a.level && b.level, constant })
    }

    /// Whether the result of a sum or difference is an absolute temperature; `None` when the operands may not be
    /// combined at all (after reporting) or when the checker has no opinion. `point` is a unit with a zero point
    /// (`degC`): a `K` here may hold a point at run time (`T = 20 degC => K`) or not (`300 K`), so a difference with
    /// one is not reported (`T - 0 degC` is an interval when `T` is a point, refused when it is not) and its result
    /// is of no known kind (`20 degC - T` is a point or an interval).
    fn temperature_point(&mut self, op: BinaryOp, left_point: bool, right_point: bool) -> Option<bool> {
        match (op, left_point, right_point) {
            (BinaryOp::Add, true, true) => {
                self.report(
                    Severity::Error,
                    "temperature-points",
                    "two absolute temperatures cannot be added: one of them is an interval (K, delta_celsius)".to_string(),
                );
                None
            }
            // accepted only when the K is a point, and then an interval
            (BinaryOp::Sub, false, true) => Some(false),
            (BinaryOp::Sub, true, false) => None,
            (BinaryOp::Add, true, false) | (BinaryOp::Add, false, true) => Some(true),
            _ => Some(false),
        }
    }

    fn mul_div(&mut self, op: BinaryOp, a: &Dim, b: &Dim) -> Ty {
        // An exact sign mirrors an absolute temperature in its own scale, as the core does (`-10 degC`, which
        // the parser reads as `-1 * 10 degC`, and `-t`); a point is never a divisor.
        let is_sign = |d: &Dim| d.exps.is_empty() && matches!(d.constant, Some(c) if c.abs() == 1.0);
        if (a.point && is_sign(b)) || (b.point && is_sign(a) && op == BinaryOp::Mul) {
            return Ty::Qty(Dim { constant: None, ..(if a.point { a } else { b }).clone() });
        }
        // A level times a plain number, or over one, stays a level in the core (`5 dB * 2`, `2 * 5 dB`, and `-5 dB`,
        // which the parser reads as `-1 * 5 dB`), and keeps its unit name (`-5 dB` prints `-5.0 dB`).
        if a.level && b.is_ratio() {
            return Ty::Qty(Dim { constant: None, ..a.clone() });
        }
        if b.level && a.is_ratio() && op == BinaryOp::Mul {
            return Ty::Qty(Dim { constant: None, ..b.clone() });
        }
        if (a.point && b.exps.is_empty()) || (b.point && a.exps.is_empty()) {
            self.report(
                Severity::Error,
                "temperature-points",
                "an absolute temperature cannot be multiplied or divided by a plain number: the result depends on the scale".to_string(),
            );
            return Ty::Unknown;
        }
        let sign = if op == BinaryOp::Mul { 1 } else { -1 };
        let mut result = a.combine(b, sign);
        result.constant = match (a.constant, b.constant) {
            (Some(x), Some(y)) if op == BinaryOp::Mul => Some(x * y),
            (Some(x), Some(y)) if y != 0.0 => Some(x / y),
            _ => None,
        };
        Ty::Qty(result)
    }

    fn power(&mut self, a: &Dim, b: &Dim) -> Ty {
        if !b.exps.is_empty() {
            self.report(
                Severity::Error,
                "bad-exponent",
                format!("an exponent must be a dimensionless constant, this one is {}", b.text()),
            );
            return Ty::Unknown;
        }
        match b.constant.and_then(as_rational) {
            Some(power) => {
                let mut result = a.pow(power);
                result.constant = a.constant.map(|x| x.powf(*power.numer() as f64 / *power.denom() as f64));
                Ty::Qty(result)
            }
            None if a.exps.is_empty() => Ty::Qty(Dim::dimensionless()),
            None => Ty::Unknown,
        }
    }

    // ---- calls

    fn call(&mut self, name: &str, args: &[Expr], kwargs: &[(String, Expr)], env: &Env, depth: usize) -> Ty {
        if name == "let" && args.len() == 3 {
            if let Expr::Identifier(var) = &args[0] {
                let bound = self.expr(&args[1], env, depth);
                let mut inner = env.clone();
                inner.insert(var.clone(), bound);
                return self.expr(&args[2], &inner, depth);
            }
        }
        let tys: Vec<Ty> = args.iter().map(|a| self.expr(a, env, depth)).collect();
        // a vector literal (`[1, 2]` is the call `vector(1, 2)` of the parser)
        if name == "vector" && kwargs.is_empty() && !env.contains_key(name) && !self.assigned.contains(name) && !self.funcs.contains_key(name) {
            return Ty::Vector;
        }
        for (_, value) in kwargs {
            self.expr(value, env, depth);
        }
        if !env.contains_key(name) {
            if let Some(func) = self.funcs.get(name).cloned() {
                if kwargs.is_empty() {
                    return self.user_call(name, &func, &tys, depth);
                }
                return Ty::Unknown;
            }
        }
        // a name bound to anything else, or bound somewhere we cannot see, is not the builtin of that name
        if env.contains_key(name) || self.assigned.contains(name) || self.dynamic_names {
            return Ty::Unknown;
        }
        self.builtin(name, &tys)
    }

    fn user_call(&mut self, name: &str, func: &Rc<Func>, args: &[Ty], depth: usize) -> Ty {
        let call_line = self.line;
        let mut types: Vec<Ty> = Vec::new();
        let mut specialised = false;
        for (i, param) in func.node.params.iter().enumerate() {
            let arg = args.get(i).cloned().unwrap_or(Ty::Unknown);
            match (&func.declared[i], &arg) {
                (Some(declared), Ty::Qty(actual)) => {
                    let allowed = func.implicit_units && actual.exps.is_empty();
                    if !actual.same_dimension(declared) && !allowed {
                        self.report(
                            Severity::Error,
                            "bad-argument",
                            format!(
                                "the argument for parameter '{param}' of '{name}' is {}, the declared unit is {}",
                                actual.text(),
                                declared.text()
                            ),
                        );
                    }
                    types.push(Ty::Qty(Dim { constant: None, point: declared.point, ..declared.clone() }));
                }
                (Some(declared), _) => types.push(Ty::Qty(declared.clone())),
                (None, Ty::Qty(dim)) => {
                    specialised = true;
                    types.push(Ty::Qty(Dim { constant: None, ..dim.clone() }));
                }
                (None, _) => types.push(Ty::Unknown),
            }
        }
        if !specialised {
            return func.returns.borrow().clone();
        }
        if depth >= MAX_DEPTH {
            return Ty::Unknown;
        }
        let key = format!("{name}({})", types.iter().map(Ty::key).collect::<Vec<_>>().join(","));
        if let Some(known) = self.memo.get(&key) {
            return known.clone();
        }
        if self.active.contains(&key) {
            return Ty::Unknown;
        }
        self.active.push(key.clone());
        let outer = std::mem::replace(
            &mut self.context,
            format!(
                " (called as {name}({}) at line {call_line})",
                types.iter().map(Ty::key).collect::<Vec<_>>().join(", ")
            ),
        );
        let result = self.check_body(func, &types, depth + 1);
        self.context = outer;
        self.active.pop();
        self.line = call_line;
        self.memo.insert(key, result.clone());
        result
    }

    fn builtin(&mut self, name: &str, args: &[Ty]) -> Ty {
        let only = |args: &[Ty]| -> Option<Dim> {
            match args {
                [Ty::Qty(dim)] => Some(dim.clone()),
                _ => None,
            }
        };
        match name {
            "sqrt" => match only(args) {
                Some(dim) => Ty::Qty(Dim { point: false, ..dim.pow(Rational64::new(1, 2)) }),
                None => Ty::Unknown,
            },
            "sin" | "cos" | "tan" | "sec" | "csc" | "cot" | "exp" | "ln" | "log" | "erf" => {
                self.dimensionless_function(name, only(args))
            }
            // a count of degrees of freedom and a coverage factor are plain numbers; an expanded
            // uncertainty is in the unit of the measurement it expands
            "dof" | "coverage_factor" => Ty::Qty(Dim::dimensionless()),
            "expanded" => match args.first() {
                Some(Ty::Qty(dim)) => Ty::Qty(Dim { constant: None, point: false, ..dim.clone() }),
                _ => Ty::Unknown,
            },
            "abs" | "floor" | "ceil" | "uncertainty" | "sigma" => match only(args) {
                Some(dim) => Ty::Qty(Dim { constant: None, ..dim }),
                None => Ty::Unknown,
            },
            "round" => match args.first() {
                Some(Ty::Qty(dim)) => Ty::Qty(Dim { constant: None, ..dim.clone() }),
                _ => Ty::Unknown,
            },
            "min" | "max" => self.extremum(name, args),
            "op_<" | "op_>" | "op_<=" | "op_>=" | "op_==" | "op_!=" | "op_\u{2260}" | "op_\u{2264}" | "op_\u{2265}" | "op_\u{2248}" | "op_approx" => {
                self.comparison(args)
            }
            "op_and" | "op_or" => {
                let word = if name == "op_and" { "and" } else { "or" };
                if let Some(left) = args.first() {
                    self.condition(left, &format!("the left side of `{word}`"));
                }
                if let Some(right) = args.get(1) {
                    self.condition(right, &format!("the right side of `{word}`"));
                }
                Ty::Bool
            }
            "op_not" => {
                if let Some(operand) = args.first() {
                    self.condition(operand, "`not`");
                }
                Ty::Bool
            }
            "ternary" | "if_then_else" => {
                if let Some(cond) = args.first() {
                    self.condition(cond, if name == "ternary" { "the condition of `? :`" } else { "the condition of `if`" });
                }
                match args {
                    [_, then, otherwise] if then == otherwise => then.clone(),
                    _ => Ty::Unknown,
                }
            }
            _ => Ty::Unknown,
        }
    }

    /// `sin`, `exp`, `ln`...: the interpreter takes an angle or a dimensionless value and answers a pure number.
    fn dimensionless_function(&mut self, name: &str, argument: Option<Dim>) -> Ty {
        let Some(dim) = argument else {
            return Ty::Unknown;
        };
        if !dim.is_plain_or_angle() {
            self.report(
                Severity::Error,
                "bad-argument",
                format!("{name} expects an angle or a dimensionless value, got {}", dim.text()),
            );
        }
        Ty::Qty(Dim::dimensionless())
    }

    /// `min` and `max` of different dimensions: the interpreter refuses them (`builtins::core::extremum`), so this is an
    /// error. A vector argument is `Unknown` to the pass and silences it.
    fn extremum(&mut self, name: &str, args: &[Ty]) -> Ty {
        let dims: Vec<&Dim> = args.iter().filter_map(|t| if let Ty::Qty(d) = t { Some(d) } else { None }).collect();
        if dims.len() != args.len() || dims.is_empty() {
            return Ty::Unknown;
        }
        if dims.iter().any(|d| !d.same_dimension(dims[0])) {
            self.report(
                Severity::Error,
                "mixed-extremum",
                format!(
                    "{name} of different dimensions ({}): compare values of one dimension",
                    dims.iter().map(|d| d.text()).collect::<Vec<_>>().join(", ")
                ),
            );
            return Ty::Unknown;
        }
        Ty::Qty(Dim { constant: None, ..dims[0].clone() })
    }

    /// A unit of the dimension `dim`, from the registry: `kg` for a mass, `K` for an absolute temperature.
    fn unit_of(&self, dim: &Dim) -> Option<String> {
        let dimension: Vec<(String, (i64, i64))> =
            dim.exps.iter().map(|(symbol, power)| (symbol.clone(), (*power.numer(), *power.denom()))).collect();
        self.registry.unit_of_dimension(&dimension).ok().map(|unit| unit.__repr__())
    }

    /// The unit the message of a zero tells to write (audit T18). The interpreter knows the unit the operand was
    /// written in; the checker knows its dimension only, so it names a unit of that dimension (`dB` for a level).
    fn zero_unit(&self, dim: &Dim) -> String {
        if dim.level && dim.exps.is_empty() {
            return "dB".to_string();
        }
        self.unit_of(dim).unwrap_or_else(|| "<its unit>".to_string())
    }

    /// The rule of `Quantity::comparable_magnitudes`: different dimensions have no comparison, so a zero needs a
    /// unit (`x > 0 m`) and an angle is not a plain number; a level and a dimensionless value that is not one have
    /// none either (`20 dB == 100`); a plain number compares with a ratio (`50 % == 0.5`).
    fn comparison(&mut self, args: &[Ty]) -> Ty {
        if let [Ty::Qty(a), Ty::Qty(b)] = args {
            let (with, plain) = if b.is_ratio() && !a.is_ratio() { (a, b) } else { (b, a) };
            let message = if plain.is_ratio() && !with.is_ratio() && plain.constant == Some(0.0) {
                Some(format!(
                    "cannot compare {} with the plain number 0: a zero needs a unit, write it with one (`x > 0 {}`; for \
                     a temperature point or a level, each scale's zero is a different value: 0 degC is 273.15 K, 0 dB \
                     is a ratio of 1)",
                    with.described(),
                    self.zero_unit(with)
                ))
            } else if plain.is_ratio() && with.level && with.exps.is_empty() {
                Some(format!(
                    "cannot compare a level with {}: write the level with its unit (`x > 20 dB`) or compare the \
                     ratio explicitly (`(x => unity) > 100`)",
                    if plain.constant.is_some() { "a plain number".to_string() } else { "a dimensionless value".to_string() }
                ))
            } else if !a.same_dimension(b) {
                Some(format!("cannot compare {} with {}", a.described(), b.described()))
            } else {
                None
            };
            if let Some(message) = message {
                self.report(Severity::Error, "dimension-mismatch", message);
            }
        }
        Ty::Bool
    }
}

/// The dimension a unit's facts describe.
fn dim_of(facts: &physure_core::units::UnitFacts) -> Dim {
    let exps = facts
        .dimension
        .iter()
        .map(|(symbol, (n, d))| (symbol.clone(), Rational64::new(*n, *d)))
        .filter(|(_, power)| *power != Rational64::from_integer(0))
        .collect();
    let point = facts.kind == UnitKind::Point;
    Dim { exps, point, level: !facts.is_linear && !point, constant: None }
}

/// What several branches agree on, or `Unknown`.
fn join(types: Vec<Ty>) -> Ty {
    match types.split_first() {
        Some((first, rest)) if rest.iter().all(|t| t == first) => first.clone(),
        _ => Ty::Unknown,
    }
}

/// An import of named symbols binds each of them (under its alias) to something the pass cannot type.
fn bind_imported(specifier: &ImportSpecifier, env: &mut Env) {
    if let ImportSpecifier::Symbols(symbols) = specifier {
        for symbol in symbols {
            env.insert(symbol.alias.clone().unwrap_or_else(|| symbol.name.clone()), Ty::Unknown);
        }
    }
}

/// The name a top-level statement binds in the program's own scope, as the interpreter's `env` holds it when the next
/// statement runs: an assignment or a function. A loop body is left out: it may never run.
fn bound_at_top_level(statement: &Statement) -> Option<&str> {
    match statement {
        Statement::Assignment(node) => Some(&node.name),
        Statement::FunctionDef(node) => Some(&node.name),
        _ => None,
    }
}

fn collect_assigned(statements: &[Statement], into: &mut HashSet<String>) {
    for statement in statements {
        match statement {
            Statement::Assignment(node) => {
                into.insert(node.name.clone());
            }
            Statement::While { body, .. } => collect_assigned(body, into),
            Statement::FunctionDef(node) => {
                into.insert(node.name.clone());
            }
            _ => {}
        }
    }
}

/// The exponent a literal stands for as an exact fraction: `2`, `0.5`, `-1.5`, a third written with enough digits.
fn as_rational(value: f64) -> Option<Rational64> {
    if !value.is_finite() {
        return None;
    }
    for denominator in 1..=12i64 {
        let numerator = (value * denominator as f64).round();
        if (numerator / denominator as f64 - value).abs() < 1e-9 && numerator.abs() < 1e9 {
            return Some(Rational64::new(numerator as i64, denominator));
        }
    }
    None
}
