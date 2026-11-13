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
use physure_core::{Quantity, RationalUnit};

use super::preprocess_symbolic_expression;
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

/// One end of the interval.
#[derive(Clone)]
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
            Some(unit) => format!("{} = {x} {}", self.var, unit.__repr__()),
            None => format!("{} = {x}", self.var),
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

/// A quadrature: the value, and the coefficient `c_i` of each evaluation in the order they were made, so that the
/// value is `sum c_i f(x_i)`.
struct Quadrature {
    value: f64,
    coefficients: Vec<f64>,
}

/// Composite three-point Gauss-Legendre quadrature over `panels` equal panels of `[a, b]` (exact for a polynomial of
/// degree 5 or less on each panel).
fn gauss_legendre(f: &mut dyn FnMut(f64) -> PhysureResult<f64>, a: f64, b: f64, panels: usize) -> PhysureResult<Quadrature> {
    let h = (b - a) / panels as f64;
    let half_h = 0.5 * h;
    // the nodes mid, mid +- half_h * sqrt(3/5) carry the weights 8/9, 5/9, 5/9 (of the panel's half length)
    let offset = half_h * 0.774_596_669_241_483_4;
    let (outer, centre) = (half_h * 5.0 / 9.0, half_h * 8.0 / 9.0);
    let mut sum = 0.0;
    let mut coefficients = Vec::with_capacity(3 * panels);
    for i in 0..panels {
        let mid = a + (i as f64 + 0.5) * h;
        for (x, c) in [(mid - offset, outer), (mid, centre), (mid + offset, outer)] {
            sum += c * f(x)?;
            coefficients.push(c);
        }
    }
    Ok(Quadrature { value: sum, coefficients })
}

/// The integral over the whole interval (finite, or with an infinite end), with `f(x)` sampled through `integrand`.
/// `jacobians` receives the factor `dx/dt` of each evaluation, so the coefficient of the sample `f(x_i)` is
/// `c_i * jacobian_i`.
fn numeric(integrand: &mut Integrand, a: f64, b: f64, jacobians: &mut Vec<f64>) -> PhysureResult<Quadrature> {
    if a.is_finite() && b.is_finite() {
        let mut f = |x: f64| {
            jacobians.push(1.0);
            integrand.sample(x)
        };
        return gauss_legendre(&mut f, a, b, 100);
    }
    // x = t / (1 - t^2) maps (-1, 1) onto the real line
    let to_unit_interval = |x: f64| {
        if x == f64::NEG_INFINITY {
            -1.0 + 1e-6
        } else if x == f64::INFINITY {
            1.0 - 1e-6
        } else {
            x / (1.0 + (1.0 + x * x).sqrt())
        }
    };
    let mut f = |t: f64| -> PhysureResult<f64> {
        let x = t / (1.0 - t * t);
        let dxdt = (1.0 + t * t) / ((1.0 - t * t) * (1.0 - t * t));
        jacobians.push(dxdt);
        Ok(integrand.sample(x)? * dxdt)
    };
    gauss_legendre(&mut f, to_unit_interval(a), to_unit_interval(b), 100)
}

/// The antiderivative of the integrand as an expression the interpreter can evaluate; `None` when the symbolic
/// engine has none (it leaves `integral(...)` in place, or refuses).
fn antiderivative(inlined: &str, var: &str) -> Option<Expr> {
    let node = crate::symbolic::SymbolicParser::parse_str(inlined).ok()?;
    let text = node.integrate_node(var).ok()?.simplify().to_string();
    if text.contains("integral(") {
        return None;
    }
    parse_expression(&text, "antiderivative").ok()
}

/// `F(b) - F(a)` with the core's arithmetic, `None` when `F` cannot be evaluated at an end or is not finite there.
fn closed_form(antiderivative: &Expr, integrand: &Integrand, lower: &Quantity, upper: &Quantity) -> Option<Quantity> {
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
    at(upper)?.sub(&at(lower)?).ok()
}

/// `sum c_i f(x_i)` with the core's arithmetic, so the samples' uncertainties and correlations propagate.
fn weighted_sum(samples: &[Quantity], coefficients: impl Iterator<Item = f64>, unit: &RationalUnit) -> PhysureResult<Quantity> {
    let mut total = Quantity::new_scalar(0.0, 0.0, unit.clone(), None, None);
    for (sample, c) in samples.iter().zip(coefficients) {
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

    if let (End::Finite(lo), End::Finite(hi)) = (&lower, &upper) {
        if let Some(f) = antiderivative(&inlined, var) {
            if let Some(value) = closed_form(&f, &integrand, lo, hi) {
                let units_involved = integrand.var_unit.is_some() || !value.unit.is_plain_number();
                return Ok(answer(value, units_involved));
            }
        }
    }

    let mean = |end: &End| match end {
        End::Finite(q) => q.value.mean(),
        End::Infinite(sign) => *sign,
    };
    let (a, b) = (mean(&lower), mean(&upper));
    let mut jacobians = Vec::new();
    let quadrature = numeric(&mut integrand, a, b, &mut jacobians)?;
    let unit = integrand.unit.clone().unwrap_or_else(RationalUnit::dimensionless);
    let mut total = if integrand.uncertain {
        let coefficients = quadrature.coefficients.iter().zip(&jacobians).map(|(c, j)| c * j);
        weighted_sum(&integrand.samples, coefficients, &unit)?
    } else {
        Quantity::new_scalar(quadrature.value, 0.0, unit.clone(), None, None)
    };
    if let Some(var_unit) = &integrand.var_unit {
        total = total.mul(&Quantity::new_scalar(1.0, 0.0, var_unit.clone(), None, None))?;
    }
    total = with_bound_uncertainty(total, &mut integrand, &lower, &upper)?;
    let units_involved = integrand.var_unit.is_some() || !unit.is_plain_number();
    Ok(answer(total, units_involved))
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
