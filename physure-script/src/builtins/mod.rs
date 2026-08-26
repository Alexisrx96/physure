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
mod tests {
    use super::*;
    use super::array::eval_array_builtin;
    use super::calc::eval_calc_builtin;
    use super::plot::{eval_plot_builtin, eval_plot_builtin_with_kwargs};
    use crate::interpreter::PhsInterpreter;
    use crate::value::PhsValue;

    fn eval(name: &str, args: Vec<PhsValue>) -> PhsValue {
        let interp = PhsInterpreter::default();
        let env = std::collections::HashMap::new();
        eval_core_builtin(name, &args, &interp, &env).unwrap().unwrap()
    }

    fn eval_calc(name: &str, args: Vec<PhsValue>) -> PhsValue {
        let interp = PhsInterpreter::default();
        eval_calc_builtin(name, &args, &interp).unwrap().unwrap()
    }

    fn eval_array(name: &str, args: Vec<PhsValue>) -> PhsValue {
        let interp = PhsInterpreter::default();
        eval_array_builtin(name, &args, &interp).unwrap().unwrap()
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(eval("sqrt", vec![PhsValue::Number(9.0)]), PhsValue::Number(3.0));
    }

    #[test]
    fn test_log() {
        assert_eq!(eval("log", vec![PhsValue::Number(100.0)]), PhsValue::Number(2.0));
    }

    #[test]
    fn test_trig() {
        assert_eq!(eval("sin", vec![PhsValue::Number(0.0)]), PhsValue::Number(0.0));
    }

    #[test]
    fn test_plot3d_and_export3d_domain_builtins() {
        let interp = PhsInterpreter::default();
        let res_plot = eval_plot_builtin(
            "plot3d",
            &[
                PhsValue::String("sin(x)*cos(y)".to_string()),
                PhsValue::String("Test 3D".to_string()),
            ],
            &interp,
        );
        assert!(res_plot.is_ok());

        let tmp_file = std::env::temp_dir().join("test_plot_3d.stl");
        let res_export = eval_plot_builtin(
            "export3d",
            &[
                PhsValue::String("sin(x)*cos(y)".to_string()),
                PhsValue::String(tmp_file.to_str().unwrap().to_string()),
                PhsValue::String("stl".to_string()),
            ],
            &interp,
        );
        assert!(res_export.is_ok());
        assert!(tmp_file.exists());
        let _ = std::fs::remove_file(tmp_file);
    }

    #[test]
    fn test_floor_ceil() {
        assert_eq!(eval("floor", vec![PhsValue::Number(3.7)]), PhsValue::Number(3.0));
        assert_eq!(eval("ceil", vec![PhsValue::Number(3.2)]), PhsValue::Number(4.0));
    }

    #[test]
    fn test_min_max() {
        assert_eq!(eval("min", vec![PhsValue::Number(3.0), PhsValue::Number(5.0)]), PhsValue::Number(3.0));
        assert_eq!(eval("max", vec![PhsValue::Number(3.0), PhsValue::Number(5.0)]), PhsValue::Number(5.0));
    }

    #[test]
    fn test_deriv() {
        let res = eval_calc("deriv", vec![PhsValue::String("x^2".into()), PhsValue::String("x".into())]);
        assert_eq!(res, PhsValue::String("2 * x".into()));
    }

    #[test]
    fn test_integral() {
        let res = eval_calc("integral", vec![PhsValue::String("2 * x".into()), PhsValue::String("x".into())]);
        if let PhsValue::String(s) = res {
            assert!(s.contains("2") && s.contains("x"));
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_solve() {
        let res = eval_calc("solve", vec![PhsValue::String("2 * x = 10".into()), PhsValue::String("x".into())]);
        match res {
            PhsValue::Number(n) => assert_eq!(n, 5.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 5.0),
            PhsValue::String(s) => assert_eq!(s, "5"),
            _ => panic!("Expected number, quantity, or string"),
        }
    }

    #[test]
    fn test_deriv_extended_script_suite() {
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("e^2x".into()), PhsValue::String("x".into())]),
            PhsValue::String("2 * e^(2 * x)".into())
        );
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("0 = sin(x)^2 + cosec(y)^2".into()), PhsValue::String("x".into())]),
            PhsValue::String("y' = (cos(x) * sin(x))/(cot(y) * csc(y)^2)".into())
        );
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("5 * (A * (X + 2))^X".into()), PhsValue::String("X".into())]),
            PhsValue::String("5 * (X/(2 + X) + ln((2 + X) * A)) * ((2 + X) * A)^X".into())
        );
        // Single-arg Leibniz notation: dy/dx, d^2y/dx^2, d/dx(sin(x))
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("dy/dx".into())]),
            PhsValue::String("y'".into())
        );
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("d^2y/dx^2".into())]),
            PhsValue::String("y''".into())
        );
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("d/dx(sin(x))".into())]),
            PhsValue::String("cos(x)".into())
        );
        // Differential variable parameter: dx, d(x)
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("y".into()), PhsValue::String("dx".into())]),
            PhsValue::String("y'".into())
        );
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("y".into()), PhsValue::String("dx".into()), PhsValue::Number(2.0)]),
            PhsValue::String("y''".into())
        );
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("x^4".into()), PhsValue::String("dx".into()), PhsValue::Number(3.0)]),
            PhsValue::String("24 * x".into())
        );
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("cos(x)".into()), PhsValue::String("dx".into()), PhsValue::Number(4.0)]),
            PhsValue::String("cos(x)".into())
        );
    }

    #[test]
    fn test_integral_extended_script_suite() {
        assert_eq!(
            eval_calc("integral", vec![PhsValue::String("xe^x".into()), PhsValue::String("x".into())]),
            PhsValue::String("e^x * x - e^x".into())
        );
        assert_eq!(
            eval_calc("integral", vec![PhsValue::String("1 / (1 + x^2)".into()), PhsValue::String("x".into())]),
            PhsValue::String("atan(x)".into())
        );
        // Single-arg differential integrand: integral("sin(x) dx")
        assert_eq!(
            eval_calc("integral", vec![PhsValue::String("sin(x) dx".into())]),
            PhsValue::String("cos(x) * -1".into())
        );
        // Definite integral with differential variable
        let def_res = eval_calc("integral", vec![
            PhsValue::String("x^2".into()),
            PhsValue::String("dx".into()),
            PhsValue::Number(0.0),
            PhsValue::Number(3.0),
        ]);
        assert_eq!(def_res, PhsValue::Number(9.0));
    }

    #[test]
    fn test_vector_calculus_fields_suite() {
        let interp = PhsInterpreter::default();
        
        // grad("x^2 + y^2", ["dx", "dy"]) -> ["2 * x", "2 * y"]
        let res_grad = eval_calc_builtin("grad", &[PhsValue::String("x^2 + y^2".into()), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into())])], &interp).unwrap().unwrap();
        assert_eq!(res_grad, PhsValue::Vector(vec![PhsValue::String("2 * x".into()), PhsValue::String("2 * y".into())]));
        
        // div(["x^2", "y^2"], ["dx", "dy"]) -> "2 * x + 2 * y"
        let res_div = eval_calc_builtin("div", &[PhsValue::Vector(vec![PhsValue::String("x^2".into()), PhsValue::String("y^2".into())]), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into())])], &interp).unwrap().unwrap();
        assert_eq!(res_div, PhsValue::String("2 * x + 2 * y".into()));
        
        // curl(["y", "-x", "0"], ["dx", "dy", "dz"]) -> ["0", "0", "-2"]
        let res_curl = eval_calc_builtin("curl", &[PhsValue::Vector(vec![PhsValue::String("y".into()), PhsValue::String("-1 * x".into()), PhsValue::String("0".into())]), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into()), PhsValue::String("dz".into())])], &interp).unwrap().unwrap();
        assert_eq!(res_curl, PhsValue::Vector(vec![PhsValue::String("0".into()), PhsValue::String("0".into()), PhsValue::String("-2".into())]));
        
        // laplacian("x^2 + y^2", ["dx", "dy"]) -> "4"
        let res_lap = eval_calc_builtin("laplacian", &[PhsValue::String("x^2 + y^2".into()), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into())])], &interp).unwrap().unwrap();
        assert_eq!(res_lap, PhsValue::String("4".into()));
    }

    #[test]
    fn test_linspace() {
        let res = eval_array("linspace", vec![PhsValue::Number(0.0), PhsValue::Number(1.0), PhsValue::Number(3.0)]);
        if let PhsValue::Vector(v) = res {
            assert_eq!(v.len(), 3);
            assert_eq!(v[0], PhsValue::Number(0.0));
            assert_eq!(v[1], PhsValue::Number(0.5));
            assert_eq!(v[2], PhsValue::Number(1.0));
        } else {
            panic!("Expected vector");
        }
    }

    #[test]
    fn test_gradient() {
        let y = PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Number(4.0), PhsValue::Number(9.0)]);
        let x = PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Number(2.0), PhsValue::Number(3.0)]);
        let res = eval_array("gradient", vec![y, x]);
        if let PhsValue::Vector(v) = res {
            assert_eq!(v.len(), 3);
        } else {
            panic!("Expected vector");
        }
    }

    #[test]
    fn test_trapz() {
        let y = PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Number(1.0)]);
        let x = PhsValue::Vector(vec![PhsValue::Number(0.0), PhsValue::Number(1.0)]);
        let res = eval_array("trapz", vec![y, x]);
        assert_eq!(res, PhsValue::Number(1.0));
    }

    #[test]
    fn test_exhaustive_requested_features() {
        assert_eq!(
            eval_calc("integral", vec![PhsValue::String("cos(x) dx".into())]),
            PhsValue::String("sin(x)".into())
        );
        let def_res = eval_calc("integral", vec![
            PhsValue::String("x^3".into()),
            PhsValue::String("dx".into()),
            PhsValue::Number(0.0),
            PhsValue::Number(2.0),
        ]);
        assert_eq!(def_res, PhsValue::Number(4.0));
        
        let res_grad = eval_calc("grad", vec![PhsValue::String("x*y*z".into()), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into()), PhsValue::String("dz".into())])]);
        assert_eq!(res_grad, PhsValue::Vector(vec![PhsValue::String("y * z".into()), PhsValue::String("x * z".into()), PhsValue::String("x * y".into())]));
        
        let res_div = eval_calc("div", vec![PhsValue::Vector(vec![PhsValue::String("x".into()), PhsValue::String("y".into()), PhsValue::String("z".into())]), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into()), PhsValue::String("dz".into())])]);
        assert_eq!(res_div, PhsValue::String("3".into()));
        
        let res_curl = eval_calc("curl", vec![PhsValue::Vector(vec![PhsValue::String("y*z".into()), PhsValue::String("x*z".into()), PhsValue::String("x*y".into())]), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into()), PhsValue::String("dz".into())])]);
        assert_eq!(res_curl, PhsValue::Vector(vec![PhsValue::String("0".into()), PhsValue::String("0".into()), PhsValue::String("0".into())]));
        
        let res_lap = eval_calc("laplacian", vec![PhsValue::String("x^2 + y^2 + z^2".into()), PhsValue::Vector(vec![PhsValue::String("dx".into()), PhsValue::String("dy".into()), PhsValue::String("dz".into())])]);
        assert_eq!(res_lap, PhsValue::String("6".into()));
        
        assert_eq!(
            eval_calc("deriv", vec![PhsValue::String("x^4".into()), PhsValue::String("dx".into()), PhsValue::Number(4.0)]),
            PhsValue::String("24".into())
        );
    }
}
