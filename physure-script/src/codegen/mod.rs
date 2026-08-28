use crate::ast::{AssignmentNode, BinaryOp, Expr, FunctionDefNode, Program, QuantityNode, Statement};
use crate::interpreter::coerce_equation_string;
use crate::symbolic::Node;
use crate::value::PhsValue;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub enum CodegenError {
    Generic(String),
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::Generic(msg) => write!(f, "Codegen error: {}", msg),
        }
    }
}

impl std::error::Error for CodegenError {}

pub trait CodeGenerator {
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError>;
}

pub mod python;
pub mod rust;
pub mod java;
pub mod js;
pub mod proto;
pub mod md;

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Python,
    Rust,
    Java,
    JavaWithClass(String),
    JavaScript,
    TypeScript,
}

pub fn transpile(program: &Program, target: Target) -> Result<String, CodegenError> {
    let program = &inline_bindings_program(program);
    match target {
        Target::Python => {
            let compiled = compile_equations_to_functions(program)?;
            python::PythonTranspiler.generate_program(&compiled)
        }
        Target::Rust => {
            let compiled = compile_equations_to_functions(program)?;
            rust::RustTranspiler.generate_program(&compiled)
        }
        Target::Java => {
            let compiled = compile_equations_to_functions(program)?;
            java::JavaTranspiler::default().generate_program(&compiled)
        }
        Target::JavaWithClass(name) => {
            let compiled = compile_equations_to_functions(program)?;
            java::JavaTranspiler::new(&name).generate_program(&compiled)
        }
        Target::JavaScript => {
            let compiled = compile_equations_to_functions(program)?;
            js::JsTranspiler::default().generate_program(&compiled)
        }
        Target::TypeScript => {
            let compiled = compile_equations_to_functions(program)?;
            js::JsTranspiler { typed: true }.generate_program(&compiled)
        }
    }
}

/// Rewrites the `let(name, value, body)` that a `where` clause desugars to by substituting the
/// value into the body. None of the three targets has a binding form that works in expression
/// position, so the call used to be emitted verbatim and the generated file did not compile.
/// The substitution is exact: PHS expressions are pure and nothing inside a body can rebind a
/// name, so the only cost is repeating the value when the body uses it more than once.
fn inline_bindings(expr: &Expr) -> Expr {
    match expr {
        Expr::FunctionCall { name, args, kwargs } => {
            if name == "let" && kwargs.is_empty() && args.len() == 3 {
                if let Expr::Identifier(bound) = &args[0] {
                    let value = inline_bindings(&args[1]);
                    // Expanding the body first means an inner binding of the same name is
                    // already gone by the time the outer one substitutes, so shadowing works
                    // without tracking scopes here.
                    let body = inline_bindings(&args[2]);
                    return substitute(&body, bound, &value);
                }
            }
            Expr::FunctionCall {
                name: name.clone(),
                args: args.iter().map(inline_bindings).collect(),
                kwargs: kwargs.iter().map(|(k, v)| (k.clone(), inline_bindings(v))).collect(),
            }
        }
        Expr::BinaryOp { op, left, right } => Expr::BinaryOp {
            op: *op,
            left: Box::new(inline_bindings(left)),
            right: Box::new(inline_bindings(right)),
        },
        Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => expr.clone(),
        Expr::ForExpr { var, iterable, body } => Expr::ForExpr {
            var: var.clone(),
            iterable: Box::new(inline_bindings(iterable)),
            body: Box::new(inline_bindings(body)),
        },
    }
}

fn substitute(expr: &Expr, name: &str, value: &Expr) -> Expr {
    match expr {
        Expr::Identifier(id) if id == name => value.clone(),
        Expr::FunctionCall { name: called, args, kwargs } => Expr::FunctionCall {
            name: called.clone(),
            args: args.iter().map(|a| substitute(a, name, value)).collect(),
            kwargs: kwargs.iter().map(|(k, v)| (k.clone(), substitute(v, name, value))).collect(),
        },
        Expr::BinaryOp { op, left, right } => Expr::BinaryOp {
            op: *op,
            left: Box::new(substitute(left, name, value)),
            right: Box::new(substitute(right, name, value)),
        },
        Expr::ForExpr { var, iterable, body } => Expr::ForExpr {
            var: var.clone(),
            iterable: Box::new(substitute(iterable, name, value)),
            body: if var == name {
                body.clone()
            } else {
                Box::new(substitute(body, name, value))
            },
        },
        other => other.clone(),
    }
}

fn inline_bindings_stmt(stmt: &Statement) -> Statement {
    match stmt {
        Statement::Assignment(node) => Statement::Assignment(AssignmentNode {
            name: node.name.clone(),
            value: inline_bindings(&node.value),
            decorators: node.decorators.clone(),
        }),
        Statement::Expr(e) => Statement::Expr(inline_bindings(e)),
        Statement::Return(e) => Statement::Return(inline_bindings(e)),
        Statement::GuardReturn { cond, value } => Statement::GuardReturn {
            cond: inline_bindings(cond),
            value: inline_bindings(value),
        },
        Statement::FunctionDef(def) => Statement::FunctionDef(FunctionDefNode {
            body_stmts: def.body_stmts.iter().map(inline_bindings_stmt).collect(),
            ..def.clone()
        }),
        Statement::While { cond, body, body_lines } => Statement::While {
            cond: inline_bindings(cond),
            body: body.iter().map(inline_bindings_stmt).collect(),
            body_lines: body_lines.clone(),
        },
        other => other.clone(),
    }
}

fn inline_bindings_program(program: &Program) -> Program {
    Program {
        statements: program.statements.iter().map(inline_bindings_stmt).collect(),
        lines: program.lines.clone(),
    }
}

/// One piece of a string literal after `{expr}` interpolation has been read out of it.
pub(crate) enum StrPart {
    Text(String),
    Expr(Expr),
}

/// Splits a string literal into literal text and the expressions its braces interpolate,
/// mirroring the interpreter: a `{...}` whose contents do not parse stays literal, braces
/// and all, and an unclosed `{` is just a brace.
pub(crate) fn split_interpolated(text: &str) -> Vec<StrPart> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut rest = text;

    while let Some(start) = rest.find('{') {
        lit.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else {
            lit.push('{');
            break;
        };
        let inner = rest[..end].trim();
        rest = &rest[end + 1..];

        let parsed = crate::parse_phs(inner)
            .ok()
            .and_then(|p| match p.statements.into_iter().next() {
                Some(Statement::Expr(e)) => Some(e),
                _ => None,
            });
        match parsed {
            Some(expr) => {
                if !lit.is_empty() {
                    parts.push(StrPart::Text(std::mem::take(&mut lit)));
                }
                parts.push(StrPart::Expr(expr));
            }
            None => lit.push_str(&format!("{{{}}}", inner)),
        }
    }
    lit.push_str(rest);
    if !lit.is_empty() || parts.is_empty() {
        parts.push(StrPart::Text(lit));
    }
    parts
}

/// A range reaching a code generator is a script that asked for one where the target has
/// none. Every target says so with this, rather than with `unreachable!()`, which crashed
/// `phs transpile` on `r = 0 m .. 100 m` instead of reporting it.
pub(crate) fn range_is_not_transpilable() -> CodegenError {
    CodegenError::Generic(
        "A range (`a .. b`) has no equivalent in the generated code: it is an interval for a \
         builtin to sample, not a value a variable can hold. Pass it to the call that consumes \
         it, or transpile its endpoints separately."
            .to_string(),
    )
}

/// Converts a PHS `snake_case` identifier to `camelCase`, the identifier casing shared
/// by every target whose ecosystem convention is camelCase (Java, JS, TS).
pub(crate) fn snake_to_camel(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            capitalize_next = true;
        } else {
            if capitalize_next && i > 0 {
                result.push(c.to_ascii_uppercase());
            } else {
                result.push(c);
            }
            capitalize_next = false;
        }
    }
    result
}

pub fn expr_to_phs_string(expr: &Expr) -> String {
    match expr {
        Expr::Quantity(q) => {
            if let Some(u) = &q.unit {
                format!("{} {}", q.magnitude, u)
            } else {
                format!("{}", q.magnitude)
            }
        }
        Expr::Str(s) => s.clone(),
        Expr::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Expr::Identifier(name) => name.clone(),
        Expr::BinaryOp { op, left, right } => {
            let l = expr_to_phs_string(left);
            let r = expr_to_phs_string(right);
            let op_str = match op {
                BinaryOp::Add => " + ",
                BinaryOp::Sub => " - ",
                BinaryOp::Mul => " * ",
                BinaryOp::Div => "/",
                BinaryOp::Pow => "^",
                BinaryOp::Convert => " => ",
                BinaryOp::Range => " .. ",
            };
            format!("({}{}{})", l, op_str, r)
        }
        Expr::FunctionCall { name, args, kwargs: _ } => {
            let arg_strs: Vec<String> = args.iter().map(expr_to_phs_string).collect();
            format!("{}({})", name, arg_strs.join(", "))
        }
        Expr::ForExpr { var, iterable, body } => {
            format!("for {} in {} {{ {} }}", var, expr_to_phs_string(iterable), expr_to_phs_string(body))
        }
    }
}

pub fn expr_to_unit_string(expr: &Expr) -> String {
    match expr {
        Expr::Identifier(name) => name.clone(),
        Expr::Quantity(q) => {
            if let Some(u) = &q.unit {
                u.clone()
            } else {
                format!("{}", q.magnitude)
            }
        }
        Expr::BinaryOp { op, left, right } => {
            let l = expr_to_unit_string(left);
            let r = expr_to_unit_string(right);
            let op_str = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Pow => "^",
                BinaryOp::Convert => "=>",
                BinaryOp::Range => "..",
            };
            format!("{}{}{}", l, op_str, r)
        }
        _ => String::new(),
    }
}

/// The `not`/`and`/`or` shape the parser desugars logical expressions into (see `ast.rs`'s
/// note on `op_>` et al.). Every target maps this straight onto its own native `!`/`&&`/`||`
/// -- all four of Python, Rust, Java, and JavaScript/TypeScript are natively short-circuit,
/// so no target needs its own short-circuit emulation.
pub(crate) enum LogicalOp<'a> {
    Not(&'a Expr),
    And(&'a Expr, &'a Expr),
    Or(&'a Expr, &'a Expr),
}

pub(crate) fn as_logical_op(expr: &Expr) -> Option<LogicalOp<'_>> {
    let Expr::FunctionCall { name, args, kwargs } = expr else { return None };
    if !kwargs.is_empty() {
        return None;
    }
    match (name.as_str(), args.len()) {
        ("op_not", 1) => Some(LogicalOp::Not(&args[0])),
        ("op_and", 2) => Some(LogicalOp::And(&args[0], &args[1])),
        ("op_or", 2) => Some(LogicalOp::Or(&args[0], &args[1])),
        _ => None,
    }
}

/// Recognizes an expression whose PHS type is definitely `Bool` without needing to run the
/// interpreter: a literal, a comparison (`as_comparison_op`), a logical operator
/// (`as_logical_op`), or an identifier previously classified as boolean by `known_bools`.
///
/// This is what lets codegen -- which never sees a runtime `PhsValue` -- decide between the
/// `assert(Bool)`/`assert(Bool, String)` and `assert(Quantity, Quantity)` overloads (the
/// interpreter makes the same decision dynamically, from the actual argument values), and
/// lets Java and typed TypeScript give a local an explicit `boolean` type instead of the
/// default `Quantity`. Deliberately narrow: an opaque function call or an unclassified
/// identifier is never treated as boolean, matching this design's "no general overload
/// resolution" and "no inferring boolean return types for arbitrary functions" scope.
pub(crate) fn is_definitely_bool(expr: &Expr, known_bools: &HashSet<String>) -> bool {
    match expr {
        Expr::Bool(_) => true,
        Expr::Identifier(name) => known_bools.contains(name),
        Expr::FunctionCall { .. } => as_comparison_op(expr).is_some() || as_logical_op(expr).is_some(),
        _ => false,
    }
}

/// The overload an `assert`/`exact_assert` call site resolves to, decided statically from its
/// arity and (for a 2-argument `assert` call) whether the first argument `is_definitely_bool`.
/// Mirrors `builtins::core::eval_core_builtin`'s runtime dispatch, but codegen has no runtime
/// value to inspect -- only the AST plus whatever `known_bools` the caller has tracked from
/// earlier assignments in the same generated scope. Every codegen target intercepts this
/// before falling through to its generic `FunctionCall` handling, since none of them can
/// express PHS's `assert`/`exact_assert` as an ordinary function call.
pub(crate) enum AssertShape<'a> {
    Bool { condition: &'a Expr },
    BoolWithMessage { condition: &'a Expr, message: &'a Expr },
    Quantities { kind: &'static str, actual: &'a Expr, expected: &'a Expr },
}

pub(crate) fn as_assert_call<'a>(expr: &'a Expr, known_bools: &HashSet<String>) -> Option<AssertShape<'a>> {
    let Expr::FunctionCall { name, args, kwargs } = expr else { return None };
    if !kwargs.is_empty() {
        return None;
    }
    match (name.as_str(), args.as_slice()) {
        ("assert", [cond]) if is_definitely_bool(cond, known_bools) => Some(AssertShape::Bool { condition: cond }),
        ("assert", [cond, msg]) if is_definitely_bool(cond, known_bools) => {
            Some(AssertShape::BoolWithMessage { condition: cond, message: msg })
        }
        ("assert", [a, b]) => Some(AssertShape::Quantities { kind: "assert", actual: a, expected: b }),
        ("exact_assert", [a, b]) => Some(AssertShape::Quantities { kind: "exact_assert", actual: a, expected: b }),
        _ => None,
    }
}

/// `kinetic_energy` -> `KineticEnergy`. Used by `proto::ProtoGenerator` and
/// `rust::RustTranspiler::generate_export_shim` for message/service/struct names, so it lives
/// here rather than being duplicated in both.
pub(crate) fn to_pascal_case(name: &str) -> String {
    name.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod pascal_case_tests {
    use super::to_pascal_case;

    #[test]
    fn converts_snake_case_to_pascal_case() {
        assert_eq!(to_pascal_case("kinetic_energy"), "KineticEnergy");
        assert_eq!(to_pascal_case("ke"), "Ke");
        assert_eq!(to_pascal_case("a_b_c"), "ABC");
    }
}

/// Recognizes the `FunctionCall { name: "op_<", .. }` shape the parser desugars relational
/// operators into (see `ast.rs`'s note on `op_>` et al.), and returns the operands with the
/// symbol a target language's own `<`/`>`/`==`/... spells. Every `while`/`if` condition a PHS
/// script writes is one of these, so every codegen backend needs it to emit a runnable
/// condition instead of calling a nonexistent `op_<` function.
pub(crate) fn as_comparison_op(expr: &Expr) -> Option<(&'static str, &Expr, &Expr)> {
    let Expr::FunctionCall { name, args, kwargs } = expr else { return None };
    if !kwargs.is_empty() || args.len() != 2 {
        return None;
    }
    let symbol = match name.as_str() {
        "op_>" | "op_gt" => ">",
        "op_<" | "op_lt" => "<",
        "op_>=" | "op_gte" => ">=",
        "op_<=" | "op_lte" => "<=",
        "op_==" | "op_eq" => "==",
        "op_!=" | "op_neq" => "!=",
        _ => return None,
    };
    Some((symbol, &args[0], &args[1]))
}

/// Converts a symbolic algebra `Node` (from equation solving) into the same `Expr` tree
/// used for ordinary transpiled code, so it can be handed to the existing per-language
/// `generate_expr` without any target-specific translation logic of its own.
fn node_to_expr(node: &Node) -> Expr {
    match node {
        Node::Number(n) => Expr::Quantity(QuantityNode {
            magnitude: *n,
            uncertainty: None,
            uncertainty_lower: None,
            is_sigma: false,
            unit: None,
        }),
        Node::Symbol(s) | Node::Quantity(s, _) => Expr::Identifier(s.clone()),
        Node::Add(terms) => fold_terms(terms, BinaryOp::Add),
        Node::Mul(terms) => fold_terms(terms, BinaryOp::Mul),
        Node::Sub(l, r) => binary(BinaryOp::Sub, l, r),
        Node::Div(l, r) => binary(BinaryOp::Div, l, r),
        Node::Pow(l, r) => binary(BinaryOp::Pow, l, r),
        Node::Sin(x) => call1("sin", x),
        Node::Cos(x) => call1("cos", x),
        Node::Ln(x) => call1("ln", x),
        Node::Exp(x) => call1("exp", x),
        Node::Tan(x) => call1("tan", x),
        Node::Cot(x) => call1("cot", x),
        Node::Sec(x) => call1("sec", x),
        Node::Csc(x) => call1("csc", x),
        Node::Arcsin(x) => call1("arcsin", x),
        Node::Arccos(x) => call1("arccos", x),
        Node::Arctan(x) => call1("arctan", x),
        Node::Arccot(x) => call1("arccot", x),
        Node::Arcsec(x) => call1("arcsec", x),
        Node::Arccsc(x) => call1("arccsc", x),
        Node::Sinh(x) => call1("sinh", x),
        Node::Cosh(x) => call1("cosh", x),
        Node::Tanh(x) => call1("tanh", x),
        Node::Coth(x) => call1("coth", x),
        Node::Sech(x) => call1("sech", x),
        Node::Csch(x) => call1("csch", x),
        Node::Abs(x) => call1("abs", x),
        Node::Sqrt(x) => call1("sqrt", x),
        Node::Equation(a, b) => binary(BinaryOp::Sub, a, b),
        Node::Integral(u, _) => call1("integrate", u),
    }
}

fn binary(op: BinaryOp, l: &Node, r: &Node) -> Expr {
    Expr::BinaryOp { op, left: Box::new(node_to_expr(l)), right: Box::new(node_to_expr(r)) }
}

fn call1(name: &str, arg: &Node) -> Expr {
    Expr::FunctionCall { name: name.to_string(), args: vec![node_to_expr(arg)], kwargs: vec![] }
}

fn fold_terms(terms: &[Node], op: BinaryOp) -> Expr {
    let mut iter = terms.iter();
    let first = node_to_expr(iter.next().expect("Add/Mul node with no terms"));
    iter.fold(first, |acc, t| Expr::BinaryOp { op, left: Box::new(acc), right: Box::new(node_to_expr(t)) })
}

/// Java and Rust have no runtime support for calling an `Equation` with named arguments,
/// unlike Python where the generated code can just call into the real `physure` runtime.
/// Instead, the whole program is run through the interpreter once to learn every
/// equation's `(lhs, rhs)` Nodes, and each named-argument call site (e.g.
/// `eq5(I = -1.6mA, V = -6.3V)`) is compiled into a real static function (whichever side
/// of the equation is fully bound by the given argument names, translated via
/// `node_to_expr`) plus a plain positional call to it — preserving the actual formula
/// instead of folding the call to its precomputed numeric result.
///
/// Equation-*definition* statements themselves (`eq1 = "V = R * I"`, `eq5 = solve(eq1,
/// "R")`) are dropped: an `Equation` has no meaningful standalone Java/Rust value, only
/// the functions generated from its call sites above matter.
fn compile_equations_to_functions(program: &Program) -> Result<Program, CodegenError> {
    let mut interp = crate::interpreter::PhsInterpreter::default();
    let mut equations: HashMap<String, (Node, Node)> = HashMap::new();
    let mut functions: Vec<FunctionDefNode> = Vec::new();
    let mut signatures: HashMap<String, Vec<String>> = HashMap::new();
    let mut statements = Vec::with_capacity(program.statements.len());

    for stmt in &program.statements {
        let value = interp
            .run_statement(stmt)
            .map_err(|e| CodegenError::Generic(e.to_string()))?;

        if let Statement::Assignment(node) = stmt {
            if let PhsValue::Equation(lhs, rhs) = coerce_equation_string(value) {
                equations.insert(node.name.clone(), (lhs, rhs));
                continue;
            }
        }

        statements.push(match stmt {
            Statement::Assignment(node) => Statement::Assignment(AssignmentNode {
                name: node.name.clone(),
                value: rewrite_equation_calls(&node.value, &equations, &mut functions, &mut signatures)?,
                decorators: node.decorators.clone(),
            }),
            Statement::Expr(expr) => {
                Statement::Expr(rewrite_equation_calls(expr, &equations, &mut functions, &mut signatures)?)
            }
            other => other.clone(),
        });
    }

    let mut all_statements: Vec<Statement> = functions.into_iter().map(Statement::FunctionDef).collect();
    all_statements.extend(statements);
    Ok(Program { statements: all_statements, lines: vec![] })
}

/// Replaces `name(k1 = v1, k2 = v2)` calls to a tracked equation with a plain positional
/// call to a generated function, creating that function (once per equation) on first use.
fn rewrite_equation_calls(
    expr: &Expr,
    equations: &HashMap<String, (Node, Node)>,
    functions: &mut Vec<FunctionDefNode>,
    signatures: &mut HashMap<String, Vec<String>>,
) -> Result<Expr, CodegenError> {
    match expr {
        Expr::FunctionCall { name, kwargs, .. } if !kwargs.is_empty() && equations.contains_key(name) => {
            let (lhs, rhs) = &equations[name];
            let kwarg_names: Vec<String> = kwargs.iter().map(|(k, _)| k.clone()).collect();
            let kwarg_set: HashSet<&str> = kwarg_names.iter().map(String::as_str).collect();

            match signatures.get(name) {
                Some(existing) if existing.iter().map(String::as_str).collect::<HashSet<_>>() != kwarg_set => {
                    return Err(CodegenError::Generic(format!(
                        "Equation '{}' is called with different arguments in different places ({:?} vs {:?}); \
                         generating one static function per equation doesn't support that yet",
                        name, existing, kwarg_names
                    )));
                }
                Some(_) => {}
                None => {
                    let mut rhs_free = HashSet::new();
                    rhs.free_symbols(&mut rhs_free);
                    let chosen = if rhs_free.iter().all(|s| kwarg_set.contains(s.as_str())) { rhs } else { lhs };
                    functions.push(FunctionDefNode {
                        name: name.clone(),
                        params: kwarg_names.clone(),
                        param_units: vec![None; kwarg_names.len()],
                        body_stmts: vec![Statement::Expr(node_to_expr(chosen))],
                        body_lines: vec![],
                        decorators: Vec::new(),
                        doc: None,
                    });
                    signatures.insert(name.clone(), kwarg_names.clone());
                }
            }

            let kwarg_map: HashMap<&str, &Expr> = kwargs.iter().map(|(k, v)| (k.as_str(), v)).collect();
            let param_order = signatures[name].clone();
            let mut new_args = Vec::with_capacity(param_order.len());
            for p in &param_order {
                let v = kwarg_map.get(p.as_str()).expect("kwarg set already checked above");
                new_args.push(rewrite_equation_calls(v, equations, functions, signatures)?);
            }
            Ok(Expr::FunctionCall { name: name.clone(), args: new_args, kwargs: vec![] })
        }
        Expr::FunctionCall { name, args, kwargs } => {
            let new_args = args
                .iter()
                .map(|a| rewrite_equation_calls(a, equations, functions, signatures))
                .collect::<Result<Vec<_>, _>>()?;
            let new_kwargs = kwargs
                .iter()
                .map(|(k, v)| Ok((k.clone(), rewrite_equation_calls(v, equations, functions, signatures)?)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::FunctionCall { name: name.clone(), args: new_args, kwargs: new_kwargs })
        }
        Expr::BinaryOp { op, left, right } => Ok(Expr::BinaryOp {
            op: *op,
            left: Box::new(rewrite_equation_calls(left, equations, functions, signatures)?),
            right: Box::new(rewrite_equation_calls(right, equations, functions, signatures)?),
        }),
        Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => Ok(expr.clone()),
        Expr::ForExpr { var, iterable, body } => {
            let new_iterable = rewrite_equation_calls(iterable, equations, functions, signatures)?;
            let new_body = rewrite_equation_calls(body, equations, functions, signatures)?;
            Ok(Expr::ForExpr {
                var: var.clone(),
                iterable: Box::new(new_iterable),
                body: Box::new(new_body),
            })
        }
    }
}

#[cfg(test)]
mod const_fold_tests {
    use super::*;

    #[test]
    fn bool_literal_round_trips_through_expr_to_phs_string() {
        assert_eq!(expr_to_phs_string(&Expr::Bool(true)), "True");
        assert_eq!(expr_to_phs_string(&Expr::Bool(false)), "False");
    }

    /// A `where` clause desugars to `let(name, value, body)`, which every target used to
    /// emit verbatim as a call to a function that does not exist anywhere.
    #[test]
    fn where_bindings_are_inlined_instead_of_emitted_as_a_let_call() {
        let program =
            crate::parser::parse_phs("duplo = a + b where a = 2.0 m, b = a * 3.0").unwrap();

        for target in [Target::Python, Target::Rust, Target::Java, Target::JavaScript, Target::TypeScript] {
            let name = format!("{target:?}");
            let code = transpile(&program, target.clone()).unwrap();
            assert!(!code.contains("let("), "{name} still emits a let() call:\n{code}");
            // `b` is `a * 3.0`, so the binding for `a` has to reach inside it too.
            // JS/TS don't force a `.0` suffix on integral magnitudes (unlike Java/Rust,
            // which format via `{:?}`), so `"2.0"` never appears there; a bare `"2"`
            // needle would also falsely match inside the `v0.2.3` version header, so use
            // a collision-free, target-specific needle instead.
            match target {
                Target::JavaScript | Target::TypeScript => {
                    assert_eq!(
                        code.matches("Quantity.of(2, \"m\")").count(),
                        2,
                        "{name} lost a binding:\n{code}"
                    );
                }
                _ => {
                    assert_eq!(code.matches("2.0").count(), 2, "{name} lost a binding:\n{code}");
                }
            }
        }
    }

    #[test]
    fn named_argument_equation_call_compiles_to_a_real_function() {
        let program = crate::parser::parse_phs(
            "use solve from calc\neq1 = \"V = R * I\"\neq5 = solve(eq1, \"R\")\nres = eq5(I = -1.6mA, V = -6.3V) => kOhm",
        )
        .unwrap();

        let rust_code = transpile(&program, Target::Rust).unwrap();
        assert!(rust_code.contains("pub fn eq5("));
        assert!(rust_code.contains("&(V) / &(I)"));
        assert!(!rust_code.contains("3.937"));

        let java_code = transpile(&program, Target::Java).unwrap();
        assert!(java_code.contains("static Quantity eq5("));
        assert!(java_code.contains("V.divide(I)"));
        assert!(!java_code.contains("3.937"));

        let js_code = transpile(&program, Target::JavaScript).unwrap();
        assert!(js_code.contains("function eq5("));
        assert!(js_code.contains("V.divide(I)"));
        assert!(!js_code.contains("3.937"));

        let ts_code = transpile(&program, Target::TypeScript).unwrap();
        assert!(ts_code.contains("function eq5(I: Quantity, V: Quantity): Quantity"));
        assert!(ts_code.contains("V.divide(I)"));
        assert!(!ts_code.contains("3.937"));
    }

    #[test]
    fn unused_raw_equation_definitions_produce_no_output() {
        let program = crate::parser::parse_phs("eq1 = \"V = R * I\"\nx = 5\n").unwrap();

        let rust_code = transpile(&program, Target::Rust).unwrap();
        assert!(!rust_code.contains("eq1"));
        assert!(rust_code.contains('x'));
    }

    #[test]
    fn named_argument_call_to_unknown_symbol_still_errors() {
        let program = crate::parser::parse_phs("res = undefined_eq(x = 1)").unwrap();
        assert!(transpile(&program, Target::Rust).is_err());
    }

    #[test]
    fn test_transpile_loops_and_for_expressions_across_all_targets() {
        let program = crate::parser::parse_phs(
            "items = vector(1, 2, 3, 4)\ni = 0\nwhile i < 5 {\n  i = i + 1\n}\nres = for x in items { x * 2 }\n",
        )
        .unwrap();

        for target in [
            Target::Python,
            Target::Rust,
            Target::Java,
            Target::JavaScript,
            Target::TypeScript,
        ] {
            let code = transpile(&program, target.clone()).unwrap();
            let name = format!("{target:?}");

            match target {
                Target::Python => {
                    assert!(code.contains("while "), "Python missing while: {code}");
                    assert!(code.contains("[") && code.contains("for x in items]"), "Python missing for expr: {code}");
                }
                Target::Rust => {
                    assert!(code.contains("while "), "Rust missing while: {code}");
                    assert!(code.contains(".into_iter().map(|x|"), "Rust missing for expr: {code}");
                }
                Target::Java | Target::JavaWithClass(_) => {
                    assert!(code.contains("Quantity i = Quantity.of(0"), "Java missing init: {code}");
                    assert!(code.contains("while "), "Java missing while: {code}");
                    assert!(code.contains("i = i.add("), "Java missing re-assignment in while: {code}");
                    assert!(!code.contains("Quantity i = i.add("), "Java contains duplicate Quantity declaration in while: {code}");
                    assert!(code.contains(".stream().map(x ->"), "Java missing for expr: {code}");
                }
                Target::JavaScript | Target::TypeScript => {
                    assert!(code.contains("while "), "{name} missing while: {code}");
                    assert!(code.contains(".map((x) =>"), "{name} missing for expr: {code}");
                }
            }
        }
    }

    #[test]
    fn test_for_expr_inlining_and_shadowing() {
        let program = crate::parser::parse_phs(
            "items = vector(1, 2, 3, 4)\nres = (for x in items { x + y }) where y = 10.0",
        )
        .unwrap();

        let code = transpile(&program, Target::Python).unwrap();
        assert!(!code.contains("let("), "let should be inlined: {code}");
        assert!(code.contains("10.0"), "y should be substituted with 10.0: {code}");
    }
}

#[cfg(test)]
mod classifier_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn comparisons_and_logicals_are_definitely_bool() {
        let known = HashSet::new();
        let cmp = Expr::FunctionCall { name: "op_>".into(), args: vec![Expr::Bool(true), Expr::Bool(true)], kwargs: vec![] };
        assert!(is_definitely_bool(&cmp, &known));
        let not_expr = Expr::FunctionCall { name: "op_not".into(), args: vec![Expr::Bool(true)], kwargs: vec![] };
        assert!(is_definitely_bool(&not_expr, &known));
        assert!(is_definitely_bool(&Expr::Bool(false), &known));
        assert!(!is_definitely_bool(&Expr::Identifier("x".into()), &known));
    }

    #[test]
    fn a_known_bool_identifier_is_definitely_bool() {
        let mut known = HashSet::new();
        known.insert("flag".to_string());
        assert!(is_definitely_bool(&Expr::Identifier("flag".to_string()), &known));
    }

    #[test]
    fn assert_call_shape_depends_on_arity_and_the_bool_classifier() {
        let known = HashSet::new();
        let one_arg = Expr::FunctionCall { name: "assert".into(), args: vec![Expr::Bool(true)], kwargs: vec![] };
        assert!(matches!(as_assert_call(&one_arg, &known), Some(AssertShape::Bool { .. })));

        let bool_msg = Expr::FunctionCall {
            name: "assert".into(),
            args: vec![Expr::Bool(false), Expr::Str("boom".into())],
            kwargs: vec![],
        };
        assert!(matches!(as_assert_call(&bool_msg, &known), Some(AssertShape::BoolWithMessage { .. })));

        let quantities = Expr::FunctionCall {
            name: "assert".into(),
            args: vec![Expr::Identifier("a".into()), Expr::Identifier("b".into())],
            kwargs: vec![],
        };
        assert!(matches!(as_assert_call(&quantities, &known), Some(AssertShape::Quantities { kind: "assert", .. })));

        let exact = Expr::FunctionCall {
            name: "exact_assert".into(),
            args: vec![Expr::Identifier("a".into()), Expr::Identifier("b".into())],
            kwargs: vec![],
        };
        assert!(matches!(as_assert_call(&exact, &known), Some(AssertShape::Quantities { kind: "exact_assert", .. })));
    }
}
