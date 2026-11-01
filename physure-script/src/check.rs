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
//! neither a variable nor a unit and meets a quantity. A value the pass cannot type (an argument of a
//! function it has not seen, a vector, an imported name) is `Unknown` and silences every check that depends
//! on it. The one report that is a warning, not an error, is `min` or `max` of different dimensions, which
//! the interpreter accepts (it compares base-unit magnitudes) and which is never what was meant.
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
    /// The interpreter accepts it and it is almost certainly not what was meant.
    Warning,
}

/// A finding, at the line of the statement it was found in (1-based; 0 for an AST built without lines).
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub line: usize,
    pub severity: Severity,
    /// A short stable name: `dimension-mismatch`, `unknown-unit`, `unknown-name`, `bad-argument`,
    /// `bad-exponent`, `temperature-points`, `mixed-extremum`.
    pub code: &'static str,
    pub message: String,
}

/// Parses `code` and checks it against the catalog that unit text is read against on this thread (the
/// embedded one, plus a `physure.conf` in the working directory, unless a `units::with_catalog` scope is set).
pub fn check_source(code: &str) -> PhysureResult<Vec<Diagnostic>> {
    let program = crate::parser::parse_phs(code)?;
    Ok(physure_core::units::with_current_catalog(|scoped| match scoped {
        Some(registry) => check_program(&program, registry),
        None => check_program(&program, &physure_core::units::conf::build_registry_from_conf().0),
    }))
}

/// Checks a parsed program against `registry`.
pub fn check_program(program: &Program, registry: &UnitRegistry) -> Vec<Diagnostic> {
    let mut checker = Checker::new(registry);
    checker.prepare(&program.statements);
    let mut env = Env::new();
    checker.block(&program.statements, &program.lines, &mut env, 0);
    checker.diagnostics
}

// ---- dimensions

/// A dimension by dimension symbol (`L`, `M`, `T`, `A`...), whether it is an absolute temperature, and the
/// value when it is a dimensionless literal (an exponent is read from it).
#[derive(Clone, Debug, PartialEq, Default)]
struct Dim {
    exps: BTreeMap<String, Rational64>,
    point: bool,
    constant: Option<f64>,
}

impl Dim {
    fn dimensionless() -> Dim {
        Dim::default()
    }

    fn same_dimension(&self, other: &Dim) -> bool {
        self.exps == other.exps
    }

    fn combine(&self, other: &Dim, sign: i64) -> Dim {
        let mut exps = self.exps.clone();
        for (symbol, power) in &other.exps {
            let entry = exps.entry(symbol.clone()).or_insert_with(|| Rational64::from_integer(0));
            *entry += *power * Rational64::from_integer(sign);
        }
        exps.retain(|_, power| *power != Rational64::from_integer(0));
        Dim { exps, point: false, constant: None }
    }

    fn pow(&self, power: Rational64) -> Dim {
        let exps = self.exps.iter().map(|(symbol, p)| (symbol.clone(), *p * power)).collect();
        Dim { exps, point: false, constant: None }
    }

    /// Dimensionless, or an angle: what `sin`, `cos`, `tan`, `exp`, `ln` and `log` take.
    fn is_plain_or_angle(&self) -> bool {
        self.exps.is_empty() || (self.exps.len() == 1 && self.exps.get("A") == Some(&Rational64::from_integer(1)))
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
    /// A bare word that is not bound, not imported and not a unit: the interpreter reads it as text, which
    /// fails the moment it meets a quantity.
    Symbol(String),
}

impl Ty {
    fn key(&self) -> String {
        match self {
            Ty::Unknown => "?".to_string(),
            Ty::Qty(dim) => format!("{}{}", dim.text(), if dim.point { "@" } else { "" }),
            Ty::Bool => "bool".to_string(),
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
                self.expr(cond, env, depth);
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
        self.expr(cond, env, depth);
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
        let result = self.block(&func.node.body_stmts, &func.node.body_lines, &mut body_env, depth);
        self.line = outer_line;
        result
    }

    // ---- expressions

    fn expr(&mut self, expr: &Expr, env: &Env, depth: usize) -> Ty {
        match expr {
            Expr::Quantity(node) => self.quantity(node),
            Expr::Bool(_) => Ty::Bool,
            Expr::Str(_) => Ty::Unknown,
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
            // a name that is not a variable is text, and the interpreter hands text back unconverted
            if let Ty::Qty(dim) = &from {
                if !dim.same_dimension(&target) {
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
            format!("'{name}' is neither a variable nor a unit, and meets a quantity here"),
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
        Ty::Qty(Dim { exps: a.exps.clone(), point, constant })
    }

    /// Whether the result of a sum or difference is an absolute temperature; `None` (after reporting) when the
    /// operands may not be combined at all.
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
            (BinaryOp::Sub, false, true) => {
                self.report(
                    Severity::Error,
                    "temperature-points",
                    "an absolute temperature cannot be subtracted from an interval".to_string(),
                );
                None
            }
            (BinaryOp::Add, true, false) | (BinaryOp::Add, false, true) | (BinaryOp::Sub, true, false) => Some(true),
            _ => Some(false),
        }
    }

    fn mul_div(&mut self, op: BinaryOp, a: &Dim, b: &Dim) -> Ty {
        if (a.point && b.exps.is_empty()) || (b.point && a.exps.is_empty() && op == BinaryOp::Mul) {
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
            "sin" | "cos" | "tan" | "exp" | "ln" | "log" => self.dimensionless_function(name, only(args)),
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
            "op_and" | "op_or" | "op_not" => Ty::Bool,
            "ternary" | "if_then_else" => match args {
                [_, then, otherwise] if then == otherwise => then.clone(),
                _ => Ty::Unknown,
            },
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

    /// `min` and `max` compare base-unit magnitudes with no dimension check, which the interpreter accepts.
    fn extremum(&mut self, name: &str, args: &[Ty]) -> Ty {
        let dims: Vec<&Dim> = args.iter().filter_map(|t| if let Ty::Qty(d) = t { Some(d) } else { None }).collect();
        if dims.len() != args.len() || dims.is_empty() {
            return Ty::Unknown;
        }
        if dims.iter().any(|d| !d.same_dimension(dims[0])) {
            self.report(
                Severity::Warning,
                "mixed-extremum",
                format!(
                    "{name} of different dimensions ({}) compares their magnitudes in base units, which means nothing",
                    dims.iter().map(|d| d.text()).collect::<Vec<_>>().join(", ")
                ),
            );
            return Ty::Unknown;
        }
        Ty::Qty(Dim { constant: None, ..dims[0].clone() })
    }

    fn comparison(&mut self, args: &[Ty]) -> Ty {
        if let [Ty::Qty(a), Ty::Qty(b)] = args {
            if !a.same_dimension(b) {
                self.report(Severity::Error, "dimension-mismatch", format!("cannot compare {} with {}", a.text(), b.text()));
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
    Dim { exps, point: facts.kind == UnitKind::Point, constant: None }
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
