//! The Monte Carlo seed policy seen from PHS (conf freeze Phase 4b, `[Settings] monte_carlo_seed`): an interpreter is
//! one evaluation unit, whose stream starts at the seed in force and goes on across every statement it runs, so a
//! program gives the same samples on every run and two measurements in it get independent samples.
//!
//! Every test pins the seed setting and the propagation mode on its own thread (scoped guards): the process-wide
//! values are never changed here.

use physure_core::uncertainty::mode::scoped;
use physure_core::uncertainty::{scoped_monte_carlo_seed, with_seed, MonteCarloSeed, PropagationMode};
use physure_script::interpreter::eval_phs;
use physure_script::{parse_phs, PhsInterpreter, PhsValue};

/// `(mean, standard uncertainty)` of a quantity.
fn parts(value: &PhsValue) -> (f64, f64) {
    match value {
        PhsValue::Quantity(q) => (q.value.mean(), q.value.std_dev()),
        other => panic!("not a quantity: {other:?}"),
    }
}

/// The values of `program`, each statement through a fresh interpreter's run (as `phs file.phs` runs it).
fn run(program: &str) -> Vec<(f64, f64)> {
    eval_phs(program).unwrap().iter().map(parts).collect()
}

const PROGRAM: &str = "x = 10 +/- 1 m\ny = 10 +/- 1 m\nd = x - y\n";

#[test]
fn a_program_gives_the_same_samples_on_every_run_and_they_are_those_of_its_seed() {
    let _mode = scoped(PropagationMode::MonteCarlo);
    let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(0));
    let first = run(PROGRAM);
    assert_eq!(run(PROGRAM), first, "two runs of one program");
    assert_eq!(with_seed(0, || run(PROGRAM)), first, "the stream of seed 0 (phs eval --seed 0)");
    let _seven = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(7));
    assert_ne!(run(PROGRAM), first);
    let _random = scoped_monte_carlo_seed(MonteCarloSeed::Random);
    assert_ne!(run(PROGRAM), run(PROGRAM));
}

/// `x = 10 ± 1 m; y = 10 ± 1 m; x - y`: two measurements, so independent draws, and the uncertainty of the
/// difference is `sqrt(2)` times that of one (within Monte Carlo noise: with 1000 samples a sample deviation is
/// known to about 2 %), never the exact 0 of two identical sample arrays.
#[test]
fn two_measurements_in_one_program_are_drawn_independently() {
    let _mode = scoped(PropagationMode::MonteCarlo);
    let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(0));
    let values = run(PROGRAM);
    let (ux, ud) = (values[0].1, values[2].1);
    assert_ne!(values[0], values[1], "two measurements, two sample arrays");
    let ratio = ud / ux;
    assert!((ratio - 2f64.sqrt()).abs() < 0.1, "u(x - y) / u(x) = {ratio}");
}

/// The reviewer's LSP scenario: an interpreter that lives on (an LSP document, a REPL session) runs `x` in one call
/// and `y` in a later one, with the configuration read again in between (`reset_cache`, as a changed stamp of the
/// frozen file does). The later measurement continues the stream, so `x - y` is not exactly 0 and the two calls
/// give what one run of the whole program gives.
#[test]
fn an_interpreter_that_lives_on_continues_its_stream_across_calls_and_a_new_reading_of_the_configuration() {
    let _mode = scoped(PropagationMode::MonteCarlo);
    let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(0));
    let mut interp = PhsInterpreter::default();
    assert_eq!(interp.evaluation_stream(), None, "nothing drawn yet");
    let program = parse_phs(PROGRAM).unwrap();
    interp.run_statement(&program.statements[0]).unwrap();
    assert_eq!(interp.evaluation_stream().and_then(|s| s.draws()), Some(1));
    physure_core::frozen::reset_cache();
    let _again = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(0));
    interp.run_statement(&program.statements[1]).unwrap();
    let d = interp.run_statement(&program.statements[2]).unwrap();
    assert_ne!(parts(&d).1, 0.0, "x and y share no samples");
    let whole = run(PROGRAM);
    assert_eq!(parts(&d), whole[2], "the calls continue one unit");
    assert_eq!(parts(interp.get_var("y").unwrap()), whole[1]);
    // a clone (a derived function, a module) shares the stream
    let clone = interp.clone();
    let z = clone.eval_statement_with_env(&parse_phs("z = 1 +/- 0.1 m").unwrap().statements[0], &mut Default::default()).unwrap();
    assert_eq!(interp.evaluation_stream().and_then(|s| s.draws()), Some(3));
    assert_eq!(parts(&z), with_seed(0, || run(&format!("{PROGRAM}z = 1 +/- 0.1 m\n")))[3]);
}

/// An explicit seed open on the thread (`phs eval --seed`, a witness) wins over the interpreter's own stream and
/// leaves it untouched.
#[test]
fn an_explicit_seed_wins_and_leaves_the_interpreter_stream_alone() {
    let _mode = scoped(PropagationMode::MonteCarlo);
    let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(0));
    let mut interp = PhsInterpreter::default();
    let x = parse_phs("x = 10 +/- 1 m").unwrap();
    let seeded = with_seed(3, || parts(&interp.run_statement(&x.statements[0]).unwrap()));
    assert_eq!(interp.evaluation_stream(), None);
    assert_eq!(seeded, with_seed(3, || run("x = 10 +/- 1 m"))[0]);
}

/// The bits of every item of the vector `name`.
fn items(interp: &PhsInterpreter, name: &str) -> Vec<(u64, u64)> {
    match interp.get_var(name) {
        Some(PhsValue::Vector(items)) => items.iter().map(parts).map(|(mean, u)| (mean.to_bits(), u.to_bits())).collect(),
        other => panic!("{name}: {other:?}"),
    }
}

/// The items of the vector `name`.
fn values(interp: &PhsInterpreter, name: &str) -> Vec<PhsValue> {
    match interp.get_var(name) {
        Some(PhsValue::Vector(items)) => items.to_vec(),
        other => panic!("{name}: {other:?}"),
    }
}

/// Runs `program` on a new interpreter and returns it with the number of loops it handed to rayon workers.
fn looped(program: &str) -> (PhsInterpreter, u64) {
    let before = physure_script::interpreter::parallel_loops_started();
    let mut interp = PhsInterpreter::default();
    for statement in &parse_phs(program).unwrap().statements {
        interp.run_statement(statement).unwrap();
    }
    (interp, physure_script::interpreter::parallel_loops_started() - before)
}

const LOOPS: &str = "r = for i in 1..301 { 10 +/- 1 m }\nfn f(x) = x + (0 +/- 1 m)\np = parallel_map(f, r)\n";

/// A loop in `monte_carlo` mode stays on the caller's thread, whatever `parallel_threshold` says (conf freeze Phase
/// 4b): its draws are the next ones of the unit's stream, in item order, so the values do not depend on the number
/// of workers, their scheduling, or the threshold (a frozen setting). On workers each item drew from the stream of
/// whichever thread ran it, and the unit's stream did not advance.
#[test]
fn a_monte_carlo_loop_stays_sequential_and_draws_the_stream_of_its_unit() {
    let _mode = scoped(PropagationMode::MonteCarlo);
    let _seed = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(0));
    let run = |threshold: usize| {
        let _threshold = physure_core::settings::scoped(threshold);
        let (interp, parallel) = looped(LOOPS);
        assert_eq!(parallel, 0, "threshold {threshold}: no loop of a Monte Carlo evaluation goes to workers");
        assert!(!interp.runs_in_parallel());
        // 300 measurements in the `for`, 300 more in `parallel_map`: all from the interpreter's stream
        assert_eq!(interp.evaluation_stream().and_then(|s| s.draws()), Some(600), "threshold {threshold}");
        (items(&interp, "r"), items(&interp, "p"))
    };
    let below = run(usize::MAX);
    assert_eq!(run(0), below, "the values do not depend on parallel_threshold");
    assert_eq!(run(0), below, "nor on the run");
    let distinct: std::collections::HashSet<_> = below.0.iter().collect();
    assert_eq!(distinct.len(), 300, "every item is a measurement of its own");
    // the stream of seed 0, in item order: what `phs eval --seed 0` (always sequential) gives
    let seeded = with_seed(0, || {
        let _threshold = physure_core::settings::scoped(0);
        let (interp, _) = looped(LOOPS);
        (items(&interp, "r"), items(&interp, "p"))
    });
    assert_eq!(seeded, below);
}

/// A loop that cannot draw (any other propagation mode) still goes to the workers at `parallel_threshold` items,
/// and gives the values of the sequential path: the default seed did not make every loop sequential.
#[test]
fn a_loop_outside_monte_carlo_mode_still_runs_on_the_workers() {
    for mode in [PropagationMode::Correlated, PropagationMode::Uncorrelated] {
        let _mode = scoped(mode);
        let program = "fn f(x) = x * 2\np = parallel_map(f, vector(1, 2, 3))\nr = for i in 1..20001 { i * 3 + 1 }\n";
        let parallel = {
            let _threshold = physure_core::settings::scoped(10_000);
            let (interp, parallel) = looped(program);
            assert!(interp.runs_in_parallel(), "{mode:?}");
            assert_eq!(parallel, 2, "{mode:?}: the `parallel_map` and the `for` (20000 items)");
            values(&interp, "r")
        };
        let _threshold = physure_core::settings::scoped(usize::MAX);
        let (interp, loops) = looped("r = for i in 1..20001 { i * 3 + 1 }\n");
        assert_eq!(loops, 0, "below the threshold: the caller's thread");
        assert_eq!(values(&interp, "r"), parallel, "{mode:?}");
        assert_eq!(parallel.len(), 20000);
        assert_eq!(parts(&parallel[19999]), (60001.0, 0.0));
    }
}
