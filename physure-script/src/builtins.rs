use physure_core::error::{PhysureError, PhysureResult};
use physure_core::units::parser::Parser as UnitParser;
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
        "calc" => eval_calc_builtin(name, args, interpreter),
        "plot" => eval_plot_builtin_with_kwargs(name, args, kwargs, interpreter, env),
        "array" => eval_array_builtin(name, args, interpreter),
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
fn apply_format_spec(value: &PhsValue, spec: &str) -> String {
    // `base` quotes the measurement in the units it is built from — `2 kΩ: base` is
    // `2000 A^-2 * kg * m^2 * s^-3`. The scale moves into the magnitude, so the physical
    // value is untouched; only the terms change.
    if spec == "base" {
        return match value {
            PhsValue::Quantity(q) => q.base_display(),
            other => other.to_string(),
        };
    }
    // A range is its endpoints: the spec reaches both, or it silently reached neither.
    if let PhsValue::Range(start, end) = value {
        return format!("{} .. {}", apply_format_spec(start, spec), apply_format_spec(end, spec));
    }
    // `frac` writes the number as a fraction and `ifrac` as a mixed one — `1.5` is `3/2`
    // and `1 1/2`. Only when one applies: a number with no small fraction behind it keeps
    // its decimal rather than being rounded into a lie.
    let mixed_frac = match spec {
        "frac" => Some(false),
        "ifrac" => Some(true),
        _ => None,
    };
    let digits = spec.trim_start_matches('.').trim_end_matches(char::is_alphabetic);
    let precision: usize = digits.parse().unwrap_or(6);
    let kind = spec.chars().last().filter(|c| c.is_alphabetic()).unwrap_or('f');
    let render = |n: f64| match mixed_frac {
        Some(mixed) => physure_core::quantity::format_fraction(n, mixed)
            .unwrap_or_else(|| physure_core::quantity::format_float(n)),
        None => match kind {
            'e' => format!("{:.*e}", precision, n),
            'g' => {
                let s = format!("{:.*}", precision, n);
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
            _ => format!("{:.*}", precision, n),
        },
    };
    match value {
        PhsValue::Number(n) => render(*n),
        PhsValue::Quantity(q) => {
            let unit = q.unit.__repr__();
            // The spec says how many digits to show, not which half of the measurement to
            // keep: `g:.2f` on `9.81 +/- 0.05 m/s^2` printed `9.81 m/s^2` and the reader had
            // no way to tell the uncertainty had ever been there.
            let std_dev = q.value.std_dev();
            let value_str = if std_dev > 0.0 {
                format!("{} ± {}", render(q.value.mean()), render(std_dev))
            } else {
                render(q.value.mean())
            };
            if unit.is_empty() {
                value_str
            } else {
                format!("{} {}", value_str, unit)
            }
        }
        other => other.to_string(),
    }
}

pub fn eval_core_builtin(name: &str, args: &[PhsValue], _interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "format" => {
            let Some(val) = args.first() else { return Ok(None) };
            match args.get(1) {
                Some(PhsValue::String(spec)) => Ok(Some(PhsValue::String(apply_format_spec(val, spec)))),
                // No spec, nothing to apply — hand the value back untouched.
                _ => Ok(Some(val.clone())),
            }
        }
        "assert" | "exact_assert" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic(format!("{name} expects 2 arguments (actual, expected)")));
            }
            match (&args[0], &args[1]) {
                (PhsValue::Quantity(a), PhsValue::Quantity(b)) => {
                    if name == "assert" {
                        a.phs_assert(b)?;
                    } else {
                        a.phs_exact_assert(b)?;
                    }
                    Ok(Some(PhsValue::None))
                }
                _ => Err(PhysureError::Generic(format!("{name} expects two quantities"))),
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
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.sin()?))),
                _ => Err(PhysureError::Generic("sin expects a number or quantity".into())),
            }
        }
        "cos" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("cos expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.cos()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.cos()?))),
                _ => Err(PhysureError::Generic("cos expects a number or quantity".into())),
            }
        }
        "exp" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("exp expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.exp()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.exp()?))),
                _ => Err(PhysureError::Generic("exp expects a number or quantity".into())),
            }
        }
        "ln" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("ln expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.ln()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.ln()?))),
                _ => Err(PhysureError::Generic("ln expects a number or quantity".into())),
            }
        }
        "abs" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("abs expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.abs()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.abs()?))),
                _ => Err(PhysureError::Generic("abs expects a number or quantity".into())),
            }
        }
        "log" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("log expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.log10()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.log10()?))),
                _ => Err(PhysureError::Generic("log expects a number or quantity".into())),
            }
        }
        "tan" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("tan expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.tan()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.tan()?))),
                _ => Err(PhysureError::Generic("tan expects a number or quantity".into())),
            }
        }
        "floor" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("floor expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.floor()))),
                // Flooring the mean says nothing about how well it is known, so the unit
                // and the uncertainty ride along instead of being reset to zero.
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.floor()?))),
                _ => Err(PhysureError::Generic("floor expects number or quantity".into())),
            }
        }
        "ceil" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("ceil expects 1 argument".into()));
            }
            match &args[0] {
                PhsValue::Number(n) => Ok(Some(PhsValue::Number(n.ceil()))),
                PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.ceil()?))),
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
                    // Rounding the mean says nothing about how well it is known, so the
                    // uncertainty rides along untouched rather than being reset to zero.
                    let rounded = Quantity::new_scalar(
                        (q.value.mean() * factor).round() / factor,
                        q.value.std_dev(),
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

fn eval_array_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
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
        "dot" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("dot expects 2 vectors".into()));
            }
            let (v1, v2) = match (&args[0], &args[1]) {
                (PhsValue::Vector(v1), PhsValue::Vector(v2)) => (v1, v2),
                _ => return Err(PhysureError::Generic("dot expects 2 vectors".into())),
            };
            if v1.len() != v2.len() || v1.is_empty() {
                return Err(PhysureError::Generic("dot expects equal non-empty vector lengths".into()));
            }
            let mut sum = interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[0].clone(), v2[0].clone())?;
            for i in 1..v1.len() {
                let prod = interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[i].clone(), v2[i].clone())?;
                sum = interpreter.eval_binary_op_vals(BinaryOp::Add, sum, prod)?;
            }
            Ok(Some(sum))
        }
        "cross" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("cross expects 2 3D vectors".into()));
            }
            let (v1, v2) = match (&args[0], &args[1]) {
                (PhsValue::Vector(v1), PhsValue::Vector(v2)) => (v1, v2),
                _ => return Err(PhysureError::Generic("cross expects 2 3D vectors".into())),
            };
            if v1.len() != 3 || v2.len() != 3 {
                return Err(PhysureError::Generic("cross requires 3D vectors".into()));
            }
            let c1 = interpreter.eval_binary_op_vals(BinaryOp::Sub,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[1].clone(), v2[2].clone())?,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[2].clone(), v2[1].clone())?,
            )?;
            let c2 = interpreter.eval_binary_op_vals(BinaryOp::Sub,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[2].clone(), v2[0].clone())?,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[0].clone(), v2[2].clone())?,
            )?;
            let c3 = interpreter.eval_binary_op_vals(BinaryOp::Sub,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[0].clone(), v2[1].clone())?,
                interpreter.eval_binary_op_vals(BinaryOp::Mul, v1[1].clone(), v2[0].clone())?,
            )?;
            Ok(Some(PhsValue::Vector(vec![c1, c2, c3])))
        }
        "norm" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("norm expects 1 vector".into()));
            }
            let v = match &args[0] {
                PhsValue::Vector(v) => v,
                _ => return Err(PhysureError::Generic("norm expects vector".into())),
            };
            if v.is_empty() {
                return Err(PhysureError::Generic("norm expects non-empty vector".into()));
            }
            let mut sum = interpreter.eval_binary_op_vals(BinaryOp::Mul, v[0].clone(), v[0].clone())?;
            for i in 1..v.len() {
                let prod = interpreter.eval_binary_op_vals(BinaryOp::Mul, v[i].clone(), v[i].clone())?;
                sum = interpreter.eval_binary_op_vals(BinaryOp::Add, sum, prod)?;
            }
            let half = PhsValue::Number(0.5);
            let res = interpreter.eval_binary_op_vals(BinaryOp::Pow, sum, half)?;
            Ok(Some(res))
        }
        "transpose" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("transpose expects 1 matrix vector".into()));
            }
            let rows = match &args[0] {
                PhsValue::Vector(r) => r,
                _ => return Err(PhysureError::Generic("transpose expects 2D vector matrix".into())),
            };
            let mut matrix = Vec::new();
            for r in rows {
                match r {
                    PhsValue::Vector(cols) => matrix.push(cols.clone()),
                    _ => return Err(PhysureError::Generic("transpose expects 2D vector matrix".into())),
                }
            }
            if matrix.is_empty() {
                return Ok(Some(PhsValue::Vector(Vec::new())));
            }
            let num_rows = matrix.len();
            let num_cols = matrix[0].len();
            let mut transposed = vec![vec![PhsValue::None; num_rows]; num_cols];
            for r in 0..num_rows {
                for c in 0..num_cols {
                    transposed[c][r] = matrix[r][c].clone();
                }
            }
            let res_rows = transposed.into_iter().map(PhsValue::Vector).collect();
            Ok(Some(PhsValue::Vector(res_rows)))
        }
        "matmul" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("matmul expects 2 matrices".into()));
            }
            let extract_matrix = |v: &PhsValue| -> PhysureResult<Vec<Vec<PhsValue>>> {
                let rows = match v {
                    PhsValue::Vector(r) => r,
                    _ => return Err(PhysureError::Generic("matmul expects 2D vector matrix".into())),
                };
                let mut mat = Vec::new();
                for r in rows {
                    match r {
                        PhsValue::Vector(cols) => mat.push(cols.clone()),
                        _ => return Err(PhysureError::Generic("matmul expects 2D vector matrix".into())),
                    }
                }
                Ok(mat)
            };
            let m1 = extract_matrix(&args[0])?;
            let m2 = extract_matrix(&args[1])?;
            if m1.is_empty() || m2.is_empty() || m1[0].len() != m2.len() {
                return Err(PhysureError::Generic("Matrix multiplication dimension mismatch".into()));
            }
            let r1 = m1.len();
            let c1 = m1[0].len();
            let c2 = m2[0].len();
            let mut res_mat = Vec::with_capacity(r1);
            for r in 0..r1 {
                let mut row = Vec::with_capacity(c2);
                for c in 0..c2 {
                    let mut sum = interpreter.eval_binary_op_vals(BinaryOp::Mul, m1[r][0].clone(), m2[0][c].clone())?;
                    for k in 1..c1 {
                        let prod = interpreter.eval_binary_op_vals(BinaryOp::Mul, m1[r][k].clone(), m2[k][c].clone())?;
                        sum = interpreter.eval_binary_op_vals(BinaryOp::Add, sum, prod)?;
                    }
                    row.push(sum);
                }
                res_mat.push(PhsValue::Vector(row));
            }
            Ok(Some(PhsValue::Vector(res_mat)))
        }
        "det" => {
            if args.len() != 1 {
                return Err(PhysureError::Generic("det expects 1 square matrix".into()));
            }
            let rows = match &args[0] {
                PhsValue::Vector(r) => r,
                _ => return Err(PhysureError::Generic("det expects 2D vector matrix".into())),
            };
            let mut mat = Vec::new();
            for r in rows {
                match r {
                    PhsValue::Vector(cols) => mat.push(cols.clone()),
                    _ => return Err(PhysureError::Generic("det expects 2D vector matrix".into())),
                }
            }
            if mat.len() == 2 && mat[0].len() == 2 && mat[1].len() == 2 {
                let ad = interpreter.eval_binary_op_vals(BinaryOp::Mul, mat[0][0].clone(), mat[1][1].clone())?;
                let bc = interpreter.eval_binary_op_vals(BinaryOp::Mul, mat[0][1].clone(), mat[1][0].clone())?;
                let det = interpreter.eval_binary_op_vals(BinaryOp::Sub, ad, bc)?;
                return Ok(Some(det));
            }
            Err(PhysureError::Generic("det currently supports 2x2 matrices".into()))
        }
        _ => Ok(None),
    }
}

pub(crate) fn clean_diff_var(v: &str) -> &str {
    let s = v.trim();
    if s.starts_with("d(") && s.ends_with(')') && s.len() > 3 {
        &s[2..s.len() - 1]
    } else if s.starts_with('d') && s.len() > 1 && s[1..].chars().all(|c| c.is_alphanumeric() || c == '_') {
        &s[1..]
    } else {
        s
    }
}

pub(crate) fn parse_leibniz_single_arg(spec: &str) -> Option<(String, String, usize)> {
    let s = spec.trim();
    if s.contains('/') {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        let num = parts[0].trim();
        let den = parts[1].trim();

        if num == "d" || num.starts_with("d/") {
            if den.contains('(') && den.ends_with(')') {
                let p1 = den.find('(')?;
                let v = &den[..p1];
                let expr = &den[p1 + 1..den.len() - 1];
                return Some((expr.to_string(), clean_diff_var(v).to_string(), 1));
            }
        }

        let den_var = if den.starts_with('d') {
            let v = &den[1..];
            if let Some(hat_idx) = v.find('^') {
                clean_diff_var(&v[..hat_idx])
            } else if v.starts_with('(') && v.ends_with(')') {
                clean_diff_var(v)
            } else if v.chars().all(|c| c.is_alphanumeric() || c == '_') {
                clean_diff_var(den)
            } else {
                clean_diff_var(v)
            }
        } else {
            den
        };

        let (order, expr) = if num.starts_with("d^") {
            let hat_idx = num.find('^').unwrap();
            let rest = &num[hat_idx + 1..];
            let end_digit = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let ord = rest[..end_digit].parse::<usize>().unwrap_or(1);
            let e = rest[end_digit..].trim();
            (ord, e)
        } else if num.starts_with("d2") || num.starts_with("d3") || num.starts_with("d4") {
            let ord = num[1..2].parse::<usize>().unwrap_or(1);
            (ord, num[2..].trim())
        } else if num.starts_with('d') && num.len() > 1 {
            (1, num[1..].trim())
        } else {
            (1, num)
        };

        let expr_clean = if expr.starts_with('(') && expr.ends_with(')') {
            &expr[1..expr.len() - 1]
        } else {
            expr
        };

        if !expr_clean.is_empty() && !den_var.is_empty() {
            return Some((expr_clean.to_string(), den_var.to_string(), order));
        }
    }
    None
}

pub(crate) fn parse_differential_integrand(s: &str) -> Option<(String, String)> {
    let trimmed = s.trim();
    if let Some(space_idx) = trimmed.rfind(' ') {
        let last_word = trimmed[space_idx + 1..].trim();
        if last_word.starts_with('d') && last_word.len() > 1 && last_word[1..].chars().all(|c| c.is_alphanumeric() || c == '_') {
            let expr = trimmed[..space_idx].trim();
            let var = clean_diff_var(last_word);
            if !expr.is_empty() && !var.is_empty() {
                return Some((expr.to_string(), var.to_string()));
            }
        }
    }
    None
}

fn eval_calc_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    match name {
        "deriv" | "derivative" | "diff" => {
            if args.is_empty() || args.len() > 3 {
                return Err(PhysureError::Generic("deriv expects 1, 2, or 3 arguments: deriv(expr, [var], [order])".into()));
            }
            let (expr_str, var_str, order, explicit_dep) = if args.len() == 1 {
                let spec = match &args[0] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("deriv expects expression string".into())),
                };
                if let Some((e, v, o)) = parse_leibniz_single_arg(spec) {
                    let dep = if e.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\'') {
                        Some(e.clone())
                    } else {
                        None
                    };
                    (e, v, o, dep)
                } else {
                    return Err(PhysureError::Generic("deriv expects (expr_str, var_str) or Leibniz notation string like deriv(\"dy/dx\")".into()));
                }
            } else {
                let e = match &args[0] {
                    PhsValue::String(s) => s.to_string(),
                    _ => return Err(PhysureError::Generic("deriv expects expression string".into())),
                };
                let raw_v = match &args[1] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("deriv expects variable string".into())),
                };
                let v = clean_diff_var(raw_v).to_string();
                let o = if args.len() == 3 {
                    match &args[2] {
                        PhsValue::Number(n) => *n as usize,
                        PhsValue::Quantity(q) => q.value.mean() as usize,
                        _ => 1,
                    }
                } else {
                    1
                };
                let dep = if e.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\'') {
                    Some(e.clone())
                } else {
                    None
                };
                (e, v, o, dep)
            };

            let inlined = preprocess_symbolic_expression(&expr_str, interpreter);
            if let Some(idx) = inlined.find('=') {
                let lhs_str = &inlined[..idx];
                let rhs_str = &inlined[idx + 1..];
                let lhs = crate::symbolic::SymbolicParser::parse_str(lhs_str)?;
                let rhs = crate::symbolic::SymbolicParser::parse_str(rhs_str)?;
                let eq_node = crate::symbolic::Node::Equation(Box::new(lhs), Box::new(rhs));
                let diff_eq = eq_node.diff_node_n(&var_str, order)?;
                return Ok(Some(PhsValue::String(diff_eq.to_string())));
            }
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let dep_var_ref = explicit_dep.as_deref();
            let mut diff_node = node;
            for _ in 0..order {
                diff_node = diff_node.diff_node_implicit(&var_str, dep_var_ref)?.simplify();
            }
            Ok(Some(PhsValue::String(diff_node.to_string())))
        }
        "grad" | "gradient" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("grad expects (expression_string, variables_vector)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("grad expects expression string".into())),
            };
            let vars = extract_string_list(&args[1])?;
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let mut grads = Vec::new();
            for v in vars {
                let clean_v = clean_diff_var(&v);
                let d = node.diff_node(clean_v)?.simplify();
                grads.push(PhsValue::String(d.to_string()));
            }
            Ok(Some(PhsValue::Vector(grads)))
        }
        "div" | "divergence" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("div expects (vector_field, variables_vector)".into()));
            }
            let field_exprs = extract_string_list(&args[0])?;
            let vars = extract_string_list(&args[1])?;
            if field_exprs.len() != vars.len() {
                return Err(PhysureError::Generic("Vector field and variables dimension mismatch".into()));
            }
            let mut sum_nodes = Vec::new();
            for (f_str, v) in field_exprs.iter().zip(vars.iter()) {
                let clean_v = clean_diff_var(v);
                let inlined = preprocess_symbolic_expression(f_str, interpreter);
                let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
                let d = node.diff_node(clean_v)?.simplify();
                sum_nodes.push(d);
            }
            let div_node = crate::symbolic::Node::Add(sum_nodes).simplify();
            Ok(Some(PhsValue::String(div_node.to_string())))
        }
        "curl" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("curl expects (3D_vector_field, 3D_variables_vector)".into()));
            }
            let field = extract_string_list(&args[0])?;
            let vars = extract_string_list(&args[1])?;
            if field.len() != 3 || vars.len() != 3 {
                return Err(PhysureError::Generic("curl requires 3D vector field and 3D variables".into()));
            }
            let parse_node = |s: &str| -> PhysureResult<crate::symbolic::Node> {
                let inlined = preprocess_symbolic_expression(s, interpreter);
                crate::symbolic::SymbolicParser::parse_str(&inlined)
            };
            let (p, q, r) = (parse_node(&field[0])?, parse_node(&field[1])?, parse_node(&field[2])?);
            let (x, y, z) = (clean_diff_var(&vars[0]), clean_diff_var(&vars[1]), clean_diff_var(&vars[2]));

            let cx = crate::symbolic::Node::Sub(Box::new(r.diff_node(y)?), Box::new(q.diff_node(z)?)).simplify();
            let cy = crate::symbolic::Node::Sub(Box::new(p.diff_node(z)?), Box::new(r.diff_node(x)?)).simplify();
            let cz = crate::symbolic::Node::Sub(Box::new(q.diff_node(x)?), Box::new(p.diff_node(y)?)).simplify();

            Ok(Some(PhsValue::Vector(vec![
                PhsValue::String(cx.to_string()),
                PhsValue::String(cy.to_string()),
                PhsValue::String(cz.to_string()),
            ])))
        }
        "laplacian" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic("laplacian expects (expression_string, variables_vector)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplacian expects expression string".into())),
            };
            let vars = extract_string_list(&args[1])?;
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let mut second_diffs = Vec::new();
            for v in vars {
                let clean_v = clean_diff_var(&v);
                let d2 = node.diff_node_n(clean_v, 2)?.simplify();
                second_diffs.push(d2);
            }
            let lap_node = crate::symbolic::Node::Add(second_diffs).simplify();
            Ok(Some(PhsValue::String(lap_node.to_string())))
        }
        "series" | "taylor" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(PhysureError::Generic(
                    "series expects (expression_string, variable_string, [about], [order]); `order` is the highest power kept, default 6".into(),
                ));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("series expects expression string".into())),
            };
            let var = match &args[1] {
                PhsValue::String(s) => clean_diff_var(s).to_string(),
                _ => return Err(PhysureError::Generic("series expects variable string".into())),
            };
            let about = match args.get(2) {
                None => crate::symbolic::Node::Number(0.0),
                Some(PhsValue::Number(n)) => crate::symbolic::Node::Number(*n),
                Some(PhsValue::Quantity(q)) => crate::symbolic::Node::Number(q.value.mean()),
                Some(PhsValue::String(s)) => crate::symbolic::SymbolicParser::parse_str(s)?,
                Some(_) => {
                    return Err(PhysureError::Generic(
                        "series expansion point must be a number or an expression string".into(),
                    ))
                }
            };
            let order = match args.get(3) {
                None => 6,
                Some(PhsValue::Number(n)) => *n as usize,
                Some(PhsValue::Quantity(q)) => q.value.mean() as usize,
                Some(_) => {
                    return Err(PhysureError::Generic("series order must be a number".into()))
                }
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            Ok(Some(PhsValue::String(node.series(&var, &about, order)?.to_string())))
        }
        "simplify" | "expand" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("simplify expects an expression string".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("simplify expects expression string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            Ok(Some(PhsValue::String(node.simplify().to_string())))
        }
        "integral" | "integrate" => {
            if args.is_empty() || args.len() > 4 {
                return Err(PhysureError::Generic("integral expects (expression_string, variable_string) or (expression_string, variable_string, a, b)".into()));
            }
            let (expr_str, var_str) = if args.len() == 1 {
                let s = match &args[0] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("integral expects expression string".into())),
                };
                if let Some((e, v)) = parse_differential_integrand(s) {
                    (e, v)
                } else {
                    return Err(PhysureError::Generic("integral expects (expression_string, variable_string) or single differential string like integral(\"sin(x) dx\")".into()));
                }
            } else {
                let e = match &args[0] {
                    PhsValue::String(s) => s.clone(),
                    _ => return Err(PhysureError::Generic("integral expects expression string".into())),
                };
                let raw_v = match &args[1] {
                    PhsValue::String(s) => s.as_str(),
                    _ => return Err(PhysureError::Generic("integral expects variable string".into())),
                };
                (e, clean_diff_var(raw_v).to_string())
            };

            if args.len() == 4 {
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

                let res_val = eval_definite_integral(&expr_str, &var_str, a, b, interpreter)?;
                return Ok(Some(PhsValue::Number(res_val)));
            }

            let inlined = preprocess_symbolic_expression(&expr_str, interpreter);
            let node = crate::symbolic::SymbolicParser::parse_str(&inlined)?;
            let int_node = node.integrate_node(&var_str)?.simplify();
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
        "dsolve" => {
            if args.len() < 3 {
                return Err(PhysureError::Generic("dsolve expects 3 arguments: dsolve(eq_str, dep_var, indep_var)".into()));
            }
            let eq_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("dsolve expects equation string".into())),
            };
            let dep_var = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("dsolve expects dependent variable string".into())),
            };
            let indep_var = match &args[2] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("dsolve expects independent variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(eq_str, interpreter);
            let res = crate::symbolic::dsolve_str(&inlined, dep_var, indep_var)?;
            Ok(Some(PhsValue::String(res)))
        }
        "laplace" => {
            if args.len() < 3 {
                return Err(PhysureError::Generic("laplace expects 3 arguments: laplace(expr_str, t_var, s_var)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplace expects expression string".into())),
            };
            let t_var = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplace expects time variable string".into())),
            };
            let s_var = match &args[2] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("laplace expects s variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let res = crate::symbolic::laplace_str(&inlined, t_var, s_var)?;
            Ok(Some(PhsValue::String(res)))
        }
        "inv_laplace" | "inverse_laplace" => {
            if args.len() < 3 {
                return Err(PhysureError::Generic("inv_laplace expects 3 arguments: inv_laplace(expr_str, s_var, t_var)".into()));
            }
            let expr_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("inv_laplace expects expression string".into())),
            };
            let s_var = match &args[1] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("inv_laplace expects s variable string".into())),
            };
            let t_var = match &args[2] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("inv_laplace expects time variable string".into())),
            };
            let inlined = preprocess_symbolic_expression(expr_str, interpreter);
            let res = crate::symbolic::inv_laplace_str(&inlined, s_var, t_var)?;
            Ok(Some(PhsValue::String(res)))
        }
        "sym_det" => {
            let mat_str = match args.first() {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_det expects a symbolic matrix string, e.g. sym_det(\"[[a, b], [c, d]]\")".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let det_node = mat.det()?;
            Ok(Some(PhsValue::String(det_node.to_string())))
        }
        "sym_trace" => {
            let mat_str = match args.first() {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_trace expects a symbolic matrix string".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let tr_node = mat.trace()?;
            Ok(Some(PhsValue::String(tr_node.to_string())))
        }
        "sym_charpoly" => {
            if args.len() < 1 {
                return Err(PhysureError::Generic("sym_charpoly expects a matrix string and optional lambda variable".into()));
            }
            let mat_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_charpoly expects a symbolic matrix string".into())),
            };
            let lambda_var = if args.len() > 1 {
                match &args[1] {
                    PhsValue::String(s) => s.as_str(),
                    _ => "lambda",
                }
            } else {
                "lambda"
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let poly_node = mat.charpoly(lambda_var)?;
            Ok(Some(PhsValue::String(poly_node.to_string())))
        }
        "sym_eigenvalues" => {
            if args.is_empty() {
                return Err(PhysureError::Generic("sym_eigenvalues expects a symbolic matrix string".into()));
            }
            let mat_str = match &args[0] {
                PhsValue::String(s) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_eigenvalues expects a symbolic matrix string".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            let eigs = mat.eigenvalues("lambda")?;
            let eigs_str: Vec<String> = eigs.iter().map(|e| e.to_string()).collect();
            Ok(Some(PhsValue::String(format!("[{}]", eigs_str.join(", ")))))
        }
        "sym_transpose" => {
            let mat_str = match args.first() {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("sym_transpose expects a symbolic matrix string".into())),
            };
            let mat = crate::symbolic::SymMatrix::parse_str(mat_str)?;
            Ok(Some(PhsValue::String(mat.transpose().to_phs_string())))
        }
        _ => Ok(None),
    }
}

#[allow(dead_code)]
fn eval_plot_builtin(name: &str, args: &[PhsValue], interpreter: &PhsInterpreter) -> PhysureResult<Option<PhsValue>> {
    let empty_env = std::collections::HashMap::new();
    eval_plot_builtin_with_kwargs(name, args, &[], interpreter, &empty_env)
}

fn eval_plot_builtin_with_kwargs(
    name: &str,
    args: &[PhsValue],
    kwargs: &[(String, PhsValue)],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
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
        "plot3d" | "export3d" | "export_3d" => {
            let fn_val = args.get(0).cloned();
            let mut x_min = -2.0; let mut x_max = 2.0; let mut x_unit = "m".to_string();
            let mut y_min = -2.0; let mut y_max = 2.0; let mut y_unit = "m".to_string();
            let mut title = "3D Surface Plot".to_string();
            let mut filename = "plot_3d.stl".to_string();
            let mut format_name = "stl".to_string();

            for (k, v) in kwargs {
                match k.as_str() {
                    "x" | "x_range" => {
                        if let PhsValue::Range(start, end) = v {
                            if let PhsValue::Quantity(q) = start.as_ref() {
                                x_min = q.value.mean();
                                x_unit = q.unit.__repr__();
                            } else if let PhsValue::Number(n) = start.as_ref() {
                                x_min = *n;
                            }
                            if let PhsValue::Quantity(q) = end.as_ref() {
                                x_max = q.value.mean();
                            } else if let PhsValue::Number(n) = end.as_ref() {
                                x_max = *n;
                            }
                        }
                    }
                    "y" | "y_range" => {
                        if let PhsValue::Range(start, end) = v {
                            if let PhsValue::Quantity(q) = start.as_ref() {
                                y_min = q.value.mean();
                                y_unit = q.unit.__repr__();
                            } else if let PhsValue::Number(n) = start.as_ref() {
                                y_min = *n;
                            }
                            if let PhsValue::Quantity(q) = end.as_ref() {
                                y_max = q.value.mean();
                            } else if let PhsValue::Number(n) = end.as_ref() {
                                y_max = *n;
                            }
                        }
                    }
                    "title" => {
                        if let PhsValue::String(s) = v { title = s.clone(); }
                    }
                    "file" | "filename" => {
                        if let PhsValue::String(s) = v {
                            filename = s.clone();
                            if filename.contains('.') {
                                format_name = filename.split('.').last().unwrap_or("stl").to_string();
                            }
                        }
                    }
                    "format" => {
                        if let PhsValue::String(s) = v { format_name = s.clone(); }
                    }
                    _ => {}
                }
            }

            if let Some(PhsValue::String(s)) = args.get(1) {
                if name == "plot3d" {
                    title = s.clone();
                } else {
                    filename = s.clone();
                    if filename.contains('.') {
                        format_name = filename.split('.').last().unwrap_or("stl").to_string();
                    }
                }
            }
            if let Some(PhsValue::String(s)) = args.get(2) {
                if name != "plot3d" {
                    format_name = s.clone();
                }
            }

            let steps = 25;
            let mut x_grid = Vec::with_capacity(steps);
            let mut y_grid = Vec::with_capacity(steps);
            let mut z_grid = Vec::with_capacity(steps * steps);

            for i in 0..steps {
                x_grid.push(x_min + (i as f64) * (x_max - x_min) / ((steps - 1) as f64));
                y_grid.push(y_min + (i as f64) * (y_max - y_min) / ((steps - 1) as f64));
            }

            let mut z_unit = "".to_string();

            if let Some(PhsValue::Function(func)) = &fn_val {
                if title == "3D Surface Plot" {
                    title = format!("3D Surface: fn {}({}, {})", func.name, func.params.get(0).cloned().unwrap_or("x".into()), func.params.get(1).cloned().unwrap_or("y".into()));
                }
                let x_q_unit = UnitParser::parse_expression(&x_unit).ok();
                let y_q_unit = UnitParser::parse_expression(&y_unit).ok();

                for r in 0..steps {
                    for c in 0..steps {
                        let x_q = if let Some(ref u) = x_q_unit { PhsValue::Quantity(physure_core::quantity::Quantity::new_scalar(x_grid[c], 0.0, u.clone(), None, None)) } else { PhsValue::Number(x_grid[c]) };
                        let y_q = if let Some(ref u) = y_q_unit { PhsValue::Quantity(physure_core::quantity::Quantity::new_scalar(y_grid[r], 0.0, u.clone(), None, None)) } else { PhsValue::Number(y_grid[r]) };

                        let res = interpreter.call_function_node(func, vec![x_q, y_q], env)?;
                        match res {
                            PhsValue::Quantity(q) => {
                                if z_unit.is_empty() { z_unit = q.unit.__repr__(); }
                                z_grid.push(q.value.mean());
                            }
                            PhsValue::Number(n) => {
                                z_grid.push(n);
                            }
                            _ => z_grid.push(0.0),
                        }
                    }
                }
            } else if let Some(PhsValue::String(expr_str)) = &fn_val {
                let inlined = preprocess_symbolic_expression(expr_str, interpreter);
                let program = crate::parser::parse_phs(&inlined)?;
                let expr = match program.statements.first() {
                    Some(crate::ast::Statement::Expr(e)) => e.clone(),
                    Some(crate::ast::Statement::Assignment(node)) => node.value.clone(),
                    _ => return Err(PhysureError::Generic("Failed to parse 3D expression".into())),
                };

                for r in 0..steps {
                    let y = y_grid[r];
                    for c in 0..steps {
                        let x = x_grid[c];
                        let mut local_env = env.clone();
                        local_env.insert("x".to_string(), PhsValue::Number(x));
                        local_env.insert("y".to_string(), PhsValue::Number(y));
                        let z = match interpreter.eval_expr(&expr, &local_env) {
                            Ok(PhsValue::Number(n)) => n,
                            Ok(PhsValue::Quantity(q)) => {
                                if z_unit.is_empty() { z_unit = q.unit.__repr__(); }
                                q.value.mean()
                            }
                            _ => 0.0,
                        };
                        z_grid.push(z);
                    }
                }
            } else {
                return Err(PhysureError::Generic("plot3d/export3d expects a function (e.g. fn P(x, y)) or expression string as first argument".into()));
            }

            let clean_z = physure_core::plotting::sanitize_unit_label(if z_unit.is_empty() { "units" } else { &z_unit });
            let mesh_data = physure_core::plotting::Mesh3DData::new(
                &title,
                &format!("x ({})", x_unit),
                &format!("y ({})", y_unit),
                &format!("z ({})", clean_z),
                x_grid,
                y_grid,
                z_grid,
                steps,
                steps,
            );

            if name == "plot3d" {
                let html_str = mesh_data.export_html_threejs();
                let ascii_plot = draw_3d_surface_ascii(&title, &title, interpreter)?;
                Ok(Some(PhsValue::Plot(crate::value::PlotData {
                    title,
                    x_unit: x_unit,
                    y_unit: y_unit,
                    ascii: ascii_plot,
                    svg: html_str,
                })))
            } else {
                let bytes = mesh_data.export_format(&format_name)?;
                std::fs::write(&filename, &bytes)
                    .map_err(|e| PhysureError::Generic(format!("Failed to write {}: {}", filename, e)))?;
                Ok(Some(PhsValue::String(format!("✓ Exported 3D mesh '{}' ({})", filename, format_name))))
            }
        }
        "plot_field" => {
            let u_expr = match args.get(0) {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("plot_field expects u(x, y) expression string".into())),
            };
            let v_expr = match args.get(1) {
                Some(PhsValue::String(s)) => s.as_str(),
                _ => return Err(PhysureError::Generic("plot_field expects v(x, y) expression string".into())),
            };
            let title = match args.get(2) {
                Some(PhsValue::String(s)) => s.clone(),
                _ => format!("Vector Field Plot: F = ({}, {})", u_expr, v_expr),
            };

            let svg_plot = draw_vector_field_svg(u_expr, v_expr, &title, interpreter)?;
            let ascii_plot = draw_vector_field_ascii(u_expr, v_expr, &title, interpreter)?;

            Ok(Some(PhsValue::Plot(crate::value::PlotData {
                title,
                x_unit: "x".to_string(),
                y_unit: "y".to_string(),
                ascii: ascii_plot,
                svg: svg_plot,
            })))
        }
        "plot_nd" => {
            let title = match args.get(1) {
                Some(PhsValue::String(s)) => s.clone(),
                _ => "N-Dimensional Parallel Coordinates Plot".to_string(),
            };

            let (svg_plot, ascii_plot) = draw_nd_parallel_coords_svg(&args[0], &title)?;

            Ok(Some(PhsValue::Plot(crate::value::PlotData {
                title,
                x_unit: "dim".to_string(),
                y_unit: "val".to_string(),
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

fn extract_string_list(val: &PhsValue) -> PhysureResult<Vec<String>> {
    match val {
        PhsValue::Vector(vec) => {
            let mut list = Vec::new();
            for item in vec {
                match item {
                    PhsValue::String(s) => list.push(s.clone()),
                    _ => list.push(item.to_string()),
                }
            }
            Ok(list)
        }
        PhsValue::String(s) => Ok(s.split(',').map(|item| item.trim().to_string()).collect()),
        _ => Err(PhysureError::Generic("Expected vector or comma-separated string list".into())),
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

    // Try Symbolic First
    if !a.is_infinite() && !b.is_infinite() {
        if let Ok(node) = crate::symbolic::SymbolicParser::parse_str(&inlined) {
            if let Ok(int_node) = node.integrate_node(var_str) {
                let int_str = int_node.simplify().to_string();
                if let Ok(int_prog) = crate::parser::parse_phs(&int_str) {
                    if let Some(crate::ast::Statement::Expr(int_expr)) = int_prog.statements.first() {
                        let mut env_a = interpreter.env.clone();
                        env_a.insert(var_str.to_string(), PhsValue::Number(a));
                        let mut env_b = interpreter.env.clone();
                        env_b.insert(var_str.to_string(), PhsValue::Number(b));
                        
                        let val_b = interpreter.eval_expr(int_expr, &env_b);
                        let val_a = interpreter.eval_expr(int_expr, &env_a);

                        let get_num = |v: PhysureResult<PhsValue>| -> Option<f64> {
                            match v.ok()? {
                                PhsValue::Number(n) => Some(n),
                                PhsValue::Quantity(q) => Some(q.value.mean()),
                                _ => None,
                            }
                        };
                        if let (Some(nb), Some(na)) = (get_num(val_b), get_num(val_a)) {
                            return Ok(nb - na);
                        }
                    }
                }
            }
        }
    }


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

#[allow(dead_code)]
fn draw_3d_surface_svg(expr_str: &str, title: &str, interpreter: &PhsInterpreter) -> PhysureResult<String> {
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    let program = crate::parser::parse_phs(&inlined)?;
    let expr = match program.statements.first() {
        Some(crate::ast::Statement::Expr(e)) => e.clone(),
        Some(crate::ast::Statement::Assignment(node)) => node.value.clone(),
        _ => return Err(PhysureError::Generic("Failed to parse 3D expression".into())),
    };

    let steps = 15;
    let x_min = -2.0; let x_max = 2.0;
    let y_min = -2.0; let y_max = 2.0;

    let mut grid = vec![vec![0.0; steps]; steps];
    let mut z_min = f64::INFINITY;
    let mut z_max = f64::NEG_INFINITY;

    for i in 0..steps {
        let x = x_min + (i as f64) * (x_max - x_min) / ((steps - 1) as f64);
        for j in 0..steps {
            let y = y_min + (j as f64) * (y_max - y_min) / ((steps - 1) as f64);
            let mut env = interpreter.env.clone();
            env.insert("x".to_string(), PhsValue::Number(x));
            env.insert("y".to_string(), PhsValue::Number(y));
            let z = match interpreter.eval_expr(&expr, &env) {
                Ok(PhsValue::Number(n)) => n,
                Ok(PhsValue::Quantity(q)) => q.value.mean(),
                _ => 0.0,
            };
            grid[i][j] = z;
            if z < z_min { z_min = z; }
            if z > z_max { z_max = z; }
        }
    }

    let z_span = if z_max != z_min { z_max - z_min } else { 1.0 };
    let width = 600.0;
    let height = 400.0;
    let cx = width / 2.0;
    let cy = height / 2.0 + 40.0;

    let project = |x: f64, y: f64, z: f64| -> (f64, f64) {
        let norm_z = (z - z_min) / z_span - 0.5;
        let px = cx + (x - y) * 55.0;
        let py = cy - norm_z * 90.0 + (x + y) * 28.0;
        (px, py)
    };

    struct SvgPoly {
        depth: f64,
        svg: String,
    }
    let mut polys: Vec<SvgPoly> = Vec::new();

    for i in 0..steps - 1 {
        let x0 = x_min + (i as f64) * (x_max - x_min) / ((steps - 1) as f64);
        let x1 = x_min + ((i + 1) as f64) * (x_max - x_min) / ((steps - 1) as f64);
        for j in 0..steps - 1 {
            let y0 = y_min + (j as f64) * (y_max - y_min) / ((steps - 1) as f64);
            let y1 = y_min + ((j + 1) as f64) * (y_max - y_min) / ((steps - 1) as f64);

            let z00 = grid[i][j];
            let z10 = grid[i+1][j];
            let z11 = grid[i+1][j+1];
            let z01 = grid[i][j+1];

            let p00 = project(x0, y0, z00);
            let p10 = project(x1, y0, z10);
            let p11 = project(x1, y1, z11);
            let p01 = project(x0, y1, z01);

            let avg_z = (z00 + z10 + z11 + z01) / 4.0;
            let norm_avg_z = (avg_z - z_min) / z_span;
            let hue = (240.0 - norm_avg_z * 240.0).clamp(0.0, 240.0);

            // Painter's algorithm depth sorting (back to front)
            let depth = (i + j) as f64 + (1.0 - norm_avg_z) * 0.2;

            let svg_str = format!(
                "<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"hsl({:.0},85%,55%)\" stroke=\"rgba(255,255,255,0.25)\" stroke-width=\"0.6\" opacity=\"0.95\"/>",
                p00.0, p00.1, p10.0, p10.1, p11.0, p11.1, p01.0, p01.1, hue
            );

            polys.push(SvgPoly { depth, svg: svg_str });
        }
    }

    // Sort polygons from back to front
    polys.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap_or(std::cmp::Ordering::Equal));
    let svg_polys: Vec<String> = polys.into_iter().map(|p| p.svg).collect();

    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 600 400\" width=\"100%\" height=\"100%\"><rect width=\"100%\" height=\"100%\" fill=\"#0d1117\"/><text x=\"300\" y=\"30\" text-anchor=\"middle\" fill=\"#58a6ff\" font-family=\"sans-serif\" font-size=\"16\" font-weight=\"bold\">{}</text>{}</svg>",
        title, svg_polys.join("\n")
    ))
}

fn draw_3d_surface_ascii(_expr_str: &str, title: &str, _interpreter: &PhsInterpreter) -> PhysureResult<String> {
    Ok(format!("🏔️ {} [3D Surface View]", title))
}

fn draw_vector_field_svg(u_expr: &str, v_expr: &str, title: &str, interpreter: &PhsInterpreter) -> PhysureResult<String> {
    let parse_expr = |s: &str| -> PhysureResult<crate::ast::Expr> {
        let inlined = preprocess_symbolic_expression(s, interpreter);
        let program = crate::parser::parse_phs(&inlined)?;
        match program.statements.first() {
            Some(crate::ast::Statement::Expr(e)) => Ok(e.clone()),
            Some(crate::ast::Statement::Assignment(node)) => Ok(node.value.clone()),
            _ => Err(PhysureError::Generic("Failed to parse field expression".into())),
        }
    };
    let u_ast = parse_expr(u_expr)?;
    let v_ast = parse_expr(v_expr)?;

    let grid_size = 12;
    let plot_w = 480.0;
    let plot_h = 300.0;
    let padding_left = 60.0;
    let padding_top = 50.0;

    let mut arrows = Vec::new();

    for i in 0..grid_size {
        let gx = -2.0 + (i as f64) * 4.0 / ((grid_size - 1) as f64);
        let sx = padding_left + (i as f64) * plot_w / ((grid_size - 1) as f64);
        for j in 0..grid_size {
            let gy = -2.0 + (j as f64) * 4.0 / ((grid_size - 1) as f64);
            let sy = padding_top + (1.0 - (j as f64) / ((grid_size - 1) as f64)) * plot_h;

            let mut env = interpreter.env.clone();
            env.insert("x".to_string(), PhsValue::Number(gx));
            env.insert("y".to_string(), PhsValue::Number(gy));

            let u = match interpreter.eval_expr(&u_ast, &env) { Ok(PhsValue::Number(n)) => n, Ok(PhsValue::Quantity(q)) => q.value.mean(), _ => 0.0 };
            let v = match interpreter.eval_expr(&v_ast, &env) { Ok(PhsValue::Number(n)) => n, Ok(PhsValue::Quantity(q)) => q.value.mean(), _ => 0.0 };

            let len = (u * u + v * v).sqrt();
            let scale = if len > 1e-6 { (15.0 / len).min(18.0) } else { 0.0 };
            let ex = sx + u * scale;
            let ey = sy - v * scale;

            let hue = (240.0 - (len / 5.0).min(1.0) * 240.0).clamp(0.0, 240.0);

            arrows.push(format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"hsl({:.0},80%,60%)\" stroke-width=\"2\" marker-end=\"url(#arrow)\"/>",
                sx, sy, ex, ey, hue
            ));
        }
    }

    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 600 400\" width=\"100%\" height=\"100%\"><defs><marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"5\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#58a6ff\"/></marker></defs><rect width=\"100%\" height=\"100%\" fill=\"#0d1117\"/><text x=\"300\" y=\"30\" text-anchor=\"middle\" fill=\"#58a6ff\" font-family=\"sans-serif\" font-size=\"16\" font-weight=\"bold\">{}</text>{}</svg>",
        title, arrows.join("\n")
    ))
}

fn draw_vector_field_ascii(_u_expr: &str, _v_expr: &str, title: &str, _interpreter: &PhsInterpreter) -> PhysureResult<String> {
    Ok(format!("↗️ {} [Vector Field View]", title))
}

fn draw_nd_parallel_coords_svg(val: &PhsValue, title: &str) -> PhysureResult<(String, String)> {
    let extract_rows = match val {
        PhsValue::Vector(rows) => rows,
        _ => return Err(PhysureError::Generic("plot_nd expects a 2D matrix of data points".into())),
    };
    let mut matrix: Vec<Vec<f64>> = Vec::new();
    for r in extract_rows {
        match r {
            PhsValue::Vector(cols) => {
                let row_nums: Vec<f64> = cols.iter().map(|item| match item {
                    PhsValue::Number(n) => *n,
                    PhsValue::Quantity(q) => q.value.mean(),
                    _ => 0.0,
                }).collect();
                matrix.push(row_nums);
            }
            _ => {}
        }
    }
    if matrix.is_empty() {
        return Err(PhysureError::Generic("plot_nd matrix is empty".into()));
    }
    let num_dims = matrix[0].len();
    let num_samples = matrix.len();

    let width = 600.0;
    let height = 400.0;
    let padding_left = 60.0;
    let padding_right = 40.0;
    let padding_top = 60.0;
    let padding_bottom = 50.0;
    let plot_w = width - padding_left - padding_right;
    let plot_h = height - padding_top - padding_bottom;

    let mut svg_lines = Vec::new();
    for row_idx in 0..num_samples {
        let mut path_pts = Vec::new();
        for dim in 0..num_dims {
            let sx = padding_left + (dim as f64) * plot_w / ((num_dims - 1).max(1) as f64);
            let val = matrix[row_idx][dim];
            let sy = padding_top + (1.0 - (val / 10.0).clamp(-1.0, 1.0) * 0.5 - 0.5) * plot_h;
            path_pts.push(format!("{:.1},{:.1}", sx, sy));
        }
        let hue = (row_idx as f64 * 360.0 / num_samples as f64) % 360.0;
        svg_lines.push(format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"hsl({:.0},70%,60%)\" stroke-width=\"2\" opacity=\"0.75\"/>",
            path_pts.join(" "), hue
        ));
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 600 400\" width=\"100%\" height=\"100%\"><rect width=\"100%\" height=\"100%\" fill=\"#0d1117\"/><text x=\"300\" y=\"35\" text-anchor=\"middle\" fill=\"#58a6ff\" font-family=\"sans-serif\" font-size=\"16\" font-weight=\"bold\">{}</text>{}</svg>",
        title, svg_lines.join("\n")
    );
    let ascii = format!("🌐 {} [N-D Parallel Coordinates: {} dimensions, {} samples]", title, num_dims, num_samples);
    Ok((svg, ascii))
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
