use criterion::{criterion_group, criterion_main, Criterion};
use physure_core::{Quantity, RationalUnit, CovarianceStore, PruningConfig};
use physure_core::units::Parser;
use ndarray::ArrayD;

fn bench_units(c: &mut Criterion) {
    let m = RationalUnit::new_from_dimensions([("m".to_string(), (1, 1))]);
    let s = RationalUnit::new_from_dimensions([("s".to_string(), (1, 1))]);

    c.bench_function("unit_mul", |b| {
        b.iter(|| m.mul(&s))
    });

    c.bench_function("unit_div", |b| {
        b.iter(|| m.div(&s))
    });
}

/// Units whose scale is not 1, which is where an exact scale costs something: the scale-1 units
/// of `bench_units` take the fast path whatever the representation.
fn bench_prefixed_units(c: &mut Criterion) {
    let km = Parser::parse_expression("km").unwrap();
    let ms = Parser::parse_expression("ms").unwrap();
    let mile = Parser::parse_expression("mile").unwrap();
    let q = Quantity::with_uncertainty(5.0, 0.0, "km").unwrap();

    c.bench_function("unit_mul_prefixed", |b| b.iter(|| km.mul(&ms)));
    c.bench_function("unit_div_prefixed", |b| b.iter(|| km.div(&ms)));
    c.bench_function("quantity_convert_prefixed", |b| b.iter(|| q.convert_to(&mile).unwrap()));
    c.bench_function("parse_compound_unit", |b| b.iter(|| Parser::parse_expression("kg*m/s^2").unwrap()));
    c.bench_function("parse_prefixed_compound_unit", |b| b.iter(|| Parser::parse_expression("km/h").unwrap()));
}

fn bench_quantity_arithmetic(c: &mut Criterion) {
    let u1 = RationalUnit::new_from_dimensions([("m".to_string(), (1, 1))]);
    let q1 = Quantity::new_scalar(10.0, 0.5, u1.clone(), None, None);
    let q2 = Quantity::new_scalar(5.0, 0.2, u1.clone(), None, None);

    c.bench_function("quantity_add_scalar", |b| {
        b.iter(|| q1.add(&q2).unwrap())
    });

    c.bench_function("quantity_mul_scalar", |b| {
        b.iter(|| q1.mul(&q2).unwrap())
    });
}

fn bench_covariance_propagation(c: &mut Criterion) {
    let mut store = CovarianceStore::new(PruningConfig::default());
    let var_data = vec![1.0, 0.1, 0.1, 2.0];
    let shape = vec![2, 2];
    store.register_variable_slice(1, &var_data, &shape);
    store.register_variable_slice(2, &var_data, &shape);

    let jacobians = vec![(var_data.as_slice(), shape.as_slice()), (var_data.as_slice(), shape.as_slice())];

    c.bench_function("covariance_propagate", |b| {
        b.iter(|| {
            store.propagate_slices(3, vec![1, 2], jacobians.clone());
        })
    });
}

criterion_group!(benches, bench_units, bench_prefixed_units, bench_quantity_arithmetic, bench_covariance_propagation);
criterion_main!(benches);
