use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity;
use physure_core::units::RationalUnit;
use crate::value::PhsValue;

/// The magnitude a range endpoint denotes, or `None` when it denotes none.
pub(crate) fn range_endpoint(val: &PhsValue) -> Option<Quantity> {
    match val {
        PhsValue::Quantity(q) => Some(q.clone()),
        PhsValue::Number(n) => Some(Quantity::new_scalar(*n, 0.0, RationalUnit::dimensionless(), None, None)),
        _ => None,
    }
}

/// The number of a range's end in the unit of its start, the one every reader of a range (a `for`, a plot) counts in:
/// the end's raw number read in the start's unit made `1 km .. 3000 m` run to 2999 km and `0 K .. 2 degC` stop at
/// 1 K. A dimensionless start keeps the end's number as written (`0 .. 50 %` runs to 50).
pub(crate) fn range_end_in_start_unit(start: &PhsValue, end: &PhsValue) -> PhysureResult<f64> {
    match (start, end) {
        (PhsValue::Quantity(s), PhsValue::Quantity(e)) if !s.unit.dimensions.is_empty() && s.unit != e.unit => {
            Ok(e.convert_to(&s.unit)?.value.mean())
        }
        (_, PhsValue::Quantity(e)) => Ok(e.value.mean()),
        (_, PhsValue::Number(n)) => Ok(*n),
        _ => Err(PhysureError::Generic("Range end must be a number or quantity".into())),
    }
}

/// Builds `min .. max`, after checking that it names an interval at all.
///
/// An endpoint with no dimension of its own takes the other's unit, so `0 .. 100 m` reads
/// as `0 m .. 100 m` — on paper the lower bound of an interval does not repeat the unit
/// either. Everything else is refused rather than repaired: a range whose sides measure
/// different things, one that does not run upwards, and anything that is not a magnitude.
/// A missing endpoint never reaches here; the grammar requires both.
pub(crate) fn make_range(l_val: PhsValue, r_val: PhsValue) -> PhysureResult<PhsValue> {
    let (Some(mut min), Some(mut max)) = (range_endpoint(&l_val), range_endpoint(&r_val)) else {
        return Err(PhysureError::Generic(format!(
            "A range runs between two magnitudes, and `{} .. {}` has something else on at least one side",
            l_val, r_val,
        )));
    };

    if min.unit.dimensions.is_empty() && !max.unit.dimensions.is_empty() {
        min.unit = max.unit.clone();
    } else if max.unit.dimensions.is_empty() && !min.unit.dimensions.is_empty() {
        max.unit = min.unit.clone();
    } else if !min.unit.same_dimensions(&max.unit) {
        return Err(PhysureError::IncompatibleDimensions {
            op: "range",
            dim1: min.unit.__repr__(),
            dim2: max.unit.__repr__(),
        });
    }
    // A temperature point and a temperature interval do not bound one range (`5 delta_degC .. 20 degC` ran over 15
    // numbers read in delta_degC).
    if min.unit.point_against_interval(&max.unit) {
        if let Some(crate::builtins::dimensions::Mismatch::TemperatureKind(why)) =
            crate::builtins::dimensions::mismatch(&l_val, &r_val)
        {
            return Err(PhysureError::Generic(format!(
                "a range of a temperature point and a temperature interval: {why}; {}",
                physure_core::quantity::POINT_AGAINST_INTERVAL_ADVICE
            )));
        }
    }
    let (lo, hi) = (min.canonical_magnitude(), max.canonical_magnitude());
    if lo.is_nan() || hi.is_nan() {
        return Err(PhysureError::Generic(format!(
            "A range needs two magnitudes that can be ordered, and `{} .. {}` has one that cannot",
            min, max,
        )));
    }
    if lo >= hi {
        return Err(PhysureError::Generic(format!(
            "A range runs from its minimum to its maximum: `{}` is not below `{}`",
            min, max,
        )));
    }

    // A bare number stays a bare number when nothing was adopted — a dimensionless range is
    // written `-2 .. 2` and the consumers that read one distinguish the two cases.
    let rewrap = |original: PhsValue, q: Quantity| match original {
        PhsValue::Number(_) if q.unit.dimensions.is_empty() => original,
        _ => PhsValue::Quantity(q),
    };
    Ok(PhsValue::Range(
        Box::new(rewrap(l_val, min)),
        Box::new(rewrap(r_val, max)),
    ))
}

/// The truth of a condition (`if`, `while`, `? :`, `if ... then return`, `@requires`, `@ensures`, a conditional
/// breakpoint) and of an operand of `and`, `or`, `not`, `assert`: only a Bool has one. A number or a quantity is not a
/// condition (`x != 0 m`, `x != 0` say what is meant; "exactly zero is false" read a unit-dependent convention into
/// it), nor is text (`"false"` is not `False`), nor anything else: each is a type error from [`not_a_bool`].
pub(crate) fn condition_of(value: &PhsValue, context: &str) -> PhysureResult<bool> {
    match value {
        PhsValue::Bool(b) => Ok(*b),
        other => Err(PhysureError::Generic(not_a_bool(context, other))),
    }
}

/// The one table of what a condition or a logical operand that is not a Bool is told, by its type:
///
/// | value | message, after "`if` needs a Bool (True or False), got" |
/// |---|---|
/// | a number | a Number (1.0): a number is not a condition; compare it, e.g. `x != 0` |
/// | a quantity | a Quantity (1.0 m): a quantity is not a condition; compare it, e.g. `x != 0 m` (in its own unit) |
/// | the text "true" or "false" | a String ("true"): text is not a condition; the Bool literals are True and False |
/// | other text | a String ("on"): text is not a condition; compare it, e.g. s == "on" |
/// | anything else | a Vector: a Vector is not a condition (Matrix, Range, Function, Plot, Sigma, ...) |
/// | None | None: None is not a condition |
///
/// An identifier with no value is the error `` `foo` has no value `` before it gets here.
pub(crate) fn not_a_bool(context: &str, value: &PhsValue) -> String {
    let kind = value.type_name();
    let article = if kind.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
    let shown = |text: String| -> String {
        if text.chars().count() > 40 {
            format!("{}...", text.chars().take(40).collect::<String>())
        } else {
            text
        }
    };
    let what = match value {
        PhsValue::Number(_) => not_a_number_condition(&format!(" ({})", shown(value.to_string()))),
        PhsValue::Quantity(q) if q.unit.is_plain_number() => not_a_number_condition(&format!(" ({})", shown(value.to_string()))),
        PhsValue::Quantity(q) => not_a_quantity_condition(
            &format!(" ({})", shown(value.to_string())),
            // a level that reaches here nameless by some arithmetic step still has a unit
            &Some(q.unit.__repr__()).filter(|unit| !unit.is_empty()).unwrap_or_else(|| "<its unit>".to_string()),
        ),
        PhsValue::String(text) => not_a_text_condition(&shown(text.clone())),
        PhsValue::None => "None: None is not a condition".to_string(),
        _ => format!("{article} {kind}: {article} {kind} is not a condition"),
    };
    format!("{context} needs a Bool (True or False), got {what}")
}

/// What the static checker knows of a condition that is not a Bool, for [`not_a_bool_statically`].
pub(crate) enum KnownCondition<'a> {
    /// A dimensionless number.
    Number,
    /// A quantity, described (`of dimension L`, `a level`), with a unit to write its zero in when one is known (a
    /// level or a temperature point has its own, which the checker does not keep).
    Quantity { described: String, unit: Option<String> },
    /// Text, with its content when the checker read it from a literal.
    Text(Option<&'a str>),
    /// A vector literal.
    Vector,
    /// A name that is no variable, constant, unit or function: it has no value (evaluating it is an error).
    Unbound(&'a str),
}

/// The same table as [`not_a_bool`], for the static checker (`phs check`), which knows the type of the condition but
/// not its value: the words are the interpreter's, without the value in parentheses.
pub(crate) fn not_a_bool_statically(context: &str, known: KnownCondition<'_>) -> String {
    let what = match known {
        KnownCondition::Number => not_a_number_condition(""),
        KnownCondition::Quantity { described, unit: Some(unit) } => not_a_quantity_condition(&format!(" ({described})"), &unit),
        KnownCondition::Quantity { described, unit: None } => {
            format!("a Quantity ({described}): a quantity is not a condition; compare it with a value in its own unit")
        }
        KnownCondition::Text(Some(text)) => not_a_text_condition(text),
        KnownCondition::Text(None) => "a String: text is not a condition; compare it, e.g. `s == \"on\"`".to_string(),
        KnownCondition::Vector => "a Vector: a Vector is not a condition".to_string(),
        KnownCondition::Unbound(name) => {
            return format!("`{name}` has no value; the Bool literals are True and False ({context} needs a Bool)");
        }
    };
    format!("{context} needs a Bool (True or False), got {what}")
}

fn not_a_number_condition(shown: &str) -> String {
    format!("a Number{shown}: a number is not a condition; compare it, e.g. `x != 0`")
}

fn not_a_quantity_condition(shown: &str, unit: &str) -> String {
    format!("a Quantity{shown}: a quantity is not a condition; compare it, e.g. `x != 0 {unit}`")
}

fn not_a_text_condition(text: &str) -> String {
    if text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false") {
        format!("a String (\"{text}\"): text is not a condition; the Bool literals are True and False")
    } else {
        format!("a String (\"{text}\"): text is not a condition; compare it, e.g. `s == \"{text}\"`")
    }
}


/// Trailing `#` / `//` comments survive into a unit annotation's text; the unit parser
/// must never see them.
pub(crate) fn strip_unit_comment(text: &str) -> &str {
    text.split('#').next().unwrap().split("//").next().unwrap().trim()
}

/// The thread-local scopes of the thread that starts a parallel section (a long `for` expression, `parallel_map`),
/// which every rayon task opens again: a worker is another thread and sees none of them. Without this, a program
/// evaluated inside `with_catalog` (every WASM and Java evaluation) read the embedded catalog in a loop of
/// `parallel_threshold` items or more, and a scoped angle mode, propagation mode or loop ceiling did not reach the
/// body either. The same holds for the uncertainty settings a host or a witness pins on its thread (the Monte Carlo
/// sample count, the `while` ceiling, the asymmetric shape, the default degrees of freedom, the seed setting a
/// worker's own stream would start from) and the display rules a witness pins (the text a loop body builds).
///
/// Not carried: the evaluation unit (the Monte Carlo stream) of the caller: each task drawing the caller's sequence
/// would make the draws of every item the same. A loop in `monte_carlo` mode, the only mode that draws, stays on the
/// caller's thread instead (`PhsInterpreter::runs_in_parallel`). Nor the call depth and the stack floor of `limits`,
/// which belong to each thread's own stack.
#[derive(Clone, Copy)]
pub(crate) struct CallerScopes<'a> {
    catalog: Option<&'a physure_core::UnitRegistry>,
    angle_mode: physure_core::AngleMode,
    propagation_mode: physure_core::PropagationMode,
    parallel_threshold: usize,
    max_loop_iterations: usize,
    max_pipeline_steps: usize,
    monte_carlo_samples: usize,
    max_while_iterations: usize,
    asymmetric_shape: physure_core::uncertainty::ShapeKind,
    default_dof: f64,
    monte_carlo_seed: physure_core::uncertainty::MonteCarloSeed,
    display_rules: physure_core::settings::DisplayRules,
}

impl CallerScopes<'_> {
    /// Calls `f` with the scopes in force on this thread; they stay valid for as long as `f` runs.
    pub(crate) fn capture<R>(f: impl FnOnce(CallerScopes<'_>) -> R) -> R {
        physure_core::units::with_current_catalog(|catalog| {
            f(CallerScopes {
                catalog,
                angle_mode: physure_core::angle_mode(),
                propagation_mode: physure_core::propagation_mode(),
                parallel_threshold: physure_core::settings::parallel_threshold(),
                max_loop_iterations: physure_core::settings::max_loop_iterations(),
                max_pipeline_steps: physure_core::settings::max_pipeline_steps(),
                monte_carlo_samples: physure_core::settings::monte_carlo_samples(),
                max_while_iterations: physure_core::settings::max_while_iterations(),
                asymmetric_shape: physure_core::uncertainty::shapes::asymmetric_shape(),
                default_dof: physure_core::uncertainty::dof::default_dof(),
                monte_carlo_seed: physure_core::uncertainty::monte_carlo_seed(),
                display_rules: physure_core::settings::display_rules(),
            })
        })
    }

    /// Runs `f` on this thread under the captured scopes.
    pub(crate) fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        let _angle = physure_core::units::angle_mode::scoped(self.angle_mode);
        let _propagation = physure_core::uncertainty::mode::scoped(self.propagation_mode);
        let _threshold = physure_core::settings::scoped(self.parallel_threshold);
        let _loops = physure_core::settings::scoped_max_loop_iterations(self.max_loop_iterations);
        let _steps = physure_core::settings::scoped_max_pipeline_steps(self.max_pipeline_steps);
        let _samples = physure_core::settings::scoped_monte_carlo_samples(self.monte_carlo_samples);
        let _while = physure_core::settings::scoped_max_while_iterations(self.max_while_iterations);
        let _shape = physure_core::uncertainty::shapes::scoped_asymmetric_shape(self.asymmetric_shape);
        let _dof = physure_core::uncertainty::dof::scoped_default_dof(self.default_dof);
        let _seed = physure_core::uncertainty::scoped_monte_carlo_seed(self.monte_carlo_seed);
        let _display = physure_core::settings::scoped_display_rules(self.display_rules);
        physure_core::units::with_scoped_catalog(self.catalog, f)
    }
}

#[cfg(test)]
mod tests {
    use super::CallerScopes;

    /// A PHS program cannot build an asymmetric value yet, so the asymmetric shape is checked on the mechanism itself:
    /// a scope opened on the caller's thread is in force inside `run` on another thread (a rayon worker).
    #[test]
    fn the_scoped_asymmetric_shape_is_carried_to_another_thread() {
        use physure_core::uncertainty::shapes::{asymmetric_shape, scoped_asymmetric_shape};
        use physure_core::uncertainty::ShapeKind;
        let _fechner = scoped_asymmetric_shape(ShapeKind::Fechner);
        let (inside, outside) = CallerScopes::capture(|scopes| {
            std::thread::scope(|s| {
                let inside = s.spawn(|| scopes.run(asymmetric_shape)).join().unwrap();
                let outside = s.spawn(asymmetric_shape).join().unwrap();
                (inside, outside)
            })
        });
        assert_eq!(outside, ShapeKind::Dimidiated, "another thread does not see the scope by itself (not vacuous)");
        assert_eq!(inside, ShapeKind::Fechner);
    }

    /// The seed setting a worker's own stream would start from is the caller's scoped one; the caller's evaluation
    /// unit is not carried (a worker drawing the caller's stream would repeat its draws).
    #[test]
    fn the_scoped_seed_setting_is_carried_to_another_thread_but_not_the_unit() {
        use physure_core::uncertainty::{in_evaluation_unit, monte_carlo_seed, scoped_monte_carlo_seed, with_seed, MonteCarloSeed};
        let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(12345));
        let (inside, outside) = with_seed(1, || {
            CallerScopes::capture(|scopes| {
                std::thread::scope(|s| {
                    let inside = s.spawn(|| scopes.run(|| (monte_carlo_seed(), in_evaluation_unit()))).join().unwrap();
                    let outside = s.spawn(monte_carlo_seed).join().unwrap();
                    (inside, outside)
                })
            })
        });
        assert_ne!(outside, MonteCarloSeed::Fixed(12345), "not vacuous");
        assert_eq!(inside, (MonteCarloSeed::Fixed(12345), false));
    }
}
