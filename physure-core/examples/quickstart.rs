//! Minimal tour: exact units, dimension-checked arithmetic, uncertainty.
use physure_core::{Quantity, RationalUnit};

fn main() {
    let metre = RationalUnit::new_from_dimensions([("m".to_string(), (1, 1))]);
    let second = RationalUnit::new_from_dimensions([("s".to_string(), (1, 1))]);

    // 10.0 ± 0.1 m  /  2.0 ± 0.05 s  ->  5 m/s with propagated uncertainty
    let d = Quantity::new_scalar(10.0, 0.1, metre.clone(), None, None);
    let t = Quantity::new_scalar(2.0, 0.05, second.clone(), None, None);
    let v = d.div(&t).expect("compatible dimensions");

    println!("v = {} ± {} (unit id {})", v.value.mean(), v.value.std_dev(), v.unit.id);

    // Dimension errors are caught, not silently ignored:
    assert!(d.add(&t).is_err(), "m + s must be rejected");

    // Rational exponents stay exact: (m^2)^(1/2) == m
    let area = metre.mul(&metre);
    let side = area.pow(num_rational::Rational64::new(1, 2));
    assert_eq!(side, metre);
    println!("sqrt(m^2) == m: exact");
}
