//! The uncertainty settings of Phase 4b applied process-wide (`asymmetric_shape`, `default_dof`): a binary of its
//! own, because a process-wide change of either would change the results of the crate's unit tests that build a
//! default-shape value or read an undeclared degree of freedom without a lock. The tests here serialise on one lock
//! and put the compiled values back.

use physure_core::units::conf::parse_physure_conf;
use physure_core::units::UnitRegistry;
use physure_core::uncertainty::moments::MomentsBackend;
use physure_core::uncertainty::shapes::{asymmetric_shape, set_asymmetric_shape};
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
