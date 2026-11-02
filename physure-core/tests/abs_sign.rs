//! The absolute value is `-x` where `x` is negative, so its derivative is `sign(x)` and not `1`: the spread
//! of `|x|` is the same either way, but what it shares with `x` is not. Taking `1` for a negative `x` made
//! `|x| + x` (which is exactly zero) come out at `2 u`.

use physure_core::Quantity;

fn measured(value: f64) -> Quantity {
    Quantity::with_uncertainty(value, 0.1, "m").unwrap()
}

fn spread(q: &Quantity) -> (f64, f64) {
    (q.value.mean(), q.value.std_dev())
}

#[test]
fn the_absolute_value_of_a_negative_measurement_is_minus_it_and_cancels_against_it() {
    let x = measured(-3.0);
    let magnitude = x.abs().unwrap();
    assert_eq!(spread(&magnitude), (3.0, 0.1));
    assert_eq!(spread(&magnitude.add(&x).unwrap()), (0.0, 0.0), "|x| + x = 0 for a negative x");
    assert!((magnitude.sub(&x).unwrap().value.std_dev() - 0.2).abs() < 1e-12, "|x| - x = -2x");
}

#[test]
fn the_absolute_value_of_a_positive_measurement_is_the_measurement_itself() {
    let x = measured(3.0);
    let magnitude = x.abs().unwrap();
    assert_eq!(spread(&magnitude.sub(&x).unwrap()), (0.0, 0.0));
    assert!((magnitude.add(&x).unwrap().value.std_dev() - 0.2).abs() < 1e-12);
}

#[test]
fn at_zero_the_spread_carries_over_as_before() {
    assert_eq!(spread(&measured(0.0).abs().unwrap()), (0.0, 0.1));
}
