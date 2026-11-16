//! Build-time facts for `frozen::engine`. Everything is emitted under an internal name, always, so that the
//! library never reads a name a user can set (`option_env!` would carry a rejected value into the binary).

use std::env;
use std::process::Command;

#[allow(dead_code)]
#[path = "src/frozen/sha256.rs"]
mod sha256;
#[allow(dead_code)]
#[path = "build_support/commit.rs"]
mod commit_rules;

fn main() {
    watch();

    // two builds whose facts were all "unknown" would compare equal under `build`: a fact that cannot be
    // determined stops the build instead
    println!("cargo:rustc-env=PHYSURE_ENGINE_PROFILE={}", required("PROFILE"));
    println!("cargo:rustc-env=PHYSURE_ENGINE_TARGET={}", required("TARGET"));
    println!("cargo:rustc-env=PHYSURE_ENGINE_RUSTC={}", rustc_version());
    println!("cargo:rustc-env=PHYSURE_ENGINE_FLAGS={}", flags());

    let decision = commit_rules::decide(env::var("PHYSURE_BUILD_COMMIT").ok().as_deref());
    if let Some(warning) = &decision.warning {
        println!("cargo:warning={warning}");
    }
    println!("cargo:rustc-env=PHYSURE_ENGINE_COMMIT={}", decision.commit);
}

fn required(name: &str) -> String {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!("physure build script: cargo did not set {name}; the engine identity cannot be determined"),
    }
}

/// What can change the answer: the three variables and this script. (Nothing else is read: the commit is the
/// builder's claim, not something looked up in the tree.)
fn watch() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_support");
    println!("cargo:rerun-if-env-changed=PHYSURE_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=RUSTC");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
}

fn rustc_version() -> String {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    Command::new(&rustc)
        .arg("-V")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.lines().next().unwrap_or("").trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| panic!("physure build script: `{rustc} -V` gave no version; the engine identity cannot be determined"))
}

fn flags() -> String {
    let opt = required("OPT_LEVEL");
    let assertions = if env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some() { "on" } else { "off" };
    let panic = required("CARGO_CFG_PANIC");
    let rustflags = sha256::sha256_hex(env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default().as_bytes());
    let features = sha256::sha256_hex(env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default().as_bytes());
    format!("opt-level={opt};debug-assertions={assertions};panic={panic};rustflags={rustflags};target-features={features}")
}
