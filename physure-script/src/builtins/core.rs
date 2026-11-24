use physure_core::error::{PhysureError, PhysureResult};
use crate::value::PhsValue;
use crate::interpreter::PhsInterpreter;

/// `PhsValue::Bool` already carries `Display` ("True"/"False"), is what a condition must be, a
/// debugger `Inspection` kind, plugin-ABI support and export serialization -- everything a
/// comparison result needs was already built around it. The dimensionless-`Quantity`
/// stand-in this used to build (magnitude 1.0/0.0) was also the one place out of step with
/// every codegen target, which already emits the target language's own comparison operator
/// (`>` in Python/Rust/Java/JS, natively a bool in each).
fn boolean(res: bool) -> PhsValue {
    PhsValue::Bool(res)
}

/// Names an operand for an error message. A present argument (including one whose value is
/// the real `PhsValue::None`) renders as its `type_name()`; a genuinely absent argument slot
/// (reachable only via a direct call like `op_eq(True)` with the wrong arity, since normal
/// `==` syntax always supplies exactly 2 args) renders distinctly so the two cases can't be
/// confused in the message.
fn describe_arg(v: Option<&PhsValue>) -> &'static str {
    match v {
        Some(val) => val.type_name(),
        None => "a missing argument",
    }
}

/// A comparison of operand types it has no meaning for is a type error naming both: answering `false` let
/// `vector(1, 2) == vector(1, 2)`, `1 m != "a"` and `1 m < 20 m +/- 1 sigma` slip through a condition as answers.
fn cannot_compare(args: &[PhsValue], symbol: &str) -> PhysureError {
    PhysureError::Generic(format!(
        "cannot compare {} and {} with {}",
        describe_arg(args.first()),
        describe_arg(args.get(1)),
        symbol,
    ))
}

/// A number or a quantity as a quantity (a number is a dimensionless one); `None` for any other value.
fn as_quantity(v: &PhsValue) -> Option<physure_core::Quantity> {
    match v {
        PhsValue::Quantity(q) => Some(q.clone()),
        PhsValue::Number(n) => Some(physure_core::Quantity::new_scalar(
            *n,
            0.0,
            physure_core::units::RationalUnit::dimensionless(),
            None,
            None,
        )),
        _ => None,
    }
}

/// The two operands of a comparison, both numbers or quantities, or a type error.
fn quantities(args: &[PhsValue], symbol: &str) -> PhysureResult<(physure_core::Quantity, physure_core::Quantity)> {
    match (args.first().and_then(as_quantity), args.get(1).and_then(as_quantity)) {
        (Some(l), Some(r)) if args.len() == 2 => Ok((l, r)),
        _ => Err(cannot_compare(args, symbol)),
    }
}

/// `<`, `>`, `<=`, `>=` on two numbers or quantities: `Quantity::phs_compare`, by their canonical magnitudes
/// (`1 km > 999 m`), different dimensions an error, a zero written without a unit an error (`x > 0 m`), a level
/// against a plain number an error (`Quantity::comparable_magnitudes`). Transpiled code calls the same method.
/// Nothing else has an order: two strings compared their equality as `0 < 1`, which made `"b" < "a"` true.
fn order(args: &[PhsValue], symbol: &str) -> PhysureResult<Option<PhsValue>> {
    let (l, r) = quantities(args, symbol)?;
    Ok(Some(boolean(l.phs_compare(&r, symbol)?)))
}

/// PHS `==`, the one definition `!=` negates:
/// * two numbers or quantities: `Quantity::phs_eq` (canonical magnitudes within a relative 1e-12);
/// * a number or quantity and a `t +/- k sigma` bound, in either order: [`within_sigma`];
/// * two booleans, or two strings: whether they are the same;
/// * any other pair (vectors and matrices included, whose `==` the language does not define): a type error.
fn phs_equal(args: &[PhsValue], symbol: &str) -> PhysureResult<bool> {
    match (args.first(), args.get(1)) {
        _ if args.len() != 2 => Err(cannot_compare(args, symbol)),
        (Some(PhsValue::Bool(l)), Some(PhsValue::Bool(r))) => Ok(l == r),
        (Some(PhsValue::String(l)), Some(PhsValue::String(r))) => Ok(l == r),
        (Some(PhsValue::SigmaBound(target, k)), Some(measured)) | (Some(measured), Some(PhsValue::SigmaBound(target, k))) => {
            match as_quantity(measured) {
                Some(measured) => within_sigma(&measured, target, *k),
                None => Err(cannot_compare(args, symbol)),
            }
        }
        _ => {
            let (l, r) = quantities(args, symbol)?;
            l.phs_eq(&r)
        }
    }
}

/// `x == t +/- k sigma`: does the measured `x` lie within `k` of its standard uncertainties of the exact target
/// `t`? The difference `d = x - t` is taken by the core (units made consistent, a temperature point or a level read
/// as the core reads them), and the answer is `|mean(d)| <= k * u(d)`, so it is the same in every unit.
///
/// An exact `x` has no sigma to count in, and is an error: the old default of `0.05` (in whatever unit the left
/// operand had) made `10.0 mm == 10.04 mm +/- 1 sigma` true by a margin of 0.05 mm, or 0.05 m.
fn within_sigma(measured: &physure_core::Quantity, target: &physure_core::Quantity, k: f64) -> PhysureResult<bool> {
    if !(k.is_finite() && k >= 0.0) {
        return Err(PhysureError::Generic(format!("a +/- k sigma comparison needs a finite k >= 0, not {k}")));
    }
    let d = measured.sub(target)?;
    let u = d.value.std_dev();
    if u == 0.0 {
        return Err(PhysureError::Generic(
            "a ± sigma comparison needs a measured left operand with an uncertainty; use == for exact values".into(),
        ));
    }
    Ok(d.value.mean().abs() <= k * u)
}

/// PHS `≈` with the catalog's `[Settings] approx_rel_tol`, and `approx(a, b, rel)` with its own: relative on the
/// canonical magnitudes (`Quantity::phs_approx`). Only numbers and quantities are approximately equal.
fn approximately(args: &[PhsValue], rel: f64, symbol: &str) -> PhysureResult<Option<PhsValue>> {
    let (l, r) = quantities(args, symbol)?;
    Ok(Some(boolean(l.phs_approx(&r, rel)?)))
}

/// `approx(a, b)` with the catalog's tolerance, or `approx(a, b, rel)` with `rel` a plain number (`0.5`, `50 %`).
fn approx_builtin(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    const USAGE: &str = "approx expects (a, b) or (a, b, rel): two numbers or quantities and a plain relative tolerance";
    let rel = match args.get(2) {
        None => physure_core::settings::approx_rel_tol(),
        Some(v) => match as_quantity(v) {
            Some(q) if q.unit.dimensions.is_empty() && !q.unit.is_logarithmic() && args.len() == 3 => q.canonical_magnitude(),
            _ => return Err(PhysureError::Generic(USAGE.into())),
        },
    };
    if args.len() < 2 {
        return Err(PhysureError::Generic(USAGE.into()));
    }
    approximately(&args[..2], rel, "approx")
}

/// The forms a format spec takes, for the error that refuses another one.
const FORMAT_SPECS: &str = "`base`, `frac`, `ifrac`, or `.N` with N digits and optionally `f`, `e` or `g` (`.2f`, `.3e`, `.4g`, `.2`)";

/// Whether `spec` is one of [`FORMAT_SPECS`]: an unknown spec used to be read as `.6f` without a word.
fn is_format_spec(spec: &str) -> bool {
    if matches!(spec, "base" | "frac" | "ifrac") {
        return true;
    }
    let Some(rest) = spec.strip_prefix('.') else { return false };
    let digits = rest.strip_suffix(['f', 'e', 'g']).unwrap_or(rest);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
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
    let render = |n: f64| render_number(n, spec);
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

/// One number as the format spec writes it: a fraction, or `.3f`, `.2e`, `.4g`.
fn render_number(n: f64, spec: &str) -> String {
    // `frac` writes the number as a fraction and `ifrac` as a mixed one — `1.5` is `3/2`
    // and `1 1/2`. Only when one applies: a number with no small fraction behind it keeps
    // its decimal rather than being rounded into a lie.
    let mixed_frac = match spec {
        "frac" => Some(false),
        "ifrac" => Some(true),
        _ => None,
    };
    if let Some(mixed) = mixed_frac {
        return physure_core::quantity::format_fraction(n, mixed)
            .unwrap_or_else(|| physure_core::quantity::format_float(n));
    }
    let digits = spec.trim_start_matches('.').trim_end_matches(char::is_alphabetic);
    let precision: usize = digits.parse().unwrap_or(6);
    let kind = spec.chars().last().filter(|c| c.is_alphabetic()).unwrap_or('f');
    match kind {
        'e' => format!("{:.*e}", precision, n),
        'g' => {
            let s = format!("{:.*}", precision, n);
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        _ => format!("{:.*}", precision, n),
    }
}

pub fn eval_core_builtin(
    name: &str,
    args: &[PhsValue],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
    const A_NUMBER_OR_QUANTITY: &str = "a number or quantity";
    match name {
        "parallel_map" => parallel_map(args, interpreter, env),
        "format" => format_builtin(args),
        "assert" => assert_builtin(args),
        // No Bool overload here, unlike `assert` above -- boolean equality is expressed as
        // `assert(left == right)`, not `exact_assert(left == right)`. This omission is
        // deliberate, not an oversight; do not add a symmetrical Bool arm.
        "exact_assert" => exact_assert_builtin(args),
        "op_>" | "op_gt" => order(args, ">"),
        "op_<" | "op_lt" => order(args, "<"),
        "op_>=" | "op_gte" => order(args, ">="),
        "op_<=" | "op_lte" => order(args, "<="),
        "op_==" | "op_eq" => Ok(Some(boolean(phs_equal(args, "==")?))),
        // exactly the negation of `==`, the sigma form included
        "op_!=" | "op_neq" => Ok(Some(boolean(!phs_equal(args, "!=")?))),
        "op_≈" | "op_approx" => approximately(args, physure_core::settings::approx_rel_tol(), "≈"),
        "approx" => approx_builtin(args),
        "ternary" | "if_then_else" => ternary(args),
        "uncertainty" | "sigma" => uncertainty_builtin(args),
        "dof" | "coverage_factor" | "expanded" | "from_samples" => crate::builtins::dof::eval_dof_builtin(name, args),
        "vector" => vector_builtin(args),
        "sqrt" => unary(name, args, A_NUMBER_OR_QUANTITY, sqrt_number, |q| q.sqrt()),
        "sin" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| Ok(n.sin()), |q| q.sin()),
        "cos" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| Ok(n.cos()), |q| q.cos()),
        "exp" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| Ok(n.exp()), |q| q.exp()),
        "ln" => unary(name, args, A_NUMBER_OR_QUANTITY, ln_number, |q| q.ln()),
        "abs" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| Ok(n.abs()), |q| q.abs()),
        "log" => unary(name, args, A_NUMBER_OR_QUANTITY, log10_number, |q| q.log10()),
        "tan" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| Ok(n.tan()), |q| q.tan()),
        "sec" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| reciprocal_number("sec", n, n.cos()), |q| q.sec()),
        "csc" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| reciprocal_number("csc", n, n.sin()), |q| q.csc()),
        "cot" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| reciprocal_number("cot", n, n.tan()), |q| q.cot()),
        "erf" => unary(name, args, A_NUMBER_OR_QUANTITY, |n| Ok(physure_core::math::erf(n)), |q| q.erf()),
        // Flooring the mean says nothing about how well it is known, so the unit
        // and the uncertainty ride along instead of being reset to zero.
        "floor" => unary(name, args, "number or quantity", |n| Ok(n.floor()), |q| q.floor()),
        "ceil" => unary(name, args, "number or quantity", |n| Ok(n.ceil()), |q| q.ceil()),
        "min" => extremum(name, args, false),
        "max" => extremum(name, args, true),
        "round" => round_builtin(args),
        _ => Ok(None),
    }
}

/// `vector(...)` and `[...]` (the parser builds the call): the numbers and quantities of a vector share one dimension,
/// in any of its units (`vector(1 m, 2 km)`). `norm`, `dot`, `trapz`, `det`, `matmul` refuse mixed elements one by
/// one; a vector of a length and a time has no meaning for any of them, and is refused where it is made. Text and
/// the rows of a matrix are not compared.
fn vector_builtin(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    PhsValue::vector_checked(args.to_vec()).map(Some)
}

fn sqrt_number(n: f64) -> PhysureResult<f64> {
    if n < 0.0 {
        Err(PhysureError::DomainError(format!("sqrt of a negative value ({n}) is undefined for real numbers")))
    } else {
        Ok(n.sqrt())
    }
}

fn ln_number(n: f64) -> PhysureResult<f64> {
    if n <= 0.0 {
        Err(PhysureError::DomainError(format!("ln of a non-positive value ({n}) is undefined for real numbers")))
    } else {
        Ok(n.ln())
    }
}

/// `func(n)` as `1 / denominator`, where `denominator` is `cos n`, `sin n` or `tan n`: a zero denominator is the
/// pole of `func`, refused as the quantity form refuses it rather than answered with infinity.
fn reciprocal_number(func: &str, n: f64, denominator: f64) -> PhysureResult<f64> {
    if denominator == 0.0 {
        Err(PhysureError::DomainError(format!("{func} of {n} is undefined")))
    } else {
        Ok(1.0 / denominator)
    }
}

fn log10_number(n: f64) -> PhysureResult<f64> {
    if n <= 0.0 {
        Err(PhysureError::DomainError(format!("log of a non-positive value ({n}) is undefined for real numbers")))
    } else {
        Ok(n.log10())
    }
}

fn format_builtin(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    let Some(val) = args.first() else { return Ok(None) };
    match args.get(1) {
        Some(PhsValue::String(spec)) if is_format_spec(spec) => Ok(Some(PhsValue::String(apply_format_spec(val, spec)))),
        Some(PhsValue::String(spec)) => {
            Err(PhysureError::Generic(format!("`{spec}` is not a format spec: write one of {FORMAT_SPECS}")))
        }
        Some(other) => Err(PhysureError::Generic(format!(
            "the spec of `format` is a text, got {other}: write one of {FORMAT_SPECS}"
        ))),
        // No spec, nothing to apply — hand the value back untouched.
        None => Ok(Some(val.clone())),
    }
}

fn exact_assert_builtin(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    match args {
        [PhsValue::Quantity(a), PhsValue::Quantity(b)] => {
            a.phs_exact_assert(b)?;
            Ok(Some(PhsValue::None))
        }
        _ => Err(PhysureError::Generic(format!(
            "exact_assert received ({}); expected exact_assert(Quantity, Quantity)",
            args.iter().map(PhsValue::type_name).collect::<Vec<_>>().join(", ")
        ))),
    }
}

/// `ternary(c, a, b)` reached with its arguments already evaluated (the interpreter takes the written forms,
/// `if c then a else b` and `c ? a : b`, itself): `c` must be a Bool, like every condition.
fn ternary(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    let [cond, then, otherwise] = args else {
        return Err(PhysureError::Generic(format!("ternary expects (condition, then, else), got {} arguments", args.len())));
    };
    let holds = crate::interpreter::helpers::condition_of(cond, "the condition of `? :`")?;
    Ok(Some(if holds { then.clone() } else { otherwise.clone() }))
}

/// The standard uncertainty as a quantity in the same unit, so it can be fed back
/// into arithmetic (`uncertainty(x) / x => %`). Anything without one reports zero.
fn uncertainty_builtin(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    match args.first() {
        Some(PhsValue::Quantity(q)) => Ok(Some(PhsValue::Quantity(
            physure_core::Quantity::new_scalar(q.value.std_dev(), 0.0, q.unit.clone(), None, None),
        ))),
        Some(PhsValue::Number(_)) => Ok(Some(PhsValue::Number(0.0))),
        _ => Err(PhysureError::Generic("uncertainty expects a quantity".into())),
    }
}

/// A function of exactly one number or quantity: `on_number` for a number, `on_quantity` for a quantity (the unit
/// and the uncertainty are the quantity method's to carry along).
fn unary(
    name: &str,
    args: &[PhsValue],
    expects: &str,
    on_number: impl FnOnce(f64) -> PhysureResult<f64>,
    on_quantity: impl FnOnce(&physure_core::Quantity) -> PhysureResult<physure_core::Quantity>,
) -> PhysureResult<Option<PhsValue>> {
    if args.len() != 1 {
        return Err(PhysureError::Generic(format!("{name} expects 1 argument")));
    }
    match &args[0] {
        PhsValue::Number(n) => Ok(Some(PhsValue::Number(on_number(*n)?))),
        PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(on_quantity(q)?))),
        _ => Err(PhysureError::Generic(format!("{name} expects {expects}"))),
    }
}

fn parallel_map(
    args: &[PhsValue],
    interpreter: &PhsInterpreter,
    env: &std::collections::HashMap<String, PhsValue>,
) -> PhysureResult<Option<PhsValue>> {
    let (func, v) = match (args.first(), args.get(1)) {
        (Some(PhsValue::Function(f)), Some(PhsValue::Vector(v))) => (f, v),
        _ => return Err(PhysureError::Generic("parallel_map expects (fn, vector)".into())),
    };
    // The input vector is already fully materialized by the time it reaches here --
    // built by a for-expression, `vector(...)`, or anything else -- so it carries the
    // same unbounded-allocation risk the for-expression's own range check guards
    // against (see `physure_core::settings::max_loop_iterations`'s doc comment).
    // Checked against the borrowed vector, before the `.clone()` below (which would
    // otherwise double an oversized input's memory) and before
    // `.into_par_iter().collect()` allocates a second `Vec<PhsValue>` the same size.
    let ceiling = physure_core::settings::max_loop_iterations();
    if v.len() > ceiling {
        return Err(PhysureError::Generic(format!(
            "parallel_map received a vector with {} elements, exceeding the max_loop_iterations ceiling of {ceiling}; raise `max_loop_iterations` in physure.conf's [Settings] section if this is a legitimate workload",
            v.len()
        )));
    }
    let vec = v.clone();
    let call = |(i, item): (usize, PhsValue)| {
        interpreter
            .call_function_node(func, vec![item], env)
            .map_err(|e| PhysureError::Generic(format!("parallel_map failed at index {i}: {e}")))
    };
    // A breakpoint inside a rayon worker closure isn't a coherent debugging
    // experience -- pausing one of N racing threads while the rest continue -- so a
    // debug session forces this back to plain sequential `.map()` instead of silently
    // ignoring any breakpoint set inside `func`. The same holds in the sandbox and in `monte_carlo` mode, where
    // the draws must be the next ones of the unit's stream (`PhsInterpreter::runs_in_parallel`).
    if !interpreter.runs_in_parallel() {
        let results = vec.into_iter().enumerate().map(call).collect::<PhysureResult<Vec<PhsValue>>>()?;
        return PhsValue::vector_made_by(results, "`parallel_map` gives one where ").map(Some);
    }
    use rayon::prelude::*;
    crate::interpreter::count_parallel_loop();
    // See `eval_for`: the scopes of the caller have to be opened again inside each task.
    let results = crate::interpreter::helpers::CallerScopes::capture(|scopes| {
        vec.into_par_iter().enumerate().map(|item| scopes.run(|| call(item))).collect::<PhysureResult<Vec<PhsValue>>>()
    })?;
    PhsValue::vector_made_by(results, "`parallel_map` gives one where ").map(Some)
}

fn assert_builtin(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    match args {
        [PhsValue::Bool(cond)] => bool_assertion(*cond, "condition was False"),
        [PhsValue::Bool(cond), PhsValue::String(msg)] => bool_assertion(*cond, msg),
        // one argument that is not a Bool is a condition that is not one, and is told what every condition is told
        [cond] => Err(PhysureError::Generic(crate::interpreter::helpers::not_a_bool("`assert`", cond))),
        [cond, PhsValue::String(_)] => {
            Err(PhysureError::Generic(crate::interpreter::helpers::not_a_bool("`assert`", cond)))
        }
        [PhsValue::Quantity(a), PhsValue::Quantity(b)] => {
            a.phs_assert(b)?;
            Ok(Some(PhsValue::None))
        }
        _ => Err(PhysureError::Generic(format!(
            "assert received ({}); expected assert(Bool), assert(Bool, String), or assert(Quantity, Quantity)",
            args.iter().map(PhsValue::type_name).collect::<Vec<_>>().join(", ")
        ))),
    }
}

fn bool_assertion(cond: bool, message: &str) -> PhysureResult<Option<PhsValue>> {
    if cond {
        Ok(Some(PhsValue::None))
    } else {
        Err(PhysureError::AssertionFailed { kind: "assert", message: message.to_string() })
    }
}

/// `min` and `max`: the argument with the smallest (or, with `want_greater`, largest) base-SI magnitude, as it
/// was written. Every argument has the dimension of the first (a plain number is dimensionless): `max(1 m, 1 s)`
/// has no answer, and comparing base-unit magnitudes gave `1 m` with a confident unit. One argument that is a
/// vector is the extremum of its elements, which a vector already holds of one dimension; one number or quantity is
/// itself.
fn extremum(name: &str, args: &[PhsValue], want_greater: bool) -> PhysureResult<Option<PhsValue>> {
    let args = match args {
        [PhsValue::Vector(items)] if items.is_empty() => {
            return Err(PhysureError::Generic(format!("{name} of an empty vector has no answer")))
        }
        [PhsValue::Vector(items)] => items.as_slice(),
        _ => args,
    };
    if args.is_empty() {
        return Err(PhysureError::Generic(format!("{name} expects arguments")));
    }
    let magnitude = |v: &PhsValue| match v {
        PhsValue::Number(n) => Ok(*n),
        PhsValue::Quantity(q) => Ok(q.canonical_magnitude()),
        _ => Err(PhysureError::Generic(format!("{name} expects numbers or quantities"))),
    };
    magnitude(&args[0])?;
    use crate::builtins::dimensions::{first_mismatch, Mismatch};
    match first_mismatch(args) {
        Some(Mismatch::Dimension(why)) => {
            return Err(PhysureError::Generic(format!("{name} of different dimensions: {why}; compare values of one dimension")))
        }
        Some(Mismatch::TemperatureKind(why)) => {
            return Err(PhysureError::Generic(format!(
                "{name} of a temperature point and a temperature interval: {why}; {}",
                physure_core::quantity::POINT_AGAINST_INTERVAL_ADVICE
            )))
        }
        None => {}
    }
    let mut best = &args[0];
    for arg in args.iter().skip(1) {
        let best_mag = magnitude(best)?;
        let arg_mag = magnitude(arg)?;
        if (want_greater && arg_mag > best_mag) || (!want_greater && arg_mag < best_mag) {
            best = arg;
        }
    }
    Ok(Some(best.clone()))
}

fn round_builtin(args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    if args.is_empty() {
        return Err(PhysureError::Generic("round expects arguments".into()));
    }
    // the number of decimals is a plain number: `round(1.5 m, 1 s)` read the `1` of `1 s` and ignored its unit
    // and a whole one: `round(x, 2.7)` and `round(x, 1 percent)` (0.01) were truncated; `-1` rounds to tens
    let decimals = match args.get(1) {
        None => 0,
        Some(value) => match crate::builtins::dimensions::whole_number(value) {
            Some(d) => d as i32,
            None if crate::builtins::dimensions::plain_number(value).is_some() => {
                return Err(PhysureError::Generic(format!("round needs a whole number of decimals, got {value}")));
            }
            None => return Err(PhysureError::Generic(format!("round expects a plain number of decimals, got {value}"))),
        },
    };
    // the core rounds without overflow (`round(1.5 m, 400)` was NaN m) and keeps everything but the mean: rounding
    // says nothing about how well the value is known, so the uncertainty, its sources and their degrees of freedom
    // ride along (rebuilding from the mean and the standard deviation lost the degrees of freedom)
    match &args[0] {
        PhsValue::Number(n) => Ok(Some(PhsValue::Number(physure_core::quantity::round_to_decimal_place(*n, decimals)))),
        PhsValue::Quantity(q) => Ok(Some(PhsValue::Quantity(q.round(decimals)?))),
        _ => Err(PhysureError::Generic("round expects number or quantity".into())),
    }
}
