//! The thread-local settings a host or a witness opens on its thread reach the body of a parallel loop (rayon
//! workers are other threads): `CallerScopes` carries each of them. Every test makes every loop parallel with
//! `settings::scoped(0)` (at and above `parallel_threshold`) and runs the same program sequentially for comparison.

use physure_core::uncertainty::{mode, PropagationMode, UncertaintyValue};
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// The elements of the vector the last statement of `source` gives.
fn items(source: &str) -> Vec<PhsValue> {
    match eval_phs(source).unwrap_or_else(|e| panic!("{source}: {e}")).into_iter().last() {
        Some(PhsValue::Vector(items)) => items,
        other => panic!("{source}: {other:?}"),
    }
}

/// `source` with every loop parallel and with every loop sequential.
fn both_ways<R>(f: impl Fn() -> R) -> (R, R) {
    let parallel = {
        let _parallel = physure_core::settings::scoped(0);
        f()
    };
    let sequential = {
        let _sequential = physure_core::settings::scoped(usize::MAX);
        f()
    };
    (parallel, sequential)
}

#[test]
fn a_scoped_while_ceiling_reaches_a_parallel_loop() {
    let _ceiling = physure_core::settings::scoped_max_while_iterations(10);
    let program = "fn count(n) =\n    i = 0\n    while i < n { i = i + 1 }\n    i\nv = for k in 0..16 { count(50) }";
    let (parallel, sequential) = both_ways(|| eval_phs(program).map(|_| ()).map_err(|e| e.to_string()));
    assert!(sequential.unwrap_err().contains("after 10 iterations"));
    let parallel = parallel.expect_err("the caller's ceiling must reach the rayon workers");
    assert!(parallel.contains("after 10 iterations"), "{parallel}");
    let _raised = physure_core::settings::scoped_max_while_iterations(100);
    let (parallel, sequential) = both_ways(|| items(program).len());
    assert_eq!((parallel, sequential), (16, 16));
}

#[test]
fn a_scoped_default_dof_reaches_a_parallel_loop() {
    let _fifty = physure_core::uncertainty::dof::scoped_default_dof(50.0);
    let dofs = || -> Vec<f64> {
        items("v = for i in 0..16 { dof(10 +/- 1 m) }")
            .into_iter()
            .map(|item| match item {
                PhsValue::Number(n) => n,
                other => panic!("{other:?}"),
            })
            .collect()
    };
    let (parallel, sequential) = both_ways(dofs);
    assert_eq!(sequential, vec![50.0; 16]);
    assert_eq!(parallel, vec![50.0; 16], "the caller's default must reach the rayon workers");
}

#[test]
fn a_scoped_monte_carlo_sample_count_reaches_a_parallel_loop() {
    let _mode = mode::scoped(PropagationMode::MonteCarlo);
    let _samples = physure_core::settings::scoped_monte_carlo_samples(37);
    let counts = || -> Vec<usize> {
        items("v = for i in 0..16 { 10 +/- 1 m }")
            .into_iter()
            .map(|item| match item {
                PhsValue::Quantity(q) => match &q.value {
                    UncertaintyValue::MonteCarlo(mc) => mc.samples.len(),
                    other => panic!("not Monte Carlo: {}", other.get_model_name()),
                },
                other => panic!("{other:?}"),
            })
            .collect()
    };
    let (parallel, sequential) = both_ways(counts);
    assert_eq!(sequential, vec![37; 16]);
    assert_eq!(parallel, vec![37; 16], "the caller's count must reach the rayon workers");
}
