pub mod core;
pub mod dof;
pub mod array;
pub mod calc;
mod definite_integral;
pub(crate) mod dimensions;
mod inline;
mod limit;
mod quadrature;
pub mod plot;
pub use core::eval_core_builtin;
pub(crate) use inline::preprocess_symbolic_expression;

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
        "calc" => calc::eval_calc_builtin_in(name, args, kwargs, interpreter, env),
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
    eval_domain_builtin_with_kwargs(domain, name, args, &[], interpreter, &interpreter.env)
}

#[cfg(test)]
mod tests;
