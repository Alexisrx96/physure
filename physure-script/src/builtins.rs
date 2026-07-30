use physure_core::error::{PhysureError, PhysureResult};
use super::value::PhsValue;
use super::interpreter::PhsInterpreter;
use crate::ast::BinaryOp;

fn eval_bin_op(op: BinaryOp, l: &PhsValue, r: &PhsValue) -> PhysureResult<PhsValue> {
    let interp = PhsInterpreter::default();
    interp.eval_binary_op_vals(op, l.clone(), r.clone())
}

/// PHS has no boolean type; comparisons yield a dimensionless 1.0 or 0.0.
fn boolean(res: bool) -> PhsValue {
    PhsValue::Quantity(physure_core::Quantity::new_scalar(
        if res { 1.0 } else { 0.0 },
        0.0,
        physure_core::units::RationalUnit::dimensionless(),
        None,
        None,
    ))
}

/// Two quantities reduced to a pair of numbers that can be compared directly.
///
/// Comparing the raw magnitudes makes `1 km == 1000 m` false and `1 km > 999 m` false
/// as well — the scale factor has to be folded in first, exactly as `add` already does.
/// Different dimensions are an error rather than `false`: `5 m > 2 s` has no answer, and
/// answering `false` lets it slip through a conditional as if it did.
/// A dimensionless zero is comparable with anything: `x > 0` is how a sign test is
/// written, and zero is the one magnitude that reads the same in every unit.
fn comparable(l: &physure_core::Quantity, r: &physure_core::Quantity) -> PhysureResult<(f64, f64)> {
    let dimensionless_zero =
        |q: &physure_core::Quantity| q.unit.dimensions.is_empty() && q.value.mean() == 0.0;
    if !l.unit.same_dimensions(&r.unit) && !dimensionless_zero(l) && !dimensionless_zero(r) {
        return Err(PhysureError::UnitMismatch {
            expected: l.unit.__repr__(),
            actual: r.unit.__repr__(),
        });
    }
    Ok((l.canonical_magnitude(), r.canonical_magnitude()))
}

/// Applies `pred` to the two operands once both are on a common scale. Strings only
/// support equality, so they collapse to "equal" / "not equal" rather than an ordering.
fn compare(args: &[PhsValue], pred: impl Fn(f64, f64) -> bool) -> PhysureResult<Option<PhsValue>> {
    let as_quantity = |v: &PhsValue| match v {
        PhsValue::Quantity(q) => Some(q.clone()),
        PhsValue::Number(n) => Some(physure_core::Quantity::new_scalar(
            *n,
            0.0,
            physure_core::units::RationalUnit::dimensionless(),
            None,
            None,
        )),
        _ => None,
    };
    match (args.first(), args.get(1)) {
        (Some(PhsValue::String(l)), Some(PhsValue::String(r))) => {
            Ok(Some(boolean(pred(0.0, if l == r { 0.0 } else { 1.0 }))))
        }
        (Some(l), Some(r)) => match (as_quantity(l), as_quantity(r)) {
            (Some(l), Some(r)) => {
                let (l, r) = comparable(&l, &r)?;
                Ok(Some(boolean(pred(l, r))))
            }
            _ => Ok(Some(boolean(false))),
        },
        _ => Ok(Some(boolean(false))),
    }
}

pub fn domain_members(domain: &str) -> Option<&'static [&'static str]> {
    match domain {
        "calc" => Some(&["deriv", "diff", "integral", "integrate", "solve", "substitute", "sub", "limit", "lim"]),
        "plot" => Some(&["plot"]),
        "array" => Some(&["linspace", "gradient", "trapz"]),
        _ => None,
    }
}

pub fn eval_domain_builtin(domain: &str, name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match domain {
        "calc" => eval_calc_builtin(name, args, interpreter),
        "plot" => eval_plot_builtin(name, args, interpreter),
        "array" => eval_array_builtin(name, args, interpreter),
        _ => Ok(None),
    }
}

pub fn eval_core_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "format" => {
            if let Some(val) = args.first() {
                Ok(Some(val.clone()))
            } else {
                Ok(None)
            }
        }
        "op_>" | "op_gt" => compare(args, |l, r| l > r),
        "op_<" | "op_lt" => compare(args, |l, r| l < r),
        "op_>=" | "op_gte" => compare(args, |l, r| l >= r),
        "op_<=" | "op_lte" => compare(args, |l, r| l <= r),
        "op_==" | "op_eq" => {
            // `x == 5.0 +/- 0.2` asks whether x lies within k sigma of the target, so it
            // is a tolerance test rather than an ordinary comparison.
            if let (Some(PhsValue::Quantity(l)), Some(PhsValue::SigmaBound(target_q, k_sigma)))
            | (Some(PhsValue::SigmaBound(target_q, k_sigma)), Some(PhsValue::Quantity(l))) =
                (args.first(), args.get(1))
            {
                let unc = if l.value.std_dev() > 0.0 { l.value.std_dev() } else { 0.05 };
                let (a, b) = comparable(l, target_q)?;
                return Ok(Some(boolean((a - b).abs() <= k_sigma * unc)));
            }
            compare(args, |l, r| (l - r).abs() < 1e-9)
        }
        "op_!=" | "op_neq" => compare(args, |l, r| (l - r).abs() >= 1e-9),
        "op_≈" | "op_approx" => compare(args, |l, r| (l - r).abs() < 1e-3),
        "ternary" | "if_then_else" => {
            let cond_true = match args.first() {
                Some(PhsValue::Quantity(q)) => q.value.mean() > 0.0,
                Some(PhsValue::Number(n)) => *n > 0.0,
                _ => false,
            };
            if cond_true {
                Ok(args.get(1).cloned())
            } else {
                Ok(args.get(2).cloned())
            }
        }
        // The standard uncertainty as a quantity in the same unit, so it can be fed back
        // into arithmetic (`uncertainty(x) / x => %`). Anything without one reports zero.
        "uncertainty" | "sigma" => match args.first() {
            Some(PhsValue::Quantity(q)) => Ok(Some(PhsValue::Quantity(
                physure_core::Quantity::new_scalar(q.value.std_dev(), 0.0, q.unit.clone(), None, None),
            ))),
            Some(PhsValue::Number(_)) => Ok(Some(PhsValue::Number(0.0))),
            _ => Err(PhysureError::Generic("uncertainty expects a quantity".into())),
        },
        "vector" => {
            Ok(Some(PhsValue::Vector(args.to_vec())))
        }
        "sqrt" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("sqrt expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.sqrt()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.sqrt()?))),
                _ => Err(PhysureError::Generic("sqrt expects a number or quantity".into())),
            }
        }
        "sin" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("sin expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.sin()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Number(q.value.mean().sin()))),
                _ => Err(PhysureError::Generic("sin expects a number".into())),
            }
        }
        "cos" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("cos expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.cos()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Number(q.value.mean().cos()))),
                _ => Err(PhysureError::Generic("cos expects a number".into())),
            }
        }
        "exp" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("exp expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.exp()))),
                _ => Err(PhysureError::Generic("exp expects a number".into())),
            }
        }
        "ln" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("ln expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.ln()))),
                _ => Err(PhysureError::Generic("ln expects a number".into())),
            }
        }
        "abs" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("abs expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.abs()))),
                _ => Err(PhysureError::Generic("abs expects a number".into())),
            }
        }
        "log" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("log expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.log10()))),
                _ => Err(PhysureError::Generic("log expects a number".into())),
            }
        }
        "tan" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("tan expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.tan()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Number(q.value.mean().tan()))),
                _ => Err(PhysureError::Generic("tan expects a number".into())),
            }
        }
        "floor" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("floor expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.floor()))),
                PhsValue::Quantity(q) => {
                    use physure_core::quantity::Quantity;
                    Ok(Some(PhsValue::Quantity(Quantity::new_scalar(
                        q.value.mean().floor(),
                        0.0,
                        q.unit.clone(),
                        None,
                        None,
                    ))))
                }
                _ => Err(PhysureError::Generic("floor expects number or quantity".into())),
            }
        }
        "ceil" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("ceil expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.ceil()))),
                PhsValue::Quantity(q) => {
                    use physure_core::quantity::Quantity;
                    Ok(Some(PhsValue::Quantity(Quantity::new_scalar(
                        q.value.mean().ceil(),
                        0.0,
                        q.unit.clone(),
                        None,
                        None,
                    ))))
                }
                _ => Err(PhysureError::Generic("ceil expects number or quantity".into())),
            }
        }
        "min" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("min expects arguments".into()));
            }
            let mut best = &args[0];
            for arg in args.iter().skip(1) {
                // compare by base-SI magnitude but return the original
                let best_mag = match best {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("min expects numbers or quantities".into())),
                };
                let arg_mag = match arg {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("min expects numbers or quantities".into())),
                };
                if arg_mag < best_mag {
                    best = arg;
                }
            }
            Ok(Some(best.clone()))
        }
        "max" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("max expects arguments".into()));
            }
            let mut best = &args[0];
            for arg in args.iter().skip(1) {
                let best_mag = match best {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("max expects numbers or quantities".into())),
                };
                let arg_mag = match arg {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.canonical_magnitude(),
                    _ => return Err(PhysureError::Generic("max expects numbers or quantities".into())),
                };
                if arg_mag > best_mag {
                    best = arg;
                }
            }
            Ok(Some(best.clone()))
        }
        "round" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("round expects arguments".into()));
            }
            let decimals = match args.get(1) {
                Some(PhsValue::Number(d)) => *d as i32,
                Some(PhsValue::Quantity(q)) => q.canonical_magnitude() as i32,
                _ => 0,
            };
            let factor = 10.0f64.powi(decimals);
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number((n * factor).round() / factor))),
                PhsValue::Quantity(q) => {
                    use physure_core::quantity::Quantity;
                    let rounded = Quantity::new_scalar(
                        (q.value.mean() * factor).round() / factor,
                        0.0,
                        q.unit.clone(),
                        None,
                        None,
                    );
                    Ok(Some(PhsValue::Quantity(rounded)))
                }
                _ => Err(PhysureError::Generic("round expects number or quantity".into())),
            }
        }
        _ => Ok(None),
    }
}

fn eval_array_builtin(name: &str, args: &[PhsValue], _interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "linspace" => {
            if args.len() < 2 {
                return Err(PhysureError::Generic("linspace expects start and stop".into()));
            }
            let start = match &args[0] {
                PhsValue::Number(n) => *n,
                PhsValue::Quantity(q) => q.value.mean(),
                _ => 0.0,
            };
            let stop = match &args[1] {
                PhsValue::Number(n) => *n,
                PhsValue::Quantity(q) => q.value.mean(),
                _ => 1.0,
            };
            let count = if args.len() >= 3 {
                match &args[2] {
                    PhsValue::Number(n) => *n as usize,
                    _ => 50,
                }
            } else {
                50
            };
            let unit = match &args[0] {
                PhsValue::Quantity(q) => Some(q.unit.clone()),
                _ => match &args[1] {
                    PhsValue::Quantity(q) => Some(q.unit.clone()),
                    _ => None,
                },
            };
            let step = if count > 1 { (stop - start) / (count - 1) as f64 } else { 0.0 };
            let vec: Vec<PhsValue> = (0..count)
                .map(|i| {
                    let val = start + i as f64 * step;
                    if let Some(ref u) = unit {
                        use physure_core::quantity::Quantity;
                        PhsValue::Quantity(Quantity::new_scalar(val, 0.0, u.clone(), None, None))
                    } else {
                        PhsValue::Number(val)
                    }
                })
                .collect();
            Ok(Some(PhsValue::Vector(vec)))
        }
        "gradient" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("gradient expects y and x vectors".into()));
            }
            let y_vec = match &args[0] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("gradient expects y vector".into())),
            };
            let x_vec = match &args[1] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("gradient expects x vector".into())),
            };
            if y_vec.len() != x_vec.len() || y_vec.len() < 2 {
                return Err(PhysureError::Generic("gradient expects equal length vectors with at least 2 elements".into()));
            }
            let mut result = Vec::new();
            for i in 0..y_vec.len() {
                let (i_prev, i_next) = if i == 0 {
                    (0, 1)
                } else if i == y_vec.len() - 1 {
                    (i - 1, i)
                } else {
                    (i - 1, i + 1)
                };
                let dy = eval_bin_op(BinaryOp::Sub, &y_vec[i_next], &y_vec[i_prev])?;
                let dx = eval_bin_op(BinaryOp::Sub, &x_vec[i_next], &x_vec[i_prev])?;
                let grad = eval_bin_op(BinaryOp::Div, &dy, &dx)?;
                result.push(grad);
            }
            Ok(Some(PhsValue::Vector(result)))
        }
        "trapz" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("trapz expects y and x vectors".into()));
            }
            let y_vec = match &args[0] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("trapz expects y vector".into())),
            };
            let x_vec = match &args[1] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("trapz expects x vector".into())),
            };
            if y_vec.len() != x_vec.len() || y_vec.len() < 2 {
                return Err(PhysureError::Generic("trapz expects equal length vectors with at least 2 elements".into()));
            }
            let mut total = PhsValue::None;
            let mut is_first = true;
            let two = PhsValue::Number(2.0);
            for i in 0..y_vec.len() - 1 {
                let dx = eval_bin_op(BinaryOp::Sub, &x_vec[i+1], &x_vec[i])?;
                let sum_y = eval_bin_op(BinaryOp::Add, &y_vec[i+1], &y_vec[i])?;
                let avg_y = eval_bin_op(BinaryOp::Div, &sum_y, &two)?;
                let area = eval_bin_op(BinaryOp::Mul, &avg_y, &dx)?;
                if is_first {
                    total = area;
                    is_first = false;
                } else {
                    total = eval_bin_op(BinaryOp::Add, &total, &area)?;
                }
            }
            Ok(Some(total))
        }
        _ => Ok(None),
    }
}

fn eval_calc_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "deriv" | "diff" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("deriv expects expression string and variable string".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("deriv expects expression string".into())),
            };
            let var_str = match &args[1] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("deriv expects variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let diff_node = node.diff_node(var_str)?.simplify();
            Ok(Some(PhsValue::String(diff_node.to_string())))
        }
        "integral" | "integrate" => {
            if args.len() == 4 {
                let expr_str = match &args[0] {
                    PhsValue::String(s) => s,
                    _ => return Err(PhysureError::Generic("integral expects expression string".into())),
                };
                let var_str = match &args[1] {
                    PhsValue::String(s) => s,
                    _ => return Err(PhysureError::Generic("integral expects variable string".into())),
                };

                let extract_bound = |v: &PhsValue| -> f64 {
                    match v {
                        PhsValue::Number(n) => *n,
                        PhsValue::Quantity(q) => q.value.mean(),
                        PhsValue::String(s) => match s.trim() {
                            "inf" | "+inf" | "infinity" | "oo" | "∞" => f64::INFINITY,
                            "-inf" | "-infinity" | "-oo" | "-∞" => f64::NEG_INFINITY,
                            _ => 0.0,
                        },
                        _ => 0.0,
                    }
                };

                let a = extract_bound(&args[2]);
                let b = extract_bound(&args[3]);

                let res_val = eval_definite_integral(expr_str, var_str, a, b, interpreter)?;
                return Ok(Some(PhsValue::Number(res_val)));
            }
            if args.len() != 2 {
                return Err(PhysureError::Generic("integral expects (expression_string, variable_string) or (expression_string, variable_string, a, b)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("integral expects expression string".into())),
            };
            let var_str = match &args[1] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("integral expects variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let int_node = node.integrate_node(var_str)?.simplify();
            Ok(Some(PhsValue::String(int_node.to_string())))
        }
        "limit" | "lim" => {
            if args.len() != 3 {
                return Err(PhysureError::Generic("limit expects (expression_string, variable_string, target_point)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("limit expects expression string".into())),
            };
            let var_str = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("limit expects variable string".into())),
            };

            let point_val = match &args[2] {
                PhsValue::Number(n) => *n,
                PhsValue::Quantity(q) => q.value.mean(),
                PhsValue::String(s) => match s.trim() {
                    "inf" | "+inf" | "infinity" | "oo" | "∞" => f64::INFINITY,
                    "-inf" | "-infinity" | "-oo" | "-∞" => f64::NEG_INFINITY,
                    _ => 0.0,
                },
                _ => 0.0,
            };

            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            if let Ok(program) = crate::parser::parse_phs(&inlined) {
                if let Some(stmt) = program.statements.first() {
                    let expr = match stmt {
                        crate::ast::Statement::Expr(e) => e.clone(),
                        crate::ast::Statement::Assignment(node) => node.value.clone(),
                        _ => return Ok(Some(PhsValue::Number(0.0))),
                    };
                    let test_val = if point_val.is_infinite() {
                        if point_val.is_sign_positive() { 1e7 } else { -1e7 }
                    } else {
                        point_val + 1e-8
                    };
                    let mut local_env = interpreter.env.clone();
                    let q_val = physure_core::Quantity::new_scalar(test_val, 0.0, physure_core::units::RationalUnit::dimensionless(), None, None);
                    local_env.insert(var_str.to_string(), PhsValue::Quantity(q_val));
                    if let Ok(val) = interpreter.eval_expr(&expr, &local_env) {
                        let num = match &val {
                            PhsValue::Number(n) => *n,
                            PhsValue::Quantity(q) => q.value.mean(),
                            _ => 0.0,
                        };
                        if num.abs() < 1e-5 {
                            return Ok(Some(PhsValue::Number(0.0)));
                        }
                        if num.abs() > 1e6 {
                            let sign = if num.is_sign_positive() { f64::INFINITY } else { f64::NEG_INFINITY };
                            return Ok(Some(PhsValue::Number(sign)));
                        }
                        return Ok(Some(val));
                    }
                }
            }
            Ok(Some(PhsValue::Number(0.0)))
        }
        "solve" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("solve expects equation string and target string".into()));
            }
            let target_str = match &args[1] {
                PhsValue::String(s) => s,
                _ => return Err(PhysureError::Generic("solve expects target string".into())),
            };
            let node = match &args[0] {
                PhsValue::String(eq_str) => {
                    let inlined = preprocess_symbolic_expression(eq_str, interpreter);
                    crate::symbolic::SymbolicParser::parse_str(&inlined)?
                }
                PhsValue::Equation(l, r) => crate::symbolic::Node::Sub(Box::new(l.clone()), Box::new(r.clone())),
                _ => return Err(PhysureError::Generic("solve expects an equation string or a previously-solved equation".into())),
            };
            let solved_node = node.solve_equation(target_str)?;
            let solved_str = solved_node.to_string();

            // if target resolves against bound quantities, evaluate the solved expression against the interpreter's env
            // The python tests might expect a Number/Quantity back if variables are bound
            if let Ok(program) = crate::parser::parse_phs(&solved_str) {
                if let Some(crate::ast::Statement::Expr(expr)) = program.statements.first() {
                    if !has_unbound_vars(expr, interpreter) {
                        if let Ok(val) = interpreter.eval_expr(expr, &interpreter.env) {
                            return Ok(Some(val));
                        }
                    }
                }
            }
            Ok(Some(PhsValue::Equation(crate::symbolic::Node::Symbol(target_str.clone()), solved_node)))
        }
        "substitute" | "sub" => {
            if args.len() != 3 {
                return Err(PhysureError::Generic("substitute expects (equation_or_string, target_symbol, replacement_or_equation)".into()));
            }
            let target_str = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("substitute target must be a symbol string".into())),
            };

            let replacement_str = match &args[2] {
                PhsValue::String(s) => {
                    if s.contains('=') {
                        s.split('=').nth(1).unwrap_or(s).trim().to_string()
                    } else {
                        s.clone()
                    }
                }
                PhsValue::Equation(_, r) => r.to_phs_string(),
                _ => args[2].to_string(),
            };

            match &args[0] {
                PhsValue::Equation(l, r) => {
                    let new_l_str = l.to_phs_string().replace(target_str, &format!("({})", replacement_str));
                    let new_r_str = r.to_phs_string().replace(target_str, &format!("({})", replacement_str));
                    let new_l = crate::symbolic::SymbolicParser::parse_str(&new_l_str)?;
                    let new_r = crate::symbolic::SymbolicParser::parse_str(&new_r_str)?;
                    Ok(Some(PhsValue::Equation(new_l, new_r)))
                }
                PhsValue::String(eq_str) => {
                    let parts: Vec<&str> = eq_str.split('=').collect();
                    if parts.len() == 2 {
                        let new_l_str = parts[0].trim().replace(target_str, &format!("({})", replacement_str));
                        let new_r_str = parts[1].trim().replace(target_str, &format!("({})", replacement_str));
                        let new_l = crate::symbolic::SymbolicParser::parse_str(&new_l_str)?;
                        let new_r = crate::symbolic::SymbolicParser::parse_str(&new_r_str)?;
                        Ok(Some(PhsValue::Equation(new_l, new_r)))
                    } else {
                        let new_str = eq_str.replace(target_str, &format!("({})", replacement_str));
                        let new_node = crate::symbolic::SymbolicParser::parse_str(&new_str)?;
                        Ok(Some(PhsValue::String(new_node.to_string())))
                    }
                }
                _ => Err(PhysureError::Generic("substitute base must be an equation or string".into())),
            }
        }
        _ => Ok(None),
    }
}

fn eval_plot_builtin(name: &str, args: &[PhsValue], _interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "plot" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("plot expects at least 1 argument".into()));
            }
            let title = if args.len() >= 3 {
                if let PhsValue::String(s) = &args[2] {
                    s.clone()
                } else {
                    "Physure Live Plot".to_string()
                }
            } else {
                "Physure Live Plot".to_string()
            };

            let ((x_arr, x_unit), (y_arr, y_unit)) = if args.len() >= 2 {
                (extract_vec_f64_and_unit(&args[0]), extract_vec_f64_and_unit(&args[1]))
            } else {
                let (y_a, y_u) = extract_vec_f64_and_unit(&args[0]);
                let x_a: Vec<f64> = (0..y_a.len()).map(|i| i as f64).collect();
                ((x_a, String::new()), (y_a, y_u))
            };

            let ascii_plot = draw_ascii_plot(&x_arr, &y_arr, &title, &x_unit, &y_unit);
            let svg_plot = draw_svg_plot(&x_arr, &y_arr, &title, &x_unit, &y_unit);
            Ok(Some(PhsValue::Plot(crate::value::PlotData {
                title,
                x_unit,
                y_unit,
                ascii: ascii_plot,
                svg: svg_plot,
            })))
        }
        _ => Ok(None),
    }
}

fn extract_vec_f64_and_unit(val: &PhsValue) -> (Vec<f64>, String) {
    match val {
        PhsValue::Number(n) => (vec![*n], String::new()),
        PhsValue::Quantity(q) => (vec![q.value.mean()], q.unit.__repr__()),
        PhsValue::Vector(vec) => {
            let mut nums = Vec::new();
            let mut unit_str = String::new();
            for item in vec {
                match item {
                    PhsValue::Number(n) => nums.push(*n),
                    PhsValue::Quantity(q) => {
                        nums.push(q.value.mean());
                        if unit_str.is_empty() {
                            unit_str = q.unit.__repr__();
                        }
                    }
                    _ => {}
                }
            }
            (nums, unit_str)
        }
        _ => (Vec::new(), String::new()),
    }
}

fn draw_ascii_plot(x: &[f64], y: &[f64], title: &str, x_unit: &str, y_unit: &str) -> String {
    if x.is_empty() || y.is_empty() {
        return format!("📊 {}: [No data points]", title);
    }
    let n = x.len().min(y.len());
    let mut pairs: Vec<(f64, f64)> = x[..n].iter().zip(y[..n].iter()).map(|(&a, &b)| (a, b)).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let x_min = pairs[0].0;
    let x_max = pairs.last().unwrap().0;

    let width = 46;
    let height = 8;

    let mut x_grid = Vec::with_capacity(width);
    let mut y_grid = Vec::with_capacity(width);

    for c in 0..width {
        let x_val = if width > 1 {
            x_min + (c as f64) * (x_max - x_min) / ((width - 1) as f64)
        } else {
            x_min
        };
        x_grid.push(x_val);

        // 1D Linear Interpolation
        let y_val = if pairs.len() == 1 {
            pairs[0].1
        } else if x_val <= pairs[0].0 {
            pairs[0].1
        } else if x_val >= pairs.last().unwrap().0 {
            pairs.last().unwrap().1
        } else {
            let mut val = pairs[0].1;
            for i in 0..pairs.len() - 1 {
                if x_val >= pairs[i].0 && x_val <= pairs[i + 1].0 {
                    let dx = pairs[i + 1].0 - pairs[i].0;
                    if dx.abs() > 1e-12 {
                        let t = (x_val - pairs[i].0) / dx;
                        val = pairs[i].1 + t * (pairs[i + 1].1 - pairs[i].1);
                    } else {
                        val = pairs[i].1;
                    }
                    break;
                }
            }
            val
        };
        y_grid.push(y_val);
    }

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for &val in &y_grid {
        if val < y_min { y_min = val; }
        if val > y_max { y_max = val; }
    }
    let y_span = if y_max != y_min { y_max - y_min } else { 1.0 };

    let fmt_x = if x_unit.is_empty() { String::new() } else { format!(" {}", x_unit) };
    let fmt_y = if y_unit.is_empty() { String::new() } else { format!(" {}", y_unit) };

    let mut lines = Vec::new();
    lines.push(format!("  📊 {}", title));

    let top_y_str = format!("  {:.*e}{}", 3, y_max, fmt_y);
    lines.push(format!("{:>18} ┐", top_y_str.trim()));

    for r in (0..height).rev() {
        let y_level = y_min + (r as f64 / (height - 1) as f64) * y_span;
        let mut row_chars = String::new();
        for c in 0..width {
            let val = y_grid[c];
            let diff = (val - y_level).abs() / y_span;
            if diff < (1.0 / (2.0 * height as f64)) {
                row_chars.push('█');
            } else if val > y_level {
                row_chars.push('░');
            } else {
                row_chars.push(' ');
            }
        }
        lines.push(format!("                   │ {}", row_chars));
    }

    let bot_y_str = format!("  {:.*e}{}", 3, y_min, fmt_y);
    lines.push(format!("{:>18} └{}", bot_y_str.trim(), "─".repeat(width)));

    let x_min_str = format!("{:.*e}{}", 3, x_min, fmt_x);
    let x_max_str = format!("{:.*e}{}", 3, x_max, fmt_x);
    let x_min_trim = x_min_str.trim();
    let x_max_trim = x_max_str.trim();
    let pad_len = if width + 12 > x_min_trim.len() + x_max_trim.len() {
        width + 12 - x_min_trim.len() - x_max_trim.len()
    } else {
        1
    };
    lines.push(format!("                     {}{}{}", x_min_trim, " ".repeat(pad_len), x_max_trim));

    lines.join("\n")
}

fn draw_svg_plot(x: &[f64], y: &[f64], title: &str, x_unit: &str, y_unit: &str) -> String {
    if x.is_empty() || y.is_empty() {
        return String::new();
    }
    let n = x.len().min(y.len());
    let mut pairs: Vec<(f64, f64)> = x[..n].iter().zip(y[..n].iter()).map(|(&a, &b)| (a, b)).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let x_min = pairs[0].0;
    let x_max = pairs.last().unwrap().0;

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for &(_, val) in &pairs {
        if val < y_min { y_min = val; }
        if val > y_max { y_max = val; }
    }
    let y_span = if y_max != y_min { y_max - y_min } else { 1.0 };
    let x_span = if x_max != x_min { x_max - x_min } else { 1.0 };

    let width = 600.0;
    let height = 350.0;
    let padding_left = 80.0;
    let padding_bottom = 50.0;
    let padding_top = 40.0;
    let padding_right = 30.0;

    let plot_w = width - padding_left - padding_right;
    let plot_h = height - padding_top - padding_bottom;

    let points: Vec<String> = pairs.iter().map(|&(px, py)| {
        let sx = padding_left + ((px - x_min) / x_span) * plot_w;
        let sy = padding_top + (1.0 - (py - y_min) / y_span) * plot_h;
        format!("{:.1},{:.1}", sx, sy)
    }).collect();

    let points_str = points.join(" ");

    let fill_first = format!("{:.1},{:.1}", padding_left, padding_top + plot_h);
    let fill_last = format!("{:.1},{:.1}", padding_left + plot_w, padding_top + plot_h);
    let fill_points = format!("{} {} {}", fill_first, points_str, fill_last);

    let x_label = if x_unit.is_empty() { "x".to_string() } else { format!("x ({})", x_unit) };
    let y_label = if y_unit.is_empty() { "y".to_string() } else { format!("y ({})", y_unit) };

    format!(
        r###"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" style="background-color:#1e1e1e; font-family:sans-serif;"><rect width="100%" height="100%" fill="#1e1e1e"/><text x="{title_x}" y="25" fill="#569cd6" font-size="14" font-weight="bold" text-anchor="middle">{title}</text><rect x="{pl}" y="{pt}" width="{pw}" height="{ph}" fill="#252526" stroke="#444444" stroke-width="1"/><polygon points="{fill_points}" fill="#4ec9b0" fill-opacity="0.15"/><polyline points="{points_str}" fill="none" stroke="#4ec9b0" stroke-width="2.5" stroke-linecap="round"/><text x="{pl}" y="{y_max_y}" fill="#cccccc" font-size="10" text-anchor="end" dx="-8">{y_max:.3e}</text><text x="{pl}" y="{y_min_y}" fill="#cccccc" font-size="10" text-anchor="end" dx="-8">{y_min:.3e}</text><text x="{pl}" y="{x_min_y}" fill="#cccccc" font-size="10" text-anchor="middle">{x_min:.3e}</text><text x="{x_max_x}" y="{x_min_y}" fill="#cccccc" font-size="10" text-anchor="middle">{x_max:.3e}</text><text x="{title_x}" y="{x_lbl_y}" fill="#cccccc" font-size="11" text-anchor="middle">{x_label}</text><text x="15" y="{y_lbl_y}" fill="#cccccc" font-size="11" text-anchor="middle" transform="rotate(-90 15 {y_lbl_y})">{y_label}</text></svg>"###,
        w = width, h = height,
        title_x = width / 2.0,
        title = title,
        pl = padding_left, pt = padding_top, pw = plot_w, ph = plot_h,
        fill_points = fill_points,
        points_str = points_str,
        y_max_y = padding_top + 12.0,
        y_min_y = padding_top + plot_h,
        x_min_y = padding_top + plot_h + 20.0,
        x_max_x = padding_left + plot_w,
        x_lbl_y = padding_top + plot_h + 38.0,
        y_lbl_y = padding_top + plot_h / 2.0,
        x_min = x_min, x_max = x_max, y_min = y_min, y_max = y_max,
        x_label = x_label, y_label = y_label
    )
}


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
        crate::ast::Expr::Identifier(s) => s.clone(),
        crate::ast::Expr::BinaryOp { op, left, right } => {
            let op_str = match op {
                crate::ast::BinaryOp::Add => "+",
                crate::ast::BinaryOp::Sub => "-",
                crate::ast::BinaryOp::Mul => "*",
                crate::ast::BinaryOp::Div => "/",
                crate::ast::BinaryOp::Pow => "^",
                crate::ast::BinaryOp::Convert => "=>",
            };
            format!("{} {} {}", expr_to_string(left), op_str, expr_to_string(right))
        }
        crate::ast::Expr::FunctionCall { name, args, .. } => {
            let args_str: Vec<String> = args.iter().map(expr_to_string).collect();
            format!("{}({})", name, args_str.join(", "))
        }
    }
}



fn preprocess_symbolic_expression(expr_str: &str, interpreter: &PhsInterpreter) -> String {
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

fn has_unbound_vars(expr: &crate::ast::Expr, interpreter: &PhsInterpreter) -> bool {
    match expr {
        crate::ast::Expr::Identifier(name) => {
            interpreter.get_var(name).is_none()
        }
        crate::ast::Expr::BinaryOp { left, right, .. } => {
            has_unbound_vars(left, interpreter) || has_unbound_vars(right, interpreter)
        }
        crate::ast::Expr::FunctionCall { args, .. } => {
            args.iter().any(|arg| has_unbound_vars(arg, interpreter))
        }
        _ => false,
    }
}

fn eval_definite_integral(expr_str: &str, var_str: &str, a: f64, b: f64, interpreter: &PhsInterpreter) -> PhysureResult<f64> {
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    let program = crate::parser::parse_phs(&inlined)?;
    let expr = match program.statements.first() {
        Some(crate::ast::Statement::Expr(e)) => e.clone(),
        Some(crate::ast::Statement::Assignment(node)) => node.value.clone(),
        _ => return Err(PhysureError::Generic("Failed to parse integrand expression".into())),
    };

    let mut local_env = interpreter.env.clone();
    let mut eval_at = |x: f64| -> f64 {
        local_env.insert(var_str.to_string(), PhsValue::Number(x));
        if let Ok(val) = interpreter.eval_expr(&expr, &local_env) {
            match val {
                PhsValue::Number(n) => n,
                PhsValue::Quantity(q) => q.value.mean(),
                _ => 0.0,
            }
        } else {
            0.0
        }
    };

    if a.is_infinite() || b.is_infinite() {
        let mut transform_eval = |t: f64| -> f64 {
            if t.abs() >= 1.0 - 1e-7 {
                return 0.0;
            }
            let x = t / (1.0 - t * t);
            let dxdt = (1.0 + t * t) / ((1.0 - t * t) * (1.0 - t * t));
            let fx = eval_at(x);
            if fx.is_nan() || fx.is_infinite() {
                0.0
            } else {
                fx * dxdt
            }
        };

        let t_a = if a == f64::NEG_INFINITY { -1.0 + 1e-6 } else if a == f64::INFINITY { 1.0 - 1e-6 } else { a / (1.0 + (1.0 + a * a).sqrt()) };
        let t_b = if b == f64::INFINITY { 1.0 - 1e-6 } else if b == f64::NEG_INFINITY { -1.0 + 1e-6 } else { b / (1.0 + (1.0 + b * b).sqrt()) };
        
        return Ok(quad_gauss_kronrod(&mut transform_eval, t_a, t_b, 100));
    }

    Ok(quad_gauss_kronrod(&mut eval_at, a, b, 100))
}

fn quad_gauss_kronrod<F>(mut f: F, a: f64, b: f64, steps: usize) -> f64
where F: FnMut(f64) -> f64 {
    let h = (b - a) / steps as f64;
    let mut sum = 0.0;
    for i in 0..steps {
        let x0 = a + i as f64 * h;
        let x1 = x0 + h;
        let mid = 0.5 * (x0 + x1);
        let half_h = 0.5 * h;
        let f0 = f(x0);
        let f1 = f(mid - half_h * 0.5773502691896257);
        let f2 = f(mid);
        let f3 = f(mid + half_h * 0.5773502691896257);
        let f4 = f(x1);
        sum += (half_h / 9.0) * (f0 + 4.0 * f2 + f4 + 2.0 * (f1 + f3));
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::PhsInterpreter;
    use crate::value::PhsValue;

    fn eval(name: &str, args: Vec<PhsValue>) -> PhsValue {
        let interp = PhsInterpreter::default();
        eval_core_builtin(name, &args, &interp).unwrap().unwrap()
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
        assert_eq!(eval("cos", vec![PhsValue::Number(0.0)]), PhsValue::Number(1.0));
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
}
