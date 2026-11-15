//! The display rules of Phase 4b applied process-wide (`display_significant_digits`, `display_scientific_below`,
//! `display_scientific_from`, `uncertainty_digits`): a binary of its own, because a process-wide change of them would
//! change the text that hundreds of tests of the crate assert without a lock. The tests here serialise on one lock and
//! put the compiled rules back.

use physure_core::quantity::{format_float, gum_round};
use physure_core::settings::{display_rules, scoped_display_rules, set_display_rules, DisplayRules, UncertaintyDigits};
use physure_core::units::conf::parse_physure_conf;
use physure_core::units::UnitRegistry;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

/// The lock, and the compiled rules again when the guard drops (also on a failed assertion).
struct Restore(#[allow(dead_code)] MutexGuard<'static, ()>);

impl Drop for Restore {
    fn drop(&mut self) {
        set_display_rules(DisplayRules::DEFAULT);
    }
}

fn lock() -> Restore {
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = physure_core::units::conf::build_registry_from_conf();
    Restore(guard)
}

/// Reads `text` as the lenient (unfrozen) reader reads a working directory's `physure.conf`.
fn read(text: &str) {
    parse_physure_conf(text, &mut UnitRegistry::new(), &mut HashMap::new());
}

#[test]
fn the_lenient_reader_applies_good_display_settings_and_keeps_the_ones_in_force_for_bad_ones() {
    let _restore = lock();
    assert_eq!(display_rules(), DisplayRules::DEFAULT, "the compiled rules");
    read("[Settings]\ndisplay_significant_digits = 6\ndisplay_scientific_below = 0.01\ndisplay_scientific_from = 1e6\nuncertainty_digits = 2\n");
    let configured = DisplayRules {
        significant_digits: 6,
        scientific_below: 0.01,
        scientific_from: 1e6,
        uncertainty_digits: UncertaintyDigits::Figures(2),
    };
    assert_eq!(display_rules(), configured);
    assert_eq!(std::thread::spawn(display_rules).join().unwrap(), configured, "process-wide: every thread");
    assert_eq!(format_float(1.0 / 3.0), "0.333333");
    assert_eq!(format_float(0.005), "5e-3");
    assert_eq!(format_float(2e6), "2e6");
    assert_eq!(gum_round(625.0, 40.0195264839553, None), ("625".to_string(), "40".to_string()));
    for (key, bad) in [
        ("display_significant_digits", "0"),
        ("display_significant_digits", "abc"),
        ("display_scientific_below", "1e-9"),
        ("display_scientific_from", "inf"),
        ("uncertainty_digits", "GUM"),
    ] {
        read(&format!("[Settings]\n{key} = {bad}\n"));
        assert_eq!(display_rules(), configured, "{key} = {bad:?} replaced the value in force");
    }
    read("[Settings]\ndisplay_significant_digits = 15\ndisplay_scientific_below = 1e-4\ndisplay_scientific_from = 1e16\nuncertainty_digits = gum\n");
    assert_eq!(display_rules(), DisplayRules::DEFAULT);
}

#[test]
fn a_scoped_guard_prints_by_the_compiled_rules_whatever_the_process_says() {
    let _restore = lock();
    set_display_rules(DisplayRules { significant_digits: 3, scientific_from: 10.0, uncertainty_digits: UncertaintyDigits::Figures(4), ..DisplayRules::DEFAULT });
    assert_eq!(format_float(1.0 / 3.0), "0.333");
    assert_eq!(format_float(12.0), "1.2e1");
    {
        let _pinned = scoped_display_rules(DisplayRules::DEFAULT);
        assert_eq!(format_float(1.0 / 3.0), "0.333333333333333");
        assert_eq!(format_float(12.0), "12.0");
        assert_eq!(gum_round(9.81, 0.05, None), ("9.81".to_string(), "0.05".to_string()));
    }
    assert_eq!(format_float(1.0 / 3.0), "0.333", "the guard ended");
}

#[test]
fn the_embedded_catalog_states_the_compiled_rules() {
    let _restore = lock();
    set_display_rules(DisplayRules { significant_digits: 4, ..DisplayRules::DEFAULT });
    read(physure_core::units::conf::DEFAULT_PHYSURE_CONF);
    assert_eq!(display_rules(), DisplayRules::DEFAULT);
}
