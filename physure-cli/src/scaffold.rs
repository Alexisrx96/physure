//! `phs new-plugin <name> [--lang rust|python|both] [--dir <path>]`
//!
//! Writes a working ABI-v2 native plugin and/or a `.py` ext template, plus a
//! matching `driver.py` (and build scripts for the Rust case) into `<dir>`.
//! Templates are static strings with a single `__NAME__` token substituted —
//! deliberately not `format!`, since the Rust template's own braces would
//! otherwise need escaping throughout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use crate::get_flag_value;

pub fn run_new_plugin(args: &[String]) {
    let name = match args.get(2) {
        Some(n) if !n.starts_with('-') => n.clone(),
        _ => {
            eprintln!("Usage: phs new-plugin <name> [--lang rust|python|both] [--dir <path>]");
            process::exit(1);
        }
    };

    let lang = get_flag_value(args, "--lang").unwrap_or_else(|| "rust".to_string());
    let dir = PathBuf::from(get_flag_value(args, "--dir").unwrap_or_else(|| ".".to_string()));

    match lang.as_str() {
        "rust" => {
            write_rust_files(&dir, &name);
            write_file(&dir.join("driver.py"), &DRIVER_RUST_TEMPLATE.replace("__NAME__", &name));
            println!("\nnext steps:");
            println!("  1. ./build_plugin.sh   (or build_plugin.ps1 / build_plugin.cmd on Windows)");
            println!("  2. phs some_script.phs   # calls {name}_double(...) / {name}_shout(...) directly");
            println!("  3. python driver.py     # same calls, plus a live hot-reload demo");
        }
        "python" | "py" => {
            write_python_file(&dir, &name);
            write_file(&dir.join("driver.py"), &DRIVER_PY_TEMPLATE.replace("__NAME__", &name));
            println!("\nnext steps:");
            println!("  python driver.py   # loads ext/{name}.py and calls {name}_hello(...)");
        }
        "both" => {
            write_rust_files(&dir, &name);
            write_python_file(&dir, &name);
            write_file(&dir.join("driver.py"), &DRIVER_BOTH_TEMPLATE.replace("__NAME__", &name));
            println!("\nnext steps:");
            println!("  1. ./build_plugin.sh   (or build_plugin.ps1 / build_plugin.cmd on Windows)");
            println!("  2. python driver.py   # runs both the native plugin and the .py ext, plus hot-reload");
        }
        other => {
            eprintln!("error: unknown --lang '{}', expected rust, python, or both", other);
            process::exit(1);
        }
    }
}

fn write_rust_files(dir: &Path, name: &str) {
    write_file(&dir.join("ext").join(format!("{name}.rs")), &RUST_PLUGIN_TEMPLATE.replace("__NAME__", name));
    write_file(&dir.join("build_plugin.sh"), &BUILD_SH_TEMPLATE.replace("__NAME__", name));
    write_file(&dir.join("build_plugin.ps1"), &BUILD_PS1_TEMPLATE.replace("__NAME__", name));
    write_file(&dir.join("build_plugin.cmd"), &BUILD_CMD_TEMPLATE.replace("__NAME__", name));
    make_executable(&dir.join("build_plugin.sh"));
}

fn write_python_file(dir: &Path, name: &str) {
    write_file(&dir.join("ext").join(format!("{name}.py")), &PY_EXT_TEMPLATE.replace("__NAME__", name));
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("error creating directory '{}': {}", parent.display(), e);
            process::exit(1);
        }
    }
    if let Err(e) = fs::write(path, contents) {
        eprintln!("error writing '{}': {}", path.display(), e);
        process::exit(1);
    }
    println!("wrote {}", path.display());
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

const RUST_PLUGIN_TEMPLATE: &str = r#"// Native PhysureScript plugin (ABI v2), scaffolded by `phs new-plugin __NAME__ --lang rust`.
// Build with build_plugin.sh (Linux/macOS) or build_plugin.ps1 / build_plugin.cmd (Windows)
// into ext/__NAME__.<so|dylib|dll> — phs, and any Interpreter(base_dir=...), auto-load every
// ext/*.<so|dylib|dll> next to the script.
//
// These ABI types aren't published as a crate; copy them verbatim into every plugin (see
// physure-script/src/plugin.rs for the authoritative definition this must match).

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum PluginValueTag {
    None = 0,
    Number = 1,
    Bool = 2,
    Quantity = 3,
    String = 4,
    Vector = 5,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginValue {
    pub tag: PluginValueTag,
    pub number: f64,
    pub text: *const std::os::raw::c_char,
    pub items: *const PluginValue,
    pub item_count: usize,
}

const NONE: PluginValue = PluginValue {
    tag: PluginValueTag::None,
    number: 0.0,
    text: std::ptr::null(),
    items: std::ptr::null(),
    item_count: 0,
};

#[repr(C)]
pub struct PluginFnEntry {
    pub name: *const std::os::raw::c_char,
    pub func: extern "C" fn(*const PluginValue, usize) -> PluginValue,
}

#[repr(C)]
pub struct PluginRegistry {
    pub abi_version: u32,
    pub entries: *const PluginFnEntry,
    pub entry_count: usize,
}

/// Number -> Number. Replace with your own logic.
extern "C" fn __NAME___double(args: *const PluginValue, len: usize) -> PluginValue {
    let args = unsafe { std::slice::from_raw_parts(args, len) };
    PluginValue { tag: PluginValueTag::Number, number: args[0].number * 2.0, ..NONE }
}

/// Quantity -> Bool. Quantities carry their magnitude in `.number` too.
extern "C" fn __NAME___is_positive(args: *const PluginValue, len: usize) -> PluginValue {
    let args = unsafe { std::slice::from_raw_parts(args, len) };
    PluginValue { tag: PluginValueTag::Bool, number: if args[0].number > 0.0 { 1.0 } else { 0.0 }, ..NONE }
}

/// Quantity -> Quantity. `.text` carries the unit string; round-trip it to keep the unit.
extern "C" fn __NAME___add_one(args: *const PluginValue, len: usize) -> PluginValue {
    let args = unsafe { std::slice::from_raw_parts(args, len) };
    PluginValue { tag: PluginValueTag::Quantity, number: args[0].number + 1.0, text: args[0].text, ..NONE }
}

/// String -> String.
extern "C" fn __NAME___shout(args: *const PluginValue, len: usize) -> PluginValue {
    let args = unsafe { std::slice::from_raw_parts(args, len) };
    let s = unsafe { std::ffi::CStr::from_ptr(args[0].text) }.to_str().unwrap();
    let upper = std::ffi::CString::new(format!("{}!", s.to_uppercase())).unwrap();
    PluginValue { tag: PluginValueTag::String, text: upper.into_raw(), ..NONE }
}

/// Vector<Number> -> Number.
extern "C" fn __NAME___sum(args: *const PluginValue, len: usize) -> PluginValue {
    let args = unsafe { std::slice::from_raw_parts(args, len) };
    let items = unsafe { std::slice::from_raw_parts(args[0].items, args[0].item_count) };
    let total: f64 = items.iter().map(|v| v.number).sum();
    PluginValue { tag: PluginValueTag::Number, number: total, ..NONE }
}

#[no_mangle]
pub extern "C" fn phs_plugin_entry() -> PluginRegistry {
    let entries = vec![
        PluginFnEntry { name: std::ffi::CString::new("__NAME___double").unwrap().into_raw(), func: __NAME___double },
        PluginFnEntry { name: std::ffi::CString::new("__NAME___is_positive").unwrap().into_raw(), func: __NAME___is_positive },
        PluginFnEntry { name: std::ffi::CString::new("__NAME___add_one").unwrap().into_raw(), func: __NAME___add_one },
        PluginFnEntry { name: std::ffi::CString::new("__NAME___shout").unwrap().into_raw(), func: __NAME___shout },
        PluginFnEntry { name: std::ffi::CString::new("__NAME___sum").unwrap().into_raw(), func: __NAME___sum },
    ];
    let entries: &'static [PluginFnEntry] = Box::leak(entries.into_boxed_slice());
    PluginRegistry { abi_version: 2, entries: entries.as_ptr(), entry_count: entries.len() }
}
"#;

const BUILD_SH_TEMPLATE: &str = r#"#!/usr/bin/env bash
# Compiles ext/__NAME__.rs into the native plugin phs and Interpreter(base_dir=...)
# auto-discover in ext/*.<so|dylib|dll>.
set -euo pipefail
cd "$(dirname "$0")"

case "$(uname -s)" in
    Darwin*) EXT=dylib ;;
    MINGW*|MSYS*|CYGWIN*) EXT=dll ;;
    *) EXT=so ;;
esac

rustc --edition 2021 --crate-type cdylib -o "ext/__NAME__.$EXT" ext/__NAME__.rs
echo "built ext/__NAME__.$EXT"
"#;

const BUILD_PS1_TEMPLATE: &str = r#"# Compiles ext/__NAME__.rs into ext/__NAME__.dll for Windows.
Set-Location $PSScriptRoot
rustc --edition 2021 --crate-type cdylib -o "ext\__NAME__.dll" "ext\__NAME__.rs"
Write-Host "built ext\__NAME__.dll"
"#;

const BUILD_CMD_TEMPLATE: &str = r#"@echo off
rem Compiles ext\__NAME__.rs into ext\__NAME__.dll for Windows (cmd.exe).
cd /d "%~dp0"
rustc --edition 2021 --crate-type cdylib -o "ext\__NAME__.dll" "ext\__NAME__.rs"
echo built ext\__NAME__.dll
"#;

const PY_EXT_TEMPLATE: &str = r#""""Ext function scaffolded by `phs new-plugin __NAME__ --lang python`.

Every top-level function defined here is registered into the PHS interpreter
by physure.ext.phs_loader.load_ext_functions, callable from .phs source under
its own name. Bare numeric PHS literals (e.g. `8`) arrive as a dimensionless
physure Quantity, not a plain float/int — use `.magnitude` to unwrap one.
"""


def __NAME___hello(nombre: str) -> str:
    return f"Hola, {nombre}, desde __NAME__.py"
"#;

const DRIVER_RUST_TEMPLATE: &str = r#""""Loads ext/__NAME__.<so|dylib|dll> and hot-reloads it without restarting.

Uso: python driver.py   (corre build_plugin.[sh|ps1|cmd] al menos una vez antes)
"""

from pathlib import Path
import subprocess
import sys

from physure import Interpreter

HERE = Path(__file__).parent
PLUGIN_SRC = HERE / "ext" / "__NAME__.rs"


def build() -> None:
    if sys.platform == "win32":
        subprocess.run(["powershell", "-File", "build_plugin.ps1"], cwd=HERE, check=True)
    else:
        subprocess.run(["./build_plugin.sh"], cwd=HERE, check=True)


def main() -> None:
    build()
    interp = Interpreter(base_dir=str(HERE))
    print("__NAME___double(21) =", interp.evaluate("__NAME___double(21)")[0])
    print('__NAME___shout("hola") =', interp.evaluate('__NAME___shout("hola")')[0])

    print("\n--- hot reload: __NAME___double x2 -> x10, sin reiniciar ---")
    original = PLUGIN_SRC.read_text()
    PLUGIN_SRC.write_text(original.replace("args[0].number * 2.0", "args[0].number * 10.0"))
    try:
        build()
        reloaded = interp.reload_native_ext()
        print("funciones recargadas:", reloaded)
        print("__NAME___double(21) =", interp.evaluate("__NAME___double(21)")[0])
    finally:
        PLUGIN_SRC.write_text(original)
        build()


if __name__ == "__main__":
    main()
"#;

const DRIVER_PY_TEMPLATE: &str = r#""""Loads ext/__NAME__.py functions into a PHS interpreter.

Uso: python driver.py
"""

from pathlib import Path

from physure import Interpreter
from physure.ext.phs_loader import load_ext_functions

HERE = Path(__file__).parent


def main() -> None:
    interp = Interpreter(base_dir=str(HERE))
    registered = load_ext_functions(interp, HERE)
    print("funciones .py registradas:", registered)
    print('__NAME___hello("mundo") =', interp.evaluate('__NAME___hello("mundo")')[0])


if __name__ == "__main__":
    main()
"#;

const DRIVER_BOTH_TEMPLATE: &str = r#""""Runs the native plugin (ext/__NAME__.rs) and the .py ext (ext/__NAME__.py),
then hot-reloads the native plugin without restarting the process.

Uso: python driver.py   (corre build_plugin.[sh|ps1|cmd] al menos una vez antes)
"""

from pathlib import Path
import subprocess
import sys

from physure import Interpreter
from physure.ext.phs_loader import load_ext_functions

HERE = Path(__file__).parent
PLUGIN_SRC = HERE / "ext" / "__NAME__.rs"


def build() -> None:
    if sys.platform == "win32":
        subprocess.run(["powershell", "-File", "build_plugin.ps1"], cwd=HERE, check=True)
    else:
        subprocess.run(["./build_plugin.sh"], cwd=HERE, check=True)


def main() -> None:
    build()
    interp = Interpreter(base_dir=str(HERE))
    registered = load_ext_functions(interp, HERE)
    print("funciones .py registradas:", registered)

    print("__NAME___double(21) =", interp.evaluate("__NAME___double(21)")[0])
    print('__NAME___shout("hola") =', interp.evaluate('__NAME___shout("hola")')[0])
    print('__NAME___hello("mundo") =', interp.evaluate('__NAME___hello("mundo")')[0])

    print("\n--- hot reload: __NAME___double x2 -> x10, sin reiniciar ---")
    original = PLUGIN_SRC.read_text()
    PLUGIN_SRC.write_text(original.replace("args[0].number * 2.0", "args[0].number * 10.0"))
    try:
        build()
        reloaded = interp.reload_native_ext()
        print("funciones recargadas:", reloaded)
        print("__NAME___double(21) =", interp.evaluate("__NAME___double(21)")[0])
    finally:
        PLUGIN_SRC.write_text(original)
        build()


if __name__ == "__main__":
    main()
"#;
