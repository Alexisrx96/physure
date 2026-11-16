//! The uncertainty settings of Phase 4b applied process-wide (`asymmetric_shape`, `default_dof`): a binary of its
//! own, because a process-wide change of either would change the results of the crate's unit tests that build a
//! default-shape value or read an undeclared degree of freedom without a lock. The tests here serialise on one lock
//! and put the compiled values back.

use physure_core::units::conf::parse_physure_conf;
use physure_core::units::UnitRegistry;
use physure_core::uncertainty::moments::MomentsBackend;
use physure_core::uncertainty::dof::{default_dof, set_default_dof};
use physure_core::uncertainty::shapes::{asymmetric_shape, set_asymmetric_shape};
use physure_core::uncertainty::Lineage;
use physure_core::uncertainty::ShapeKind;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads `text` as the lenient (unfrozen) reader reads a working directory's `physure.conf`.
fn read(text: &str) {
    parse_physure_conf(text, &mut UnitRegistry::new(), &mut HashMap::new());
}

#[test]
fn the_lenient_reader_applies_a_good_asymmetric_shape_and_keeps_the_one_in_force_for_a_bad_one() {
    let _lock = lock();
    let _ = physure_core::units::conf::build_registry_from_conf();
    assert_eq!(asymmetric_shape(), ShapeKind::Dimidiated, "the compiled default");
    read("[Settings]\nasymmetric_shape = fechner\n");
    assert_eq!(asymmetric_shape(), ShapeKind::Fechner);
    assert_eq!(MomentsBackend::measured(12.3, 0.4, 0.5).unwrap().shape, ShapeKind::Fechner, "a new measurement reads it");
    let elsewhere = std::thread::spawn(asymmetric_shape).join().unwrap();
    assert_eq!(elsewhere, ShapeKind::Fechner, "process-wide: every thread");
    for bad in ["foo", "", "1"] {
        read(&format!("[Settings]\nasymmetric_shape = {bad}\n"));
        assert_eq!(asymmetric_shape(), ShapeKind::Fechner, "{bad:?} replaced the shape in force");
    }
    assert_eq!(set_asymmetric_shape(ShapeKind::Dimidiated), ShapeKind::Fechner, "the setter returns the previous one");
    read("[Settings]\nasymmetric_shape = dimidiated\n");
    assert_eq!(asymmetric_shape(), ShapeKind::Dimidiated);
}

#[test]
fn the_lenient_reader_applies_a_good_default_dof_and_keeps_the_one_in_force_for_a_bad_one() {
    let _lock = lock();
    let _ = physure_core::units::conf::build_registry_from_conf();
    assert_eq!(default_dof(), f64::INFINITY, "the compiled default");
    let undeclared = Lineage::measured(1.0);
    let declared = Lineage::measured(1.0);
    declared.declare_dof(f64::INFINITY).unwrap();
    read("[Settings]\ndefault_dof = 50\n");
    assert_eq!((default_dof(), undeclared.effective_dof()), (50.0, 50.0));
    assert_eq!(declared.effective_dof(), f64::INFINITY, "an explicitly declared infinity stays infinite");
    assert_eq!(std::thread::spawn(default_dof).join().unwrap(), 50.0, "process-wide: every thread");
    for bad in ["0", "-1", "abc", "nan", "-inf"] {
        read(&format!("[Settings]\ndefault_dof = {bad}\n"));
        assert_eq!(default_dof(), 50.0, "{bad:?} replaced the default in force");
    }
    assert_eq!(set_default_dof(f64::INFINITY), 50.0, "the setter returns the previous one");
    read("[Settings]\ndefault_dof = inf\n");
    assert_eq!(undeclared.effective_dof(), f64::INFINITY);
}

/// `monte_carlo_seed` applied process-wide: a unit started afterwards draws the stream of the new seed, a new thread
/// stream derives from it, and `random` is entropy; a live stream is not restarted.
#[test]
fn the_seed_setting_is_read_when_a_unit_starts_or_a_thread_stream_is_created() {
    use physure_core::uncertainty::{monte_carlo_seed, set_monte_carlo_seed, with_evaluation_seed, with_seed, EvaluationStream, MonteCarloSeed};
    use physure_core::MonteCarloBackend;
    let _lock = lock();
    let _ = physure_core::units::conf::build_registry_from_conf();
    let samples = || MonteCarloBackend::from_stats(0.0, 1.0, 32).unwrap().samples.to_vec();
    assert_eq!(monte_carlo_seed(), MonteCarloSeed::Fixed(0), "the compiled default");
    assert_eq!(with_evaluation_seed(samples), with_seed(0, samples));
    let mut live = EvaluationStream::from_setting();
    let first = live.run(samples);
    read("[Settings]\nmonte_carlo_seed = 5\n");
    assert_eq!(monte_carlo_seed(), MonteCarloSeed::Fixed(5));
    assert_eq!(with_evaluation_seed(samples), with_seed(5, samples), "a new unit reads the new seed");
    assert_eq!(std::thread::spawn(monte_carlo_seed).join().unwrap(), MonteCarloSeed::Fixed(5), "process-wide");
    assert_eq!((first, live.run(samples)), with_seed(0, || (samples(), samples())), "the live stream goes on from 0");
    read("[Settings]\nmonte_carlo_seed = -3\n");
    assert_eq!(monte_carlo_seed(), MonteCarloSeed::Fixed(5), "a bad value keeps the seed in force");
    read("[Settings]\nmonte_carlo_seed = random\n");
    assert_ne!(with_evaluation_seed(samples), with_evaluation_seed(samples));
    set_monte_carlo_seed(MonteCarloSeed::DEFAULT);
}
