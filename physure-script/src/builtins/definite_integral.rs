//! The definite integral of PHS: `integral("f(x)", "x", a, b)`.
//!
//! What it reads and what it answers:
//!
//! - **Bounds.** A number, a quantity, `"inf"`/`"-inf"` (and their spellings), or a name or an expression written as
//!   text (`pi`, `"pi/2"`, `"bohr_radius"`, a variable of the program). A name resolves as the integrand's names do
//!   (below); one that resolves to nothing is an error, never 0. A quantity bound makes the variable a quantity of
//!   that unit; the other bound must have the same dimension (a plain `0` is the same point in every unit). A
//!   temperature point or a level is refused as a bound.
//! - **Names.** The integrand is evaluated with the program's variables visible. A free name resolves, in order, to a
//!   variable of the program, a unit, `pi` (`π`), or a constant of the catalog in force (with its unit); any other
//!   free name is an error.
//! - **Unit.** The answer is a quantity in the integrand's unit times the variable's unit whenever either carries a
//!   unit (or the answer carries an uncertainty), and a plain number otherwise.
//! - **Uncertainty.** Propagated, never dropped. With a closed form the antiderivative is evaluated at the bounds by
//!   the core's arithmetic, so the program's uncertain variables and uncertain bounds propagate exactly. Numerically
//!   the answer is the weighted sum `sum c_i f(x_i)` of the samples the quadrature took, formed with the core's
//!   arithmetic (so the lineage of every input is kept and the result stays correlated with them), plus
//!   `f(b) (b - mean b) - f(a) (a - mean a)` for uncertain bounds (the first-order sensitivity of the integral to
//!   its bounds). The sum costs one core multiplication and addition per sample; it is formed only when a sample is
//!   uncertain.

use std::collections::HashMap;

use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::format_float;
use physure_core::{Quantity, RationalUnit};

use super::preprocess_symbolic_expression;
use super::quadrature::{tanh_sinh, Quadrature, Tail};
use crate::ast::Expr;
use crate::interpreter::PhsInterpreter;
use crate::value::PhsValue;

fn refusal(message: impl std::fmt::Display) -> PhysureError {
    PhysureError::Generic(format!("integral: {message}"))
}

/// A quantity with no unit and no uncertainty.
fn plain(x: f64) -> Quantity {
    Quantity::new_scalar(x, 0.0, RationalUnit::dimensionless(), None, None)
}

/// The value of a constant of the catalog in force, with its unit; `None` if the catalog has no constant `name`.
pub(crate) fn catalog_constant(name: &str) -> PhysureResult<Option<Quantity>> {
    let lookup = |registry: &physure_core::UnitRegistry| registry.constants_meta.get(name).map(|c| c.value.clone());
    let text = physure_core::units::with_current_catalog(|scoped| match scoped {
        Some(registry) => Ok(lookup(registry)),
        None => physure_core::units::conf::try_with_conf_registry(lookup).map_err(|e| PhysureError::Generic(e.to_string())),
    })?;
    let Some(text) = text else { return Ok(None) };
    let text = text.trim();
    let (number, unit) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let number: f64 = number
        .parse()
        .map_err(|_| PhysureError::Generic(format!("the catalog constant {name} is not a number: {text:?}")))?;
    Ok(Some(Quantity::new(number, unit.trim())?))
}

/// Binds in `env` the free names of `expr` that the interpreter would not resolve by itself: `pi` and the
/// constants of the catalog. A name that is a variable of `env`, one of `bound` (the variable of integration), or a
/// unit is left to the interpreter. Any other free name is an error naming it and `role` (what `expr` is).
pub(crate) fn bind_free_names(
    expr: &Expr,
    env: &mut HashMap<String, PhsValue>,
    bound: &mut Vec<String>,
    role: &str,
) -> PhysureResult<()> {
    match expr {
        Expr::Identifier(name) => bind_name(name, env, bound, role),
        Expr::BinaryOp { left, right, .. } => {
            bind_free_names(left, env, bound, role)?;
            bind_free_names(right, env, bound, role)
        }
        Expr::FunctionCall { name, args, kwargs } => {
            // `let(name, value, body)` binds `name` in `body`
            if let ("let", [Expr::Identifier(local), value, body]) = (name.as_str(), args.as_slice()) {
                bind_free_names(value, env, bound, role)?;
                bound.push(local.clone());
                let result = bind_free_names(body, env, bound, role);
                bound.pop();
                return result;
            }
            for arg in args.iter().chain(kwargs.iter().map(|(_, e)| e)) {
                bind_free_names(arg, env, bound, role)?;
            }
            Ok(())
        }
        Expr::ForExpr { var, iterable, body } => {
            bind_free_names(iterable, env, bound, role)?;
            bound.push(var.clone());
            let result = bind_free_names(body, env, bound, role);
            bound.pop();
            result
        }
        Expr::Quantity(_) | Expr::Bool(_) | Expr::Str(_) => Ok(()),
    }
}

fn bind_name(name: &str, env: &mut HashMap<String, PhsValue>, bound: &[String], role: &str) -> PhysureResult<()> {
    let template = name.starts_with('`') || (name.contains('{') && name.contains('}'));
    if template || bound.iter().any(|b| b == name) || env.contains_key(name) || crate::parser::is_known_unit_symbol(name) {
        return Ok(());
    }
    if name == "pi" || name == "π" {
        env.insert(name.to_string(), PhsValue::Number(std::f64::consts::PI));
        return Ok(());
    }
    if let Some(constant) = catalog_constant(name)? {
        env.insert(name.to_string(), PhsValue::Quantity(constant));
        return Ok(());
    }
    Err(refusal(format!(
        "the {role} uses '{name}', which is neither a variable of the program, a unit, pi nor a constant of the catalog"
    )))
}

/// The first expression of a text, as the parser reads it.
fn parse_expression(text: &str, role: &str) -> PhysureResult<Expr> {
    let program = crate::parser::parse_phs(text)?;
    match program.statements.into_iter().next() {
        Some(crate::ast::Statement::Expr(e)) => Ok(e),
        Some(crate::ast::Statement::Assignment(node)) => Ok(node.value),
        _ => Err(refusal(format!("the {role} {text:?} is not an expression"))),
    }
}

/// One end of the interval (two of them exist per call, so the size of the `Finite` variant does not matter).
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum End {
    Finite(Quantity),
    /// `+inf` or `-inf`.
    Infinite(f64),
}

fn is_infinity_text(s: &str) -> Option<f64> {
    match s.trim() {
        "inf" | "+inf" | "infinity" | "+infinity" | "oo" | "+oo" | "∞" | "+∞" => Some(f64::INFINITY),
        "-inf" | "-infinity" | "-oo" | "-∞" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

/// A bound as the caller gave it: a number, a quantity, an infinity, or a text naming one.
fn read_end(value: &PhsValue, interpreter: &PhsInterpreter, scope: &HashMap<String, PhsValue>, depth: usize) -> PhysureResult<End> {
    match value {
        PhsValue::Number(n) => end_of_quantity(plain(*n)),
        PhsValue::Quantity(q) => end_of_quantity(q.clone()),
        PhsValue::String(s) => {
            if let Some(infinity) = is_infinity_text(s) {
                return Ok(End::Infinite(infinity));
            }
            if depth > 0 {
                return Err(refusal(format!("the bound {s:?} names nothing that has a value")));
            }
            let expr = parse_expression(s, "bound")?;
            let mut env = scope.clone();
            bind_free_names(&expr, &mut env, &mut Vec::new(), "bound")?;
            let resolved = interpreter.eval_expr(&expr, &env)?;
            read_end(&resolved, interpreter, scope, depth + 1)
        }
        other => Err(refusal(format!("a bound must be a number, a quantity or \"inf\", not a {}", other.type_name()))),
    }
}

fn end_of_quantity(q: Quantity) -> PhysureResult<End> {
    if q.unit.is_affine() {
        return Err(refusal(format!(
            "the bound {q} is an absolute temperature (a point on a scale); integrate over an interval unit such as K"
        )));
    }
    if q.unit.is_logarithmic() {
        return Err(refusal(format!("the bound {q} is a level; convert it to the ratio it stands for first")));
    }
    let mean = q.value.mean();
    if mean.is_nan() {
        return Err(refusal("a bound is NaN"));
    }
    if mean.is_infinite() {
        if !q.unit.is_plain_number() {
            return Err(refusal(format!("write an infinite bound as \"inf\" or \"-inf\", not {q}")));
        }
        return Ok(End::Infinite(mean));
    }
    Ok(End::Finite(q))
}

/// The unit of the variable of integration (`None`: a plain number) and the two ends in it.
fn variable_unit(lower: End, upper: End) -> PhysureResult<(Option<RationalUnit>, End, End)> {
    let unit_of = |end: &End| match end {
        End::Finite(q) if !q.unit.is_plain_number() => Some(q.unit.clone()),
        _ => None,
    };
    let Some(unit) = unit_of(&lower).or_else(|| unit_of(&upper)) else {
        return Ok((None, lower, upper));
    };
    let express = |end: End| -> PhysureResult<End> {
        match end {
            End::Infinite(_) => Ok(end),
            End::Finite(q) if q.unit.is_plain_number() && q.value.mean() == 0.0 && q.value.std_dev() == 0.0 => {
                Ok(End::Finite(q.with_unit(unit.clone())))
            }
            End::Finite(q) if q.unit.same_dimensions(&unit) => Ok(End::Finite(q.convert_to(&unit)?)),
            End::Finite(q) => Err(refusal(format!(
                "the bounds have different dimensions ({q} and a bound in {}); only a plain 0 or an infinity may go \
                 with a bound that has a unit",
                unit.__repr__()
            ))),
        }
    };
    Ok((Some(unit.clone()), express(lower)?, express(upper)?))
}

/// The integrand, evaluated as the program would evaluate it at one value of the variable.
struct Integrand<'a> {
    interpreter: &'a PhsInterpreter,
    expr: Expr,
    env: HashMap<String, PhsValue>,
    var: String,
    var_unit: Option<RationalUnit>,
    /// The unit of the first value; every other value is converted to it.
    unit: Option<RationalUnit>,
    /// Every value taken, in order, when one of them is uncertain.
    samples: Vec<Quantity>,
    uncertain: bool,
}

impl Integrand<'_> {
    fn variable(&self, x: f64) -> PhsValue {
        match &self.var_unit {
            Some(unit) => PhsValue::Quantity(Quantity::new_scalar(x, 0.0, unit.clone(), None, None)),
            None => PhsValue::Number(x),
        }
    }

    fn describe(&self, x: f64) -> String {
        match &self.var_unit {
            Some(unit) => format!("{} = {} {}", self.var, format_float(x), unit.__repr__()),
            None => format!("{} = {}", self.var, format_float(x)),
        }
    }

    /// The integrand at `x`, in the unit of its first value.
    fn value_at(&mut self, x: f64) -> PhysureResult<Quantity> {
        let at = self.variable(x);
        self.env.insert(self.var.clone(), at);
        let value = self
            .interpreter
            .eval_expr(&self.expr, &self.env)
            .map_err(|e| refusal(format!("the integrand cannot be evaluated at {}: {e}", self.describe(x))))?;
        let q = match value {
            PhsValue::Number(n) => plain(n),
            PhsValue::Quantity(q) => q,
            other => {
                return Err(refusal(format!(
                    "the integrand must be a number or a quantity; at {} it is a {} ({other})",
                    self.describe(x),
                    other.type_name()
                )))
            }
        };
        if q.unit.is_affine() || q.unit.is_logarithmic() {
            return Err(refusal(format!("the integrand is {q} at {}: a temperature point or a level has no integral", self.describe(x))));
        }
        let q = match &self.unit {
            None => {
                self.unit = Some(q.unit.clone());
                q
            }
            Some(unit) if q.unit == *unit => q,
            Some(unit) if q.unit.same_dimensions(unit) => q.convert_to(unit)?,
            Some(unit) => {
                return Err(refusal(format!(
                    "the integrand changes dimension: {q} at {}, {} elsewhere",
                    self.describe(x),
                    unit.__repr__()
                )))
            }
        };
        if !q.value.mean().is_finite() {
            return Err(refusal(format!(
                "the integrand is not finite at {} ({}): the integral is improper there or diverges; split the \
                 interval at that point or remove the singularity",
                self.describe(x),
                q.value.mean()
            )));
        }
        Ok(q)
    }

    /// The magnitude of the integrand at `x`, kept as a sample of the quadrature.
    fn sample(&mut self, x: f64) -> PhysureResult<f64> {
        let q = self.value_at(x)?;
        let mean = q.value.mean();
        self.uncertain |= q.value.std_dev() > 0.0;
        self.samples.push(q);
        Ok(mean)
    }
}

/// The integral over `[a, b]` (`a < b`; either end may be infinite), with `f(x)` sampled through `integrand`.
/// `divisors` receives for each evaluation the `u` by whose square the sample was divided (1 on a finite interval),
/// so the coefficient of the sample `f(x_i)` itself is `c_i / u_i / u_i`.
fn numeric(integrand: &mut Integrand, a: f64, b: f64, divisors: &mut Vec<f64>) -> PhysureResult<Quadrature> {
    match (a.is_finite(), b.is_finite()) {
        (true, true) => {
            let mut f = |x: f64| {
                divisors.push(1.0);
                integrand.sample(x)
            };
            tanh_sinh(&mut f, a, b, (Tail::Underflow, Tail::Underflow))
        }
        (true, false) => half_line(integrand, a, 1.0, divisors),
        (false, true) => half_line(integrand, b, -1.0, divisors),
        (false, false) => Ok(half_line(integrand, 0.0, -1.0, divisors)?.plus(half_line(integrand, 0.0, 1.0, divisors)?)),
    }
}

/// The integral from `c` to infinity (`direction` +1) or from minus infinity to `c` (`direction` -1), by the one
/// substitution used for every infinite end:
///
/// ```text
/// x = c + direction * (1 - u) / u,   u in (0, 1],   dx/du = -direction / u^2
/// ```
///
/// `u = 1` is `x = c` and `u -> 0` is the infinite end. For direction +1,
/// `int_c^inf f(x) dx = int_1^0 f(x(u)) (-1/u^2) du = int_0^1 f(x(u)) / u^2 du`; for direction -1, `x` runs from -inf
/// (u -> 0) to `c` (u = 1) with `dx/du = 1/u^2`, so `int_-inf^c f(x) dx = int_0^1 f(x(u)) / u^2 du` as well. The
/// infinite end is `u = 0`, which the rule reaches as `0 + dist` with full precision. The sample is divided by `u`
/// twice, not by `u * u`, so that `u^2` underflowing cannot turn a vanishing `f` into `0 / 0`. The doubly infinite
/// interval is the two half lines from 0.
fn half_line(integrand: &mut Integrand, c: f64, direction: f64, divisors: &mut Vec<f64>) -> PhysureResult<Quadrature> {
    let mut f = |u: f64| -> PhysureResult<f64> {
        let x = c + direction * ((1.0 - u) / u);
        divisors.push(u);
        let g = integrand.sample(x)? / u / u;
        if !g.is_finite() {
            return Err(refusal(format!(
                "the integrand times the substitution's dx/du = 1/u^2 is not finite at {} (u = {}): it does not \
                 decay fast enough towards the infinite end",
                integrand.describe(x),
                format_float(u)
            )));
        }
        Ok(g)
    };
    // u = 0 stands for the infinite end: that tail may stop where the terms are negligible (see `Tail::Negligible`)
    tanh_sinh(&mut f, 0.0, 1.0, (Tail::Negligible, Tail::Underflow))
}

/// The antiderivative of the integrand as an expression the interpreter can evaluate, and its text; `None` when the
/// symbolic engine has none (it leaves `integral(...)` in place, or refuses).
fn antiderivative(inlined: &str, var: &str) -> Option<(Expr, String)> {
    let node = crate::symbolic::SymbolicParser::parse_str(inlined).ok()?;
    let text = node.integrate_node(var).ok()?.simplify().to_string();
    if text.contains("integral(") {
        return None;
    }
    Some((parse_expression(&text, "antiderivative").ok()?, text))
}

/// `F(a)` and `F(b)` with the core's arithmetic, `None` when `F` cannot be evaluated at an end or is not finite there.
fn closed_form(antiderivative: &Expr, integrand: &Integrand, lower: &Quantity, upper: &Quantity) -> Option<(Quantity, Quantity)> {
    let at = |end: &Quantity| -> Option<Quantity> {
        let mut env = integrand.env.clone();
        let value = match &integrand.var_unit {
            Some(_) => PhsValue::Quantity(end.clone()),
            None if end.value.std_dev() == 0.0 => PhsValue::Number(end.value.mean()),
            None => PhsValue::Quantity(end.clone()),
        };
        env.insert(integrand.var.clone(), value);
        let q = match integrand.interpreter.eval_expr(antiderivative, &env).ok()? {
            PhsValue::Number(n) => plain(n),
            PhsValue::Quantity(q) => q,
            _ => return None,
        };
        q.value.mean().is_finite().then_some(q)
    };
    Some((at(lower)?, at(upper)?))
}

/// `sum c_i f(x_i)` with the core's arithmetic, so the samples' uncertainties and correlations propagate.
fn weighted_sum(samples: &[Quantity], coefficients: impl Iterator<Item = f64>, unit: &RationalUnit) -> PhysureResult<Quantity> {
    let mut total = Quantity::new_scalar(0.0, 0.0, unit.clone(), None, None);
    for (sample, c) in samples.iter().zip(coefficients) {
        // an exact zero contributes nothing, also where `c / u / u` overflowed next to an infinite end
        if c == 0.0 || (sample.value.mean() == 0.0 && sample.value.std_dev() == 0.0) {
            continue;
        }
        if !c.is_finite() {
            return Err(refusal(format!("the weight of a sample is not finite ({c}) next to an infinite end")));
        }
        total = total.add(&sample.mul(&plain(c))?)?;
    }
    Ok(total)
}

/// What the caller receives: a plain number when no unit and no uncertainty is involved, otherwise the quantity.
fn answer(q: Quantity, units_involved: bool) -> PhsValue {
    if !units_involved && q.unit.is_plain_number() && q.value.std_dev() == 0.0 {
        PhsValue::Number(q.value.mean())
    } else {
        PhsValue::Quantity(q)
    }
}

/// How an end is written in a message.
fn end_text(end: f64, unit: &Option<RationalUnit>) -> String {
    match unit {
        Some(unit) if end.is_finite() => format!("{} {}", format_float(end), unit.__repr__()),
        _ => format_float(end),
    }
}

/// `integral(expr, var, lower, upper)`, called where the program's variables are `scope`.
pub(crate) fn definite_integral(
    expr_str: &str,
    var: &str,
    lower: &PhsValue,
    upper: &PhsValue,
    interpreter: &PhsInterpreter,
    scope: &HashMap<String, PhsValue>,
) -> PhysureResult<PhsValue> {
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    let expr = parse_expression(&inlined, "integrand")?;
    let mut env = scope.clone();
    bind_free_names(&expr, &mut env, &mut vec![var.to_string()], "integrand")?;

    let (lower, upper) = (read_end(lower, interpreter, scope, 0)?, read_end(upper, interpreter, scope, 0)?);
    let (var_unit, lower, upper) = variable_unit(lower, upper)?;
    let mut integrand =
        Integrand { interpreter, expr, env, var: var.to_string(), var_unit, unit: None, samples: Vec::new(), uncertain: false };

    let mean = |end: &End| match end {
        End::Finite(q) => q.value.mean(),
        End::Infinite(sign) => *sign,
    };
    let (a, b) = (mean(&lower), mean(&upper));
    if a == b {
        // an empty interval: zero, in the unit the integral has
        let unit = integrand.value_at(a)?.unit;
        let mut zero = Quantity::new_scalar(0.0, 0.0, unit, None, None);
        if let Some(var_unit) = &integrand.var_unit {
            zero = zero.mul(&Quantity::new_scalar(1.0, 0.0, var_unit.clone(), None, None))?;
        }
        return finish(zero, &integrand);
    }
    // the rule integrates from the smaller end to the larger one
    let (from, to, orientation) = if a < b { (a, b, 1.0) } else { (b, a, -1.0) };
    let mut divisors = Vec::new();
    let quadrature = numeric(&mut integrand, from, to, &mut divisors);
    let interval = format!("[{}, {}]", end_text(a, &integrand.var_unit), end_text(b, &integrand.var_unit));

    if let (End::Finite(lo), End::Finite(hi)) = (&lower, &upper) {
        if let Some((f, f_text)) = antiderivative(&inlined, var) {
            if let Some((f_lo, f_hi)) = closed_form(&f, &integrand, lo, hi) {
                let quadrature = quadrature.map_err(|e| {
                    refusal(format!("the antiderivative {f_text} cannot be confirmed on {interval}: {}", strip(&e)))
                })?;
                return confirmed_closed_form(f_lo, f_hi, &f_text, orientation, &quadrature, &integrand, &interval);
            }
        }
    }

    let quadrature = quadrature?;
    if !quadrature.converged {
        return Err(not_converged(&quadrature, &interval));
    }
    let unit = integrand.unit.clone().unwrap_or_else(RationalUnit::dimensionless);
    let mut total = if integrand.uncertain {
        let coefficients = quadrature.coefficients.iter().zip(&divisors).map(|(c, u)| orientation * c / u / u);
        weighted_sum(&integrand.samples, coefficients, &unit)?
    } else {
        Quantity::new_scalar(orientation * quadrature.value, 0.0, unit, None, None)
    };
    if let Some(var_unit) = &integrand.var_unit {
        total = total.mul(&Quantity::new_scalar(1.0, 0.0, var_unit.clone(), None, None))?;
    }
    total = with_bound_uncertainty(total, &mut integrand, &lower, &upper)?;
    finish(total, &integrand)
}

/// The text of an error of this module without its `integral: ` prefix, to nest it in another one.
fn strip(error: &PhysureError) -> String {
    let text = error.to_string();
    text.strip_prefix("integral: ").map(str::to_string).unwrap_or(text)
}

fn not_converged(quadrature: &Quadrature, interval: &str) -> PhysureError {
    refusal(format!(
        "the tanh-sinh quadrature did not converge on {interval}: estimate {}, error estimate {:e} after {} \
         evaluations (level {}); the integrand may have a singularity inside the interval or at a non-zero end, a jump, a kink or a fast \
         oscillation there (split the interval at that point), or decay too slowly towards an infinite end; give a \
         closed form if there is one",
        quadrature.value,
        quadrature.error,
        quadrature.evaluations(),
        quadrature.level
    ))
}

/// The answer as a plain number or a quantity (see [`answer`]).
fn finish(total: Quantity, integrand: &Integrand) -> PhysureResult<PhsValue> {
    let unit_carried = integrand.unit.as_ref().is_some_and(|u| !u.is_plain_number());
    Ok(answer(total, integrand.var_unit.is_some() || unit_carried))
}

/// `F(b) - F(a)`, accepted only when the quadrature converged and agrees with it: an antiderivative that is not
/// continuous on the interval (a pole of the integrand inside it, a branch of `ln` or `atan` jumping) gives a
/// confident wrong difference, which the quadrature (it only samples the integrand) does not share. The two agree
/// when they differ by at most `1e-8` of the largest of `|F(b) - F(a)|`, the estimate and its `L1`, plus the
/// rounding of the difference itself (`64 eps (|F(a)| + |F(b)|)`, so a large antiderivative's cancellation is not
/// mistaken for a discontinuity).
fn confirmed_closed_form(
    f_lo: Quantity,
    f_hi: Quantity,
    f_text: &str,
    orientation: f64,
    quadrature: &Quadrature,
    integrand: &Integrand,
    interval: &str,
) -> PhysureResult<PhsValue> {
    if !quadrature.converged {
        return Err(refusal(format!(
            "the antiderivative {f_text} cannot be confirmed: {}",
            strip(&not_converged(quadrature, interval))
        )));
    }
    let closed = f_hi.sub(&f_lo)?;
    let unit = integrand.unit.clone().unwrap_or_else(RationalUnit::dimensionless);
    let result_unit = match &integrand.var_unit {
        Some(var_unit) => plain(1.0).with_unit(unit).mul(&Quantity::new_scalar(1.0, 0.0, var_unit.clone(), None, None))?.unit,
        None => unit,
    };
    let in_result_unit = |q: &Quantity| -> PhysureResult<f64> {
        if !q.unit.same_dimensions(&result_unit) {
            return Err(refusal(format!(
                "the antiderivative {f_text} has the unit {}, the integral {}",
                q.unit.__repr__(),
                result_unit.__repr__()
            )));
        }
        Ok(q.convert_to(&result_unit)?.value.mean())
    };
    let (s, s_lo, s_hi) = (in_result_unit(&closed)?, in_result_unit(&f_lo)?, in_result_unit(&f_hi)?);
    let n = orientation * quadrature.value;
    let tolerance = 1e-8 * s.abs().max(n.abs()).max(quadrature.l1) + 64.0 * f64::EPSILON * (s_lo.abs() + s_hi.abs());
    if (s - n).abs() > tolerance {
        return Err(refusal(format!(
            "the antiderivative {f_text} gives {s} on {interval} but the quadrature gives {n} (error estimate {:e}): \
             the integrand or its antiderivative is not continuous on the interval (a pole or a jump inside); split \
             the interval there",
            quadrature.error
        )));
    }
    finish(closed, integrand)
}

/// Adds `f(b) (b - mean b) - f(a) (a - mean a)` for the uncertain bounds: the first-order change of the integral
/// with its ends, carrying their lineage.
fn with_bound_uncertainty(total: Quantity, integrand: &mut Integrand, lower: &End, upper: &End) -> PhysureResult<Quantity> {
    let mut total = total;
    for (end, sign) in [(upper, 1.0), (lower, -1.0)] {
        let End::Finite(q) = end else { continue };
        if q.value.std_dev() == 0.0 {
            continue;
        }
        let deviation = q.sub(&Quantity::new_scalar(q.value.mean(), 0.0, q.unit.clone(), None, None))?;
        let f_end = integrand.value_at(q.value.mean())?;
        total = total.add(&f_end.mul(&deviation)?.mul(&plain(sign))?)?;
    }
    Ok(total)
}
