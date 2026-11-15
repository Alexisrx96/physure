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
