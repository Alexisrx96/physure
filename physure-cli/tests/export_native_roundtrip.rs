//! Slow (real `cargo build --release`) — run explicitly with:
//!   cargo test --test export_native_roundtrip -- --ignored --nocapture

use std::fs;
use std::process::Command;

const SCRIPT: &str = r#"
/// Computes the kinetic energy of a moving mass.
fn kinetic_energy(m, v) = 0.5 * m * v^2

export kinetic_energy as kinetic_energy
"#;

const CONTRACT_SCRIPT: &str = r#"
@requires(m > 0.0 kg, "m must be positive")
fn kinetic_energy(m, v) = 0.5 * m * v^2

export kinetic_energy as kinetic_energy
"#;

#[test]
#[ignore]
fn native_export_matches_interpreter_output() {
    let dir = std::env::temp_dir().join("phs_export_roundtrip_bare");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("ke.phs");
    fs::write(&script_path, SCRIPT).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_phs"))
        .args(["export", script_path.to_str().unwrap(), "--fn", "kinetic_energy", "--native", "-o"])
        .arg(&dir)
        .status()
        .unwrap();
    assert!(status.success());

    let lib_ext = if cfg!(target_os = "windows") { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
    let lib_path = dir.join(format!("kinetic_energy.{}", lib_ext));
    assert!(lib_path.exists(), "expected {} to exist", lib_path.display());

    let m = 2.0_f64;
    let v = 3.0_f64;
    let compiled_value = unsafe {
        let lib = libloading::Library::new(&lib_path).unwrap();
        let func: libloading::Symbol<unsafe extern "C" fn(f64, f64) -> f64> = lib.get(b"kinetic_energy").unwrap();
        func(m, v)
    };

    let interpreted = physure_script::eval_phs(&format!(
        "fn kinetic_energy(m, v) = 0.5 * m * v^2\nkinetic_energy({m} kg, {v} m/s)"
    ))
    .unwrap();
    let expected = match interpreted.last().unwrap() {
        physure_script::PhsValue::Quantity(q) => q.value.mean(),
        other => panic!("expected Quantity, got {:?}", other),
    };

    assert!(
        (compiled_value - expected).abs() < 1e-9,
        "compiled={} interpreted={}",
        compiled_value,
        expected
    );
}

#[test]
#[ignore]
fn native_export_contract_violation_matches_interpreter() {
    let dir = std::env::temp_dir().join("phs_export_roundtrip_contract");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("ke.phs");
    fs::write(&script_path, CONTRACT_SCRIPT).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_phs"))
        .args(["export", script_path.to_str().unwrap(), "--fn", "kinetic_energy", "--native", "-o"])
        .arg(&dir)
        .status()
        .unwrap();
    assert!(status.success());

    let lib_ext = if cfg!(target_os = "windows") { "dll" } else if cfg!(target_os = "macos") { "dylib" } else { "so" };
    let lib_path = dir.join(format!("kinetic_energy.{}", lib_ext));

    #[repr(C)]
    struct KineticEnergyResult {
        value: f64,
        ok: bool,
    }

    let (ok_valid, ok_invalid) = unsafe {
        let lib = libloading::Library::new(&lib_path).unwrap();
        let func: libloading::Symbol<unsafe extern "C" fn(f64, f64) -> KineticEnergyResult> =
            lib.get(b"kinetic_energy").unwrap();
        (func(2.0, 3.0).ok, func(-1.0, 3.0).ok)
    };

    assert!(ok_valid, "positive mass should satisfy @requires");
    assert!(!ok_invalid, "negative mass should violate @requires");

    let interpreter_rejects = physure_script::eval_phs(
        "@requires(m > 0.0 kg, \"m must be positive\")\nfn kinetic_energy(m, v) = 0.5 * m * v^2\nkinetic_energy(-1.0 kg, 3.0 m/s)",
    )
    .is_err();
    assert!(interpreter_rejects, "interpreter should also reject the negative-mass call");
}
