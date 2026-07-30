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

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Python,
    Rust,
    Java,
    JavaWithClass(String),
}

pub fn transpile(program: &Program, target: Target) -> Result<String, CodegenError> {
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

/// Converts a symbolic algebra `Node` (from equation solving) into the same `Expr` tree
/// used for ordinary transpiled code, so it can be handed to the existing per-language
/// `generate_expr` without any target-specific translation logic of its own.
fn node_to_expr(node: &Node) -> Expr {
    match node {
        Node::Number(n) => Expr::Quantity(QuantityNode {
            magnitude: *n,
            uncertainty: None,
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
            }),
            Statement::Expr(expr) => {
                Statement::Expr(rewrite_equation_calls(expr, &equations, &mut functions, &mut signatures)?)
            }
            other => other.clone(),
        });
    }

    let mut all_statements: Vec<Statement> = functions.into_iter().map(Statement::FunctionDef).collect();
    all_statements.extend(statements);
    Ok(Program { statements: all_statements })
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
        Expr::Quantity(_) | Expr::Identifier(_) => Ok(expr.clone()),
    }
}

#[cfg(test)]
mod const_fold_tests {
    use super::*;

    #[test]
    fn named_argument_equation_call_compiles_to_a_real_function() {
        let program = crate::parser::parse_phs(
            "use solve from calc\neq1 = \"V = R * I\"\neq5 = solve(eq1, \"R\")\nres = eq5(I = -1.6mA, V = -6.3V) => kOhm",
        )
        .unwrap();

        let rust_code = transpile(&program, Target::Rust).unwrap();
        assert!(rust_code.contains("pub fn eq5("));
        assert!(rust_code.contains("V / I"));
        assert!(!rust_code.contains("3.937"));

        let java_code = transpile(&program, Target::Java).unwrap();
        assert!(java_code.contains("static Quantity eq5("));
        assert!(java_code.contains("V.divide(I)"));
        assert!(!java_code.contains("3.937"));
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
}
