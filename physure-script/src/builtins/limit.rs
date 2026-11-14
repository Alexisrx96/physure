//! The limit of PHS: `limit("f(x)", "x", p)`.
//!
//! The symbolic engine has no limit, so the answer is numeric, and it is given only with a converged error estimate;
//! otherwise the call is refused (it never answers `0` or `inf` as a guess).
//!
//! - **Point.** A number, a quantity, `"inf"`/`"-inf"` (and their spellings), or a name or an expression written as
//!   text (`pi`, `"pi/2"`, a variable, a constant of the catalog). A name that resolves to nothing is an error. A
//!   quantity point makes the variable a quantity of that unit; a temperature point or a level is refused.
//! - **Names.** The expression is evaluated with the program's variables visible; a free name resolves to a variable,
//!   a unit, `pi` or a constant of the catalog, and any other name is an error (as for `integral`).
//! - **Continuity.** When the expression calls only functions that are continuous on their domain (`sin`, `cos`,
//!   `tan`, `exp`, `ln`, `log`, `sqrt`, `min`, `max`, arithmetic, and `^` with an exponent that does not involve the
//!   variable) and its value at `p` is defined and finite, the limit is that value, evaluated by the core (so an
//!   uncertain point or uncertain variables propagate exactly). `floor`, `ceil`, `round`, `abs`, comparisons,
//!   ternaries, `if`, user functions that were not inlined and other builtins are never trusted at the point itself.
//! - **Extrapolation.** Otherwise `f` is sampled on each side at `p ± h0 2^-k` (`h0 = 1/8`, or more where the spacing
//!   of f64 next to `p` requires it), `k = 0 .. 11`, and the samples are extrapolated to `h = 0` by Neville's
//!   algorithm (a polynomial in `h`, Richardson extrapolation): the entry with the smallest difference to its
//!   neighbours in the table is the estimate and that difference its error. A side is accepted when the error is
//!   below `1e-9 max(|L|, max |f|)` at two successive levels (from the fourth on), the samples approach `L` and do
//!   not diverge. Scale-relative, so a limit of 0 is accepted. The two sides must agree within the same tolerance,
//!   and the answer is their mean. A side where the expression cannot be evaluated at any sample (outside the
//!   domain of `sqrt` or `ln`) does not exist, and the limit is the one within the domain (`sqrt(x)` next to 0 is
//!   read from the right).
//! - **Zero and infinity.** When the extrapolation does not settle, samples of one sign that fall geometrically
//!   (each at most 3/4 of the previous, over the last five) give the limit 0 (`x ln x` at 0, `1/sqrt(x)` at
//!   infinity); samples of one sign whose magnitude grows with non-decreasing increments over the last five (or
//!   overflows) give a signed infinity, accepted only when both sides agree (`1/x^2` at 0 is `inf`, `1/x` at 0 is
//!   refused).
//! - **Infinite point.** `x = ±8 · 2^k`, `k = 0 .. 23`, extrapolated in `h = 1/|x|` the same way. The variable is a
//!   plain number there.
//! - **Rounding.** Two successive samples that are exactly equal after earlier ones differed mean rounding has
//!   swallowed the change of the variable: sampling stops there and nothing past it is trusted.
//! - **Uncertainty.** An extrapolated limit is a weighted sum of samples, formed with the core's arithmetic when a
//!   sample is uncertain (so the lineage of every input is kept). An uncertain point where the expression is not
//!   continuous is refused: the limit has no derivative with respect to it that the samples could give.
//!
//! Measured: removable singularities whose samples cancel (`(1 - cos x)/x^2`, `(exp x - 1 - x)/x^2`) reach about
//! 5e-11 relative; others (`sin(x)/x`, `(sqrt(1 + x) - 1)/x`, rational functions, `(1 + 1/x)^x` at infinity) 3e-13
//! or better. Refused, not guessed: sides that disagree (`abs(x)/x`, `floor(x)` at 1, `1/x` at 0), oscillation
//! (`sin(1/x)` at 0, `sin(x)` at infinity), a feature narrower than the smallest step (`(1 - cos(1000 x))/x^2`),
//! and a function that is only constant from some point on (`min(x, 100)` at infinity).

use std::collections::HashMap;

use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::format_float;
use physure_core::{Quantity, RationalUnit};

use super::definite_integral::{bind_free_names, is_infinity_text};
use super::preprocess_symbolic_expression;
use crate::ast::{BinaryOp, Expr};
use crate::interpreter::PhsInterpreter;
use crate::value::PhsValue;

/// Relative tolerance of an accepted side and of the agreement of the two sides.
const TOLERANCE: f64 = 1e-9;
/// Steps tried on each side of a finite point.
const FINITE_LEVELS: usize = 12;
/// Steps tried towards an infinite point.
const INFINITE_LEVELS: usize = 24;
/// The first sample index at which a side may be accepted.
const MIN_LEVEL: usize = 3;
/// How many trailing samples the decay and divergence tests read.
const RUN: usize = 5;
/// The largest ratio of successive samples that still counts as a geometric fall to 0.
const DECAY_RATIO: f64 = 0.75;

fn refusal(message: impl std::fmt::Display) -> PhysureError {
    PhysureError::Generic(format!("limit: {message}"))
}

/// A quantity with no unit and no uncertainty.
fn plain(x: f64) -> Quantity {
    Quantity::new_scalar(x, 0.0, RationalUnit::dimensionless(), None, None)
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

/// Where the limit is taken.
#[allow(clippy::large_enum_variant)]
enum Point {
    Finite(Quantity),
    /// `+inf` or `-inf`.
    Infinite(f64),
}

fn read_point(value: &PhsValue, interpreter: &PhsInterpreter, scope: &HashMap<String, PhsValue>, depth: usize) -> PhysureResult<Point> {
    match value {
        PhsValue::Number(n) => point_of_quantity(plain(*n)),
        PhsValue::Quantity(q) => point_of_quantity(q.clone()),
        PhsValue::String(s) => {
            if let Some(infinity) = is_infinity_text(s) {
                return Ok(Point::Infinite(infinity));
            }
            if depth > 0 {
                return Err(refusal(format!("the point {s:?} names nothing that has a value")));
            }
            let expr = parse_expression(s, "point")?;
            let mut env = scope.clone();
            bind_free_names(&expr, &mut env, &mut Vec::new(), "point", "limit")?;
            let resolved = interpreter.eval_expr(&expr, &env)?;
            read_point(&resolved, interpreter, scope, depth + 1)
        }
        other => Err(refusal(format!("the point must be a number, a quantity or \"inf\", not a {}", other.type_name()))),
    }
}

fn point_of_quantity(q: Quantity) -> PhysureResult<Point> {
    if q.unit.is_affine() {
        return Err(refusal(format!(
            "the point {q} is an absolute temperature (a point on a scale); give it in an interval unit such as K"
        )));
    }
    if q.unit.is_logarithmic() {
        return Err(refusal(format!("the point {q} is a level; convert it to the ratio it stands for first")));
    }
    let mean = q.value.mean();
    if mean.is_nan() {
        return Err(refusal("the point is NaN"));
    }
    if mean.is_infinite() {
        if !q.unit.is_plain_number() {
            return Err(refusal(format!("write an infinite point as \"inf\" or \"-inf\", not {q}")));
        }
        return Ok(Point::Infinite(mean));
    }
    Ok(Point::Finite(q))
}

/// Whether the value of `expr` at a point where it is defined and finite is its limit there: every function it calls
/// is continuous on its domain, and no exponent involves the variable (`0^x` is 1 at 0, its limit from the right 0).
fn continuous(expr: &Expr, var: &str) -> bool {
    match expr {
        Expr::Identifier(_) | Expr::Quantity(_) | Expr::Bool(_) | Expr::Str(_) => true,
        Expr::BinaryOp { op: BinaryOp::Range, .. } => false,
        Expr::BinaryOp { op: BinaryOp::Pow, left, right } => continuous(left, var) && continuous(right, var) && !mentions(right, var),
        Expr::BinaryOp { left, right, .. } => continuous(left, var) && continuous(right, var),
        Expr::FunctionCall { name, args, kwargs } => {
            matches!(name.as_str(), "sin" | "cos" | "tan" | "exp" | "ln" | "log" | "sqrt" | "min" | "max" | "let")
                && kwargs.is_empty()
                && args.iter().all(|a| continuous(a, var))
        }
        Expr::ForExpr { .. } => false,
    }
}

fn mentions(expr: &Expr, var: &str) -> bool {
    match expr {
        Expr::Identifier(name) => name == var,
        Expr::Quantity(_) | Expr::Bool(_) | Expr::Str(_) => false,
        Expr::BinaryOp { left, right, .. } => mentions(left, var) || mentions(right, var),
        Expr::FunctionCall { args, kwargs, .. } => args.iter().chain(kwargs.iter().map(|(_, e)| e)).any(|a| mentions(a, var)),
        Expr::ForExpr { iterable, body, .. } => mentions(iterable, var) || mentions(body, var),
    }
}

/// The expression, evaluated as the program would evaluate it at one value of the variable.
struct Function<'a> {
    interpreter: &'a PhsInterpreter,
    expr: Expr,
    env: HashMap<String, PhsValue>,
    var: String,
    var_unit: Option<RationalUnit>,
    /// The unit of the first sample; every other sample is converted to it.
    unit: Option<RationalUnit>,
}

impl Function<'_> {
    fn describe(&self, x: f64) -> String {
        match &self.var_unit {
            Some(unit) => format!("{} = {} {}", self.var, format_float(x), unit.__repr__()),
            None => format!("{} = {}", self.var, format_float(x)),
        }
    }

    /// The value at `at` as the interpreter gives it; `Ok(None)` where the expression is not defined (a domain
    /// error, a division by zero, NaN), an error for any other failure.
    fn raw(&mut self, at: PhsValue, x: f64) -> PhysureResult<Option<PhsValue>> {
        self.env.insert(self.var.clone(), at);
        match self.interpreter.eval_expr(&self.expr, &self.env) {
            Ok(PhsValue::Number(n)) if n.is_nan() => Ok(None),
            Ok(PhsValue::Quantity(q)) if q.value.mean().is_nan() => Ok(None),
            Ok(value) => Ok(Some(value)),
            Err(PhysureError::DomainError(_) | PhysureError::DivisionByZero(_)) => Ok(None),
            // the interpreter's division of two plain numbers
            Err(PhysureError::Generic(message)) if message == "Division by zero" => Ok(None),
            Err(e) => Err(refusal(format!("the expression cannot be evaluated at {}: {e}", self.describe(x)))),
        }
    }

    /// A sample at `x`, in the unit of the first sample; `Ok(None)` where the expression is not defined.
    fn sample(&mut self, x: f64) -> PhysureResult<Option<Quantity>> {
        let at = match &self.var_unit {
            Some(unit) => PhsValue::Quantity(Quantity::new_scalar(x, 0.0, unit.clone(), None, None)),
            None => PhsValue::Number(x),
        };
        let Some(value) = self.raw(at, x)? else { return Ok(None) };
        let q = match value {
            PhsValue::Number(n) => plain(n),
            PhsValue::Quantity(q) => q,
            other => {
                return Err(refusal(format!(
                    "the expression must be a number or a quantity; at {} it is a {} ({other})",
                    self.describe(x),
                    other.type_name()
                )))
            }
        };
        if q.unit.is_affine() || q.unit.is_logarithmic() {
            return Err(refusal(format!(
                "the expression is {q} at {}: the limit of a temperature point or a level is not extrapolated; take it \
                 of the interval or the ratio",
                self.describe(x)
            )));
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
                    "the expression changes dimension: {q} at {}, {} elsewhere",
                    self.describe(x),
                    unit.__repr__()
                )))
            }
        };
        Ok(Some(q))
    }
}

/// What one side (or the approach to an infinite point) gave.
enum Side {
    /// The expression is defined at none of the samples.
    Absent,
    /// The extrapolated limit: its value and error, the size of the samples, and the samples with their weights.
    Converged { value: f64, error: f64, scale: f64, weighted: Vec<(Quantity, f64)> },
    /// A signed infinity.
    Infinite(f64),
    /// Neither: the best estimate, its error, and why sampling stopped.
    Unsettled { estimate: f64, error: f64, why: &'static str },
}

/// The samples of one side, at the points `point(k) = (x, h)` (`h` the distance to the limit point, decreasing).
fn side(function: &mut Function, levels: usize, point: impl Fn(usize) -> (f64, f64)) -> PhysureResult<Side> {
    let mut hs: Vec<f64> = Vec::new();
    let mut samples: Vec<Quantity> = Vec::new();
    // every finite or overflowing magnitude, in order, for the decay and divergence tests
    let mut magnitudes: Vec<f64> = Vec::new();
    let mut table: Vec<Vec<f64>> = Vec::new();
    let (mut best, mut best_error, mut best_at) = (f64::NAN, f64::INFINITY, (0, 0));
    let mut agreements = 0;
    let mut undefined = 0;
    let mut why = "the last step was reached";
    for k in 0..levels {
        let (x, h) = point(k);
        if h == 0.0 || hs.last().is_some_and(|&last| h >= last) {
            why = "the spacing of f64 next to the point was reached";
            break;
        }
        let Some(q) = function.sample(x)? else {
            if !magnitudes.is_empty() {
                return Err(refusal(format!(
                    "the expression is undefined at {} although it is defined farther from the point",
                    function.describe(x)
                )));
            }
            undefined += 1;
            continue;
        };
        if undefined > 0 {
            return Err(refusal(format!(
                "the expression is undefined farther from the point than {}, where it is defined",
                function.describe(x)
            )));
        }
        let v = q.value.mean();
        if v.is_infinite() {
            magnitudes.push(v);
            why = "a sample overflowed";
            break;
        }
        if magnitudes.last() == Some(&v) && magnitudes.iter().any(|&m| m != v) {
            magnitudes.push(v);
            why = "two samples were equal: rounding has swallowed the change of the variable";
            break;
        }
        magnitudes.push(v);
        hs.push(h);
        samples.push(q);
        let n = hs.len() - 1;
        let mut row = vec![v];
        for j in 1..=n {
            let entry = (row[j - 1] * -hs[n - j] + table[n - 1][j - 1] * hs[n]) / (hs[n] - hs[n - j]);
            let error = (entry - row[j - 1]).abs().max((entry - table[n - 1][j - 1]).abs());
            row.push(entry);
            if error <= best_error {
                (best, best_error, best_at) = (entry, error, (n, j));
            }
        }
        table.push(row);
        let scale = magnitudes.iter().fold(0.0f64, |a, m| a.max(m.abs()));
        let approaching = (v - best).abs() <= (magnitudes[0] - best).abs();
        if n >= MIN_LEVEL && best_error <= TOLERANCE * best.abs().max(scale) && approaching && diverges(&magnitudes) == 0.0 {
            agreements += 1;
            if agreements == 2 {
                let (n, j) = best_at;
                let weighted = (n - j..=n).map(|i| (samples[i].clone(), lagrange_at_zero(&hs[n - j..=n], i - (n - j)))).collect();
                return Ok(Side::Converged { value: best, error: best_error, scale, weighted });
            }
        } else {
            agreements = 0;
        }
    }
    if magnitudes.is_empty() {
        return Ok(Side::Absent);
    }
    if decays(&magnitudes) {
        let scale = magnitudes.iter().fold(0.0f64, |a, m| a.max(m.abs()));
        return Ok(Side::Converged { value: 0.0, error: 0.0, scale, weighted: Vec::new() });
    }
    let infinity = diverges(&magnitudes);
    if infinity != 0.0 {
        return Ok(Side::Infinite(infinity));
    }
    Ok(Side::Unsettled { estimate: best, error: best_error, why })
}

/// The weight of the sample at `nodes[i]` in the value at 0 of the polynomial through all `nodes`.
fn lagrange_at_zero(nodes: &[f64], i: usize) -> f64 {
    nodes.iter().enumerate().filter(|&(m, _)| m != i).map(|(_, &h)| h / (h - nodes[i])).product()
}

/// Samples of one sign that fall geometrically to 0 over the last [`RUN`], or reach exactly 0 after at least three
/// such samples.
fn decays(magnitudes: &[f64]) -> bool {
    let zeros = magnitudes.iter().rev().take_while(|&&m| m == 0.0).count();
    let nonzero = &magnitudes[..magnitudes.len() - zeros];
    let needed = if zeros > 0 { 3 } else { RUN };
    if nonzero.len() < needed || nonzero.iter().any(|m| !m.is_finite()) {
        return false;
    }
    let tail = &nonzero[nonzero.len() - needed..];
    let sign = tail[0].signum();
    tail.iter().all(|m| m.signum() == sign && *m != 0.0) && tail.windows(2).all(|w| w[1].abs() <= DECAY_RATIO * w[0].abs())
}

/// `+inf` or `-inf` when the last [`RUN`] samples have one sign and a magnitude that grows with non-decreasing
/// increments (or overflows), `0` otherwise.
fn diverges(magnitudes: &[f64]) -> f64 {
    if magnitudes.len() < RUN {
        return 0.0;
    }
    let tail = &magnitudes[magnitudes.len() - RUN..];
    let sign = tail[0].signum();
    if tail.iter().any(|m| *m == 0.0 || m.signum() != sign) {
        return 0.0;
    }
    let increments: Vec<f64> = tail.windows(2).map(|w| w[1].abs() - w[0].abs()).collect();
    let growing = increments.iter().all(|&d| d > 0.0) && increments.windows(2).all(|w| w[1] >= (1.0 - 1e-6) * w[0]);
    if growing {
        sign * f64::INFINITY
    } else {
        0.0
    }
}

/// The first step next to `p`: 1/8, or a power of two large enough that the smallest step is 16 units in the last
/// place of `p`, so that every sample is a distinct point.
fn first_step(p: f64) -> f64 {
    let ulp = if p == 0.0 { 0.0 } else { f64::from_bits(p.abs().to_bits() + 1) - p.abs() };
    let needed = ulp * 16.0 * 2f64.powi(FINITE_LEVELS as i32 - 1);
    let mut h = 0.125;
    while h < needed {
        h *= 2.0;
    }
    h
}

/// `limit(expr, var, point)`, called where the program's variables are `scope`.
pub(crate) fn limit(
    expr_str: &str,
    var: &str,
    point: &PhsValue,
    interpreter: &PhsInterpreter,
    scope: &HashMap<String, PhsValue>,
) -> PhysureResult<PhsValue> {
    let inlined = preprocess_symbolic_expression(expr_str, interpreter);
    let expr = parse_expression(&inlined, "expression")?;
    let mut env = scope.clone();
    bind_free_names(&expr, &mut env, &mut vec![var.to_string()], "expression", "limit")?;
    let point = read_point(point, interpreter, scope, 0)?;
    let var_unit = match &point {
        Point::Finite(q) if !q.unit.is_plain_number() => Some(q.unit.clone()),
        _ => None,
    };
    let trusted_at_point = continuous(&expr, var);
    let mut function = Function { interpreter, expr, env, var: var.to_string(), var_unit, unit: None };

    match point {
        Point::Infinite(infinity) => {
            let sign = infinity.signum();
            let approach = side(&mut function, INFINITE_LEVELS, |k| {
                let distance = 8.0 * 2f64.powi(k as i32);
                (sign * distance, 1.0 / distance)
            })?;
            let at = if sign > 0.0 { "+inf" } else { "-inf" };
            match approach {
                Side::Absent => Err(refusal(format!("the expression is undefined towards {at}"))),
                Side::Converged { value, weighted, .. } => answer(value, &weighted, &function),
                Side::Infinite(infinity) => answer(infinity, &[], &function),
                Side::Unsettled { estimate, error, why } => Err(unsettled(&format!("towards {at}"), estimate, error, why)),
            }
        }
        Point::Finite(p) => {
            if trusted_at_point {
                let at = match &function.var_unit {
                    Some(_) => PhsValue::Quantity(p.clone()),
                    None if p.value.std_dev() == 0.0 => PhsValue::Number(p.value.mean()),
                    None => PhsValue::Quantity(p.clone()),
                };
                if let Some(value) = function.raw(at, p.value.mean())? {
                    let finite = match &value {
                        PhsValue::Number(n) => n.is_finite(),
                        PhsValue::Quantity(q) => q.value.mean().is_finite(),
                        _ => false,
                    };
                    if finite {
                        return Ok(plain_when_possible(value));
                    }
                }
            }
            if p.value.std_dev() > 0.0 {
                return Err(refusal(format!(
                    "the point {p} is uncertain, and the limit there is extrapolated (the expression is not evaluated at \
                     the point itself): an extrapolated limit has no derivative with respect to its point to propagate the uncertainty with"
                )));
            }
            two_sided(&mut function, p.value.mean())
        }
    }
}

/// Both sides of a finite point, which must agree.
fn two_sided(function: &mut Function, p: f64) -> PhysureResult<PhsValue> {
    let h0 = first_step(p);
    let step = |direction: f64| {
        move |k: usize| {
            let x = p + direction * h0 / 2f64.powi(k as i32);
            (x, (x - p).abs())
        }
    };
    let right = side(function, FINITE_LEVELS, step(1.0))?;
    let left = side(function, FINITE_LEVELS, step(-1.0))?;
    let at = function.describe(p);
    match (left, right) {
        (Side::Absent, Side::Absent) => Err(refusal(format!("the expression is undefined on both sides of {at}"))),
        (Side::Unsettled { estimate, error, why }, _) => Err(unsettled(&format!("from the left of {at}"), estimate, error, why)),
        (_, Side::Unsettled { estimate, error, why }) => Err(unsettled(&format!("from the right of {at}"), estimate, error, why)),
        (Side::Absent, one) | (one, Side::Absent) => match one {
            Side::Converged { value, weighted, .. } => answer(value, &weighted, function),
            Side::Infinite(infinity) => answer(infinity, &[], function),
            _ => unreachable!("absent and unsettled sides are handled above"),
        },
        (Side::Converged { value: l, error: el, scale: sl, weighted: wl }, Side::Converged { value: r, error: er, scale: sr, weighted: wr }) => {
            if (l - r).abs() > TOLERANCE * l.abs().max(r.abs()).max(sl).max(sr) {
                return Err(disagree(&at, &format_float(l), &format_float(r), el.max(er)));
            }
            let weighted: Vec<(Quantity, f64)> = wl.into_iter().chain(wr).map(|(q, w)| (q, w / 2.0)).collect();
            answer((l + r) / 2.0, &weighted, function)
        }
        (Side::Infinite(l), Side::Infinite(r)) if l == r => answer(l, &[], function),
        (l, r) => Err(disagree(&at, &describe(&l), &describe(&r), 0.0)),
    }
}

fn describe(side: &Side) -> String {
    match side {
        Side::Converged { value, .. } => format_float(*value),
        Side::Infinite(infinity) => format_float(*infinity),
        _ => "no value".to_string(),
    }
}

fn disagree(at: &str, left: &str, right: &str, error: f64) -> PhysureError {
    let estimate = if error > 0.0 { format!(" (error estimate {error:e})") } else { String::new() };
    refusal(format!(
        "the limit at {at} does not exist: from the left the expression tends to {left}, from the right to {right}{estimate}"
    ))
}

fn unsettled(where_: &str, estimate: f64, error: f64, why: &str) -> PhysureError {
    refusal(format!(
        "the extrapolation {where_} did not settle: best estimate {estimate}, error estimate {error:e} ({why}); the \
         expression may oscillate, jump, or change on a scale smaller than the smallest step; \
         rewrite it so that the limit is a value of a continuous expression, or give the limit in closed form"
    ))
}

/// A plain number when no unit and no uncertainty is involved, otherwise the quantity.
fn plain_when_possible(value: PhsValue) -> PhsValue {
    match value {
        PhsValue::Quantity(q) if q.unit.is_plain_number() && q.value.std_dev() == 0.0 => PhsValue::Number(q.value.mean() + 0.0),
        other => other,
    }
}

/// The extrapolated `value` in the unit of the samples; with the core's weighted sum when a sample is uncertain.
fn answer(value: f64, weighted: &[(Quantity, f64)], function: &Function) -> PhysureResult<PhsValue> {
    let unit = function.unit.clone().unwrap_or_else(RationalUnit::dimensionless);
    let value = value + 0.0; // -0.0 is 0
    if weighted.iter().any(|(q, _)| q.value.std_dev() > 0.0) {
        let mut total = Quantity::new_scalar(0.0, 0.0, unit.clone(), None, None);
        for (q, w) in weighted {
            total = total.add(&q.mul(&plain(*w))?)?;
        }
        return Ok(PhsValue::Quantity(total));
    }
    Ok(plain_when_possible(PhsValue::Quantity(Quantity::new_scalar(value, 0.0, unit, None, None))))
}
