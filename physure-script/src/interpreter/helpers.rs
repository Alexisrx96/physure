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
/// An identifier with no value is told so by the interpreter before it gets here (it evaluates to its own name).
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
    /// A name that is no variable, constant, unit or function: it evaluates to its own text.
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
/// sample count, ...).
///
/// Not carried: a `with_seed` scope (each task drawing the caller's sequence would make the draws of every item the
/// same; a seeded evaluation is sequential, `Limits::sequential`), and the call depth and the stack floor of
/// `limits`, which belong to each thread's own stack.
#[derive(Clone, Copy)]
pub(crate) struct CallerScopes<'a> {
    catalog: Option<&'a physure_core::UnitRegistry>,
    angle_mode: physure_core::AngleMode,
    propagation_mode: physure_core::PropagationMode,
    parallel_threshold: usize,
    max_loop_iterations: usize,
    max_pipeline_steps: usize,
    monte_carlo_samples: usize,
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
        physure_core::units::with_scoped_catalog(self.catalog, f)
    }
}
