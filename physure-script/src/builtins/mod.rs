pub mod core;
pub mod array;
pub mod calc;
pub mod plot;
pub use core::eval_core_builtin;

use physure_core::error::PhysureResult;
use super::value::PhsValue;
use super::interpreter::PhsInterpreter;

pub fn domain_members(domain: &str) -> Option<&'static [&'static str]> {
    match domain {
        "calc" => Some(&["deriv", "diff", "integral", "integrate", "solve", "substitute", "sub", "limit", "lim", "grad", "gradient", "div", "divergence", "curl", "laplacian", "simplify", "expand", "series", "taylor", "dsolve", "laplace", "inv_laplace", "inverse_laplace", "sym_det", "sym_trace", "sym_charpoly", "sym_eigenvalues", "sym_transpose"]),
        "plot" => Some(&["plot", "plot3d", "export3d", "export_3d", "plot_field", "plot_nd"]),
        "array" | "matrix" => Some(&["linspace", "gradient", "trapz", "dot", "cross", "norm", "unit_vector", "transpose", "matmul", "det", "sym_det", "sym_trace", "sym_charpoly", "sym_eigenvalues", "sym_transpose"]),
        _ => None,
    }
}

pub fn eval_domain_builtin_with_kwargs(
    domain: &str,
    name: &str,
    args: &[PhsValue],
    kwargs: &[(String, PhsValue)],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
    match domain {
        "calc" => calc::eval_calc_builtin(name, args, interpreter),
        "plot" => plot::eval_plot_builtin_with_kwargs(name, args, kwargs, interpreter, env),
        "array" => array::eval_array_builtin(name, args, interpreter),
        _ => Ok(None),
    }
}

pub fn eval_domain_builtin(
    domain: &str,
    name: &str,
    args: &[PhsValue],
    interpreter: &PhsInterpreter,
) -> PhysureResult<Option<PhsValue>> {
    let empty_env = std::collections::HashMap::new();
    eval_domain_builtin_with_kwargs(domain, name, args, &[], interpreter, &empty_env)
}

/// Renders a value under a Python-style `.<digits><kind>` spec (`x:.2f`, `x:.3e`). A
/// quantity keeps its unit — formatting is about how many digits to show, not about
/// discarding the half of the measurement that says what the number means.
fn expr_to_string(expr: &crate::ast::Expr) -> String {
    match expr {
        crate::ast::Expr::Quantity(q) => {
            let u = q.unit.as_deref().unwrap_or("");
            if u.is_empty() {
                format!("{}", q.magnitude)
            } else {
                format!("{} {}", q.magnitude, u)
            }
        }
        crate::ast::Expr::Str(s) | crate::ast::Expr::Identifier(s) => s.clone(),
        crate::ast::Expr::BinaryOp { op, left, right } => {
            let op_str = match op {
                crate::ast::BinaryOp::Add => "+",
                crate::ast::BinaryOp::Sub => "-",
                crate::ast::BinaryOp::Mul => "*",
                crate::ast::BinaryOp::Div => "/",
                crate::ast::BinaryOp::Pow => "^",
                crate::ast::BinaryOp::Convert => "=>",
                crate::ast::BinaryOp::Range => "..",
            };
            format!("{} {} {}", expr_to_string(left), op_str, expr_to_string(right))
        }
        crate::ast::Expr::FunctionCall { name, args, .. } => {
            let args_str: Vec<String> = args.iter().map(expr_to_string).collect();
            format!("{}({})", name, args_str.join(", "))
        }
        crate::ast::Expr::ForExpr { var, iterable, body } => {
            format!("for {} in {} {{ {} }}", var, expr_to_string(iterable), expr_to_string(body))
        }
    }
}



pub(crate) fn preprocess_symbolic_expression(expr_str: &str, interpreter: &PhsInterpreter) -> String {
    let mut result = expr_str.trim().to_string();
    for (name, val) in &interpreter.env {
        if let PhsValue::Function(func) = val {
            let fn_pattern = format!("{}(", name);
            if result.contains(&fn_pattern) {
                let body_expr = match func.body_stmts.last() {
                    Some(crate::ast::Statement::Expr(ref e)) => expr_to_string(e),
                    Some(crate::ast::Statement::Assignment(ref a)) => expr_to_string(&a.value),
                    _ => String::new(),
                };
                let body_code = format!("({})", body_expr);
                if let Some(start) = result.find(&fn_pattern) {
                    if let Some(end) = result[start..].find(')') {
                        result.replace_range(start..=start + end, &body_code);
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;
