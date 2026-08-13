//! `phs export <script.phs> --fn <name> [--native] [-o <dir>]`
//!
//! Always writes `<fn>.proto` and `<fn>.md` for the named, already-`export`ed function.
//! `--native` additionally scaffolds a throwaway `cdylib` crate wrapping the compiled FFI shim,
//! builds it with `cargo build --release`, and copies the resulting `.dll`/`.so`/`.dylib` next
//! to the `.proto`/`.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use physure_script::ast::FunctionDefNode;
use physure_script::codegen::md::MdGenerator;
use physure_script::codegen::proto::ProtoGenerator;
use physure_script::codegen::rust::RustTranspiler;
use physure_script::codegen::CodeGenerator;
use physure_script::{parse_phs, Program, Statement};

use crate::get_flag_value;

/// Baked in at `phs`'s own compile time — the same relationship this crate's own `Cargo.toml`
/// already has to `physure-core` (`path = "../physure-core", package = "physure"`), just
/// available at runtime so a scaffolded crate anywhere on disk can resolve back to the exact
/// `physure-core` this binary was built from. No publishing, no vendoring, no drift from the
/// single source of truth for unit logic.
const PHYSURE_CORE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../physure-core");

pub fn run_export(args: &[String]) {
    let script_path = match args.get(2) {
        Some(p) if !p.starts_with('-') => p.clone(),
        _ => {
            eprintln!("Usage: phs export <script.phs> --fn <name> [--native] [-o <dir>]");
            process::exit(1);
        }
    };
    let fn_name = match get_flag_value(args, "--fn") {
        Some(n) => n,
        None => {
            eprintln!("error: --fn <name> is required");
            process::exit(1);
        }
    };
    let is_native = args.iter().any(|a| a == "--native");

    let code = match fs::read_to_string(&script_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not read '{}': {}", script_path, e);
            process::exit(1);
        }
    };
    let program = match parse_phs(&code) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: parse failed: {}", e);
            process::exit(1);
        }
    };

    let node = match find_function(&program, &fn_name) {
        Some(n) => n.clone(),
        None => {
            eprintln!("error: no function named '{}' in '{}'", fn_name, script_path);
            process::exit(1);
        }
    };
    if !is_exported(&program, &fn_name) {
        eprintln!("error: '{}' exists but was never `export`ed; add `export {}`", fn_name, fn_name);
        process::exit(1);
    }

    let out_dir = PathBuf::from(get_flag_value(args, "-o").or_else(|| get_flag_value(args, "--output")).unwrap_or_else(|| {
        Path::new(&script_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ".".to_string())
    }));
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("error creating output directory '{}': {}", out_dir.display(), e);
        process::exit(1);
    }

    let single_fn_program = Program { statements: vec![Statement::FunctionDef(node.clone())] };

    let proto = match ProtoGenerator.generate_program(&single_fn_program) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error generating .proto: {}", e);
            process::exit(1);
        }
    };
    write_output(&out_dir.join(format!("{}.proto", fn_name)), &proto);

    let md = match MdGenerator.generate_program(&single_fn_program) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error generating .md: {}", e);
            process::exit(1);
        }
    };
    write_output(&out_dir.join(format!("{}.md", fn_name)), &md);

    if is_native {
        build_native(&out_dir, &fn_name, &node);
    }
}

fn find_function<'a>(program: &'a Program, name: &str) -> Option<&'a FunctionDefNode> {
    program.statements.iter().find_map(|s| match s {
        Statement::FunctionDef(f) if f.name == name => Some(f),
        _ => None,
    })
}

fn is_exported(program: &Program, name: &str) -> bool {
    program.statements.iter().any(|s| matches!(s, Statement::Export(e) if e.symbol == name))
}

fn write_output(path: &Path, contents: &str) {
    if let Err(e) = fs::write(path, contents) {
        eprintln!("error writing '{}': {}", path.display(), e);
        process::exit(1);
    }
    println!("wrote {}", path.display());
}

fn build_native(out_dir: &Path, fn_name: &str, node: &FunctionDefNode) {
    let shim = match RustTranspiler.generate_export_shim(node) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error generating native shim: {}", e);
            process::exit(1);
        }
    };
    let crate_name = format!("{}_export", fn_name);
    let crate_dir = out_dir.join(&crate_name);
    let src_dir = crate_dir.join("src");
    if let Err(e) = fs::create_dir_all(&src_dir) {
        eprintln!("error creating '{}': {}", src_dir.display(), e);
        process::exit(1);
    }

    let lib_rs = format!("// Generated by PhysureScript (PHS) Compiler\nuse physure_core::Quantity;\n\n{}", shim);
    write_output(&src_dir.join("lib.rs"), &lib_rs);

    let cargo_toml = format!(
        "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\nname = \"{crate_name}\"\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nphysure-core = {{ path = '{core_path}', package = \"physure\" }}\n",
        crate_name = crate_name,
        core_path = PHYSURE_CORE_PATH,
    );
    write_output(&crate_dir.join("Cargo.toml"), &cargo_toml);

    println!("running cargo build --release in {}...", crate_dir.display());
    let output = Command::new("cargo").args(["build", "--release"]).current_dir(&crate_dir).output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error running cargo: {}", e);
            process::exit(1);
        }
    };
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        process::exit(1);
    }

    let built_name = if cfg!(target_os = "windows") {
        format!("{}.dll", crate_name)
    } else if cfg!(target_os = "macos") {
        format!("lib{}.dylib", crate_name)
    } else {
        format!("lib{}.so", crate_name)
    };
    let dest_ext = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let built = crate_dir.join("target").join("release").join(&built_name);
    let dest = out_dir.join(format!("{}.{}", fn_name, dest_ext));
    if let Err(e) = fs::copy(&built, &dest) {
        eprintln!("error copying built library from '{}' to '{}': {}", built.display(), dest.display(), e);
        process::exit(1);
    }
    println!("wrote {}", dest.display());
}
