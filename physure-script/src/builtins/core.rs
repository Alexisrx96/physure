use physure_core::error::{PhysureError, PhysureResult};
use crate::value::PhsValue;
use crate::interpreter::PhsInterpreter;

/// `PhsValue::Bool` already carries `Display` ("True"/"False"), an `is_truthy` arm, a
/// debugger `Inspection` kind, plugin-ABI support and export serialization -- everything a
/// comparison result needs was already built around it. The dimensionless-`Quantity`
/// stand-in this used to build (magnitude 1.0/0.0) was also the one place out of step with
/// every codegen target, which already emits the target language's own comparison operator
/// (`>` in Python/Rust/Java/JS, natively a bool in each).
fn boolean(res: bool) -> PhsValue {
    PhsValue::Bool(res)
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

pub fn eval_core_builtin(
    name: &str,
    args: &[PhsValue],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
    match name {
        "parallel_map" => {
            let (func, vec) = match (args.first(), args.get(1)) {
                (Some(PhsValue::Function(f)), Some(PhsValue::Vector(v))) => (f, v.clone()),
                _ => return Err(PhysureError::Generic("parallel_map expects (fn, vector)".into())),
            };
            // A breakpoint inside a rayon worker closure isn't a coherent debugging
            // experience -- pausing one of N racing threads while the rest continue -- so a
            // debug session forces this back to plain sequential `.map()` instead of silently
            // ignoring any breakpoint set inside `func`.
            if interpreter.debug_hook_is_set() {
                let results: PhysureResult<Vec<PhsValue>> = vec
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| {
                        interpreter
                            .call_function_node(func, vec![item], env)
                            .map_err(|e| PhysureError::Generic(format!("parallel_map failed at index {i}: {e}")))
                    })
                    .collect();
                return Ok(Some(PhsValue::Vector(results?)));
            }
            use rayon::prelude::*;
            let results: Vec<PhsValue> = vec
                .into_par_iter()
                .enumerate()
                .map(|(i, item)| {
                    interpreter
                        .call_function_node(func, vec![item], env)
                        .map_err(|e| PhysureError::Generic(format!("parallel_map failed at index {i}: {e}")))
                })
                .collect::<PhysureResult<Vec<PhsValue>>>()?;
            Ok(Some(PhsValue::Vector(results)))
        }
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
            // Was its own, narrower truthiness check (Quantity/Number only, silently `false`
            // for anything else) instead of the shared `is_truthy` -- fine while every
            // comparison built a dimensionless Quantity, wrong the moment one builds a real
            // `PhsValue::Bool` instead.
            let cond_true = args.first().is_some_and(crate::interpreter::helpers::is_truthy);
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

