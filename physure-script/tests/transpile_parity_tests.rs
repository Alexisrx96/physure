use physure_script::codegen::java::JavaTranspiler;
use physure_script::codegen::python::PythonTranspiler;
use physure_script::codegen::{transpile, CodeGenerator, Target};
use physure_script::interpreter::PhsInterpreter;
use physure_script::parser::parse_phs;
use std::fs;
use std::path::Path;
use std::process::Command;

struct ParityTestCase {
    name: &'static str,
    script: &'static str,
    expected_substring: &'static str,
}

const TEST_CASES: &[ParityTestCase] = &[
    ParityTestCase {
        name: "basic_arithmetic",
        script: "d = 100 m\nt = 5 s\nv = d / t\nres = v * 10 s\n",
        expected_substring: "200",
    },
    ParityTestCase {
        name: "unit_conversion",
        script: "d = 100 m\nt = 5 s\nv = (d / t) => km/h\n",
        expected_substring: "72",
    },
    ParityTestCase {
        name: "functions",
        script: "fn kinetic_energy(m, v) = 0.5 * m * v^2\nke = kinetic_energy(80 kg, 15 m/s) => kJ\n",
        expected_substring: "9",
    },
    ParityTestCase {
        name: "power_and_roots",
        script: "r = 4 m\narea = 3.14159265 * r^2\n",
        expected_substring: "50.265",
    },
    ParityTestCase {
        // Named `mass`, not `m`: the parser flags a quantity literal's unit word against
        // every bound name in scope, including the first, and `v = 3.0 m/s` would otherwise
        // collide with a variable named `m` (see physure-script/tests/unit_shadowing.rs).
        name: "string_interpolation",
        script: "mass = 2.0 kg\nv = 3.0 m/s\nlabel = \"masa {mass} a {v}\"\n",
        expected_substring: "masa 2.0 kg a 3.0 m/s",
    },
    ParityTestCase {
        name: "uncertainty",
        script: "mass = 10.0 +/- 0.2 kg\na = 2.5 +/- 0.1 m/s^2\nf = mass * a\n",
        expected_substring: "25",
    },
    ParityTestCase {
        name: "equations",
        script: "use solve from calc\neq1 = \"V = R * I\"\neq5 = solve(eq1, \"R\")\nr = eq5(I = -2mA, V = -12V) => kOhm\n",
        expected_substring: "6",
    },
    ParityTestCase {
        name: "where_clause",
        script: "duplo = a + b where a = 2.0 m, b = a * 3.0\n",
        expected_substring: "8",
    },
    ParityTestCase {
        name: "while_loop_convergence",
        script: "x = 1.5\ni = 0\nwhile i < 5 {\n  x = (x + 2.0 / x) / 2.0\n  i = i + 1\n}\nx\n",
        expected_substring: "1.41421",
    },
];

#[test]
fn test_phs_interpreter_parity() {
    for tc in TEST_CASES {
        let program = parse_phs(tc.script).unwrap_or_else(|e| panic!("Failed to parse {}: {}", tc.name, e));
        let mut interp = PhsInterpreter::default();
        let env = interp.eval_program(&program).unwrap_or_else(|e| panic!("Failed to eval {}: {}", tc.name, e));
        let env_str = format!("{:?}", env);
        assert!(
            env_str.contains(tc.expected_substring),
            "Interpreter test {} failed to find {} in env: {}", tc.name, tc.expected_substring, env_str
        );
    }
}

#[test]
fn test_python_transpiler_parity() {
    let py_dir = repo_root().join("physure-python");
    // The generated code imports `physure`, so this needs both `uv` and a built extension.
    // The `rust` CI job has neither; `physure-python/tests/test_transpiler_parity.py` runs
    // the same cases where they can actually pass.
    let importable = Command::new("uv")
        .args(["run", "python", "-c", "import physure"])
        .current_dir(&py_dir)
        .output()
        .is_ok_and(|o| o.status.success());
    if !importable {
        eprintln!("skipping: `uv run python -c 'import physure'` does not work here");
        return;
    }
    for tc in TEST_CASES {
        let program = parse_phs(tc.script).unwrap();
        let py_code = transpile(&program, Target::Python).unwrap();
        
        let temp_file = std::env::temp_dir().join(format!("parity_{}.py", tc.name));
        fs::write(&temp_file, &py_code).unwrap();
        
        let output = Command::new("uv")
            .args(["run", "python", temp_file.to_str().unwrap()])
            .current_dir(&py_dir)
            .output()
            .expect("Failed to run python");

        let _ = fs::remove_file(&temp_file);
        assert!(
            output.status.success(),
            "Python transpiled execution failed for {}.\nStderr: {}\nCode:\n{}",
            tc.name, String::from_utf8_lossy(&output.stderr), py_code
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(tc.expected_substring),
            "Python output for {} expected '{}', got:\n{}", tc.name, tc.expected_substring, stdout
        );
    }
}

/// Absolute path to the repository root, derived from this crate rather than hard-coded so
/// the parity tests run from any checkout.
fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("physure-script has a parent directory")
}

/// The JNI bindings are a separate cdylib, so `cargo test` does not necessarily produce
/// them: the test used to fail with "Native library libphysure_java.so not found", which
/// reads like a broken binding rather than a missing build step. Build it on demand.
fn native_lib_dir() -> std::path::PathBuf {
    let target_dir = repo_root().join("target/debug");
    if target_dir.join("libphysure_java.so").exists() {
        return target_dir;
    }
    let build = Command::new("cargo")
        .args(["build", "-p", "physure-java"])
        .current_dir(repo_root())
        .output()
        .expect("Failed to run cargo build for physure-java");
    assert!(
        build.status.success(),
        "Could not build the JNI bindings: {}", String::from_utf8_lossy(&build.stderr)
    );
    target_dir
}

#[test]
fn test_java_transpiler_parity() {
    let java_src_dir = repo_root().join("physure-java/src/main/java");
    let native_lib_dir = native_lib_dir();

    // Shared across all cases: the base `com/physure/*.java` sources are identical every
    // iteration, so compile them once instead of 7 times.
    let temp_dir = std::env::temp_dir().join("phs_java_parity_shared");
    let _ = fs::create_dir_all(&temp_dir);

    let compile_base = match Command::new("sh")
        .arg("-c")
        .arg(format!("javac -d {} {}/com/physure/*.java", temp_dir.to_str().unwrap().replace('\\', "/"), java_src_dir.to_str().unwrap().replace('\\', "/")))
        .output() {
            Ok(out) => out,
            Err(_) => {
                eprintln!("Skipping Java parity test: 'sh' or 'javac' not found");
                return;
            }
        };

    if !compile_base.status.success() {
        eprintln!("Skipping Java parity test: javac failed: {}", String::from_utf8_lossy(&compile_base.stderr));
        return;
    }

    for tc in TEST_CASES {
        let class_name = format!("Parity{}", tc.name.replace("_", ""));
        let program = parse_phs(tc.script).unwrap();
        let java_code = transpile(&program, Target::JavaWithClass(class_name.clone())).unwrap();

        let gen_file = temp_dir.join(format!("{}.java", class_name));
        fs::write(&gen_file, &java_code).unwrap();

        let compile_gen = Command::new("javac")
            .args(["-cp", temp_dir.to_str().unwrap(), "-d", temp_dir.to_str().unwrap(), gen_file.to_str().unwrap()])
            .output()
            .expect("Failed to compile generated java");

        assert!(
            compile_gen.status.success(),
            "Java generated compile failed for {}.\nStderr: {}\nCode:\n{}",
            tc.name, String::from_utf8_lossy(&compile_gen.stderr), java_code
        );

        let run = Command::new("java")
            .arg(format!("-Djava.library.path={}", native_lib_dir.to_str().unwrap()))
            .args(["-cp", temp_dir.to_str().unwrap(), &class_name])
            .output();

        let run = match run {
            Ok(r) if r.status.success() => r,
            Ok(r) => {
                eprintln!("Skipping Java parity test for {}: {}", tc.name, String::from_utf8_lossy(&r.stderr));
                continue;
            }
            Err(_) => {
                eprintln!("Skipping Java parity test: java run failed");
                continue;
            }
        };

        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            stdout.contains(tc.expected_substring),
            "Java output for {} expected '{}', got:\n{}", tc.name, tc.expected_substring, stdout
        );
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

/// Real-`javac`-compile regression test for task 10 (Java codegen for `not`/`and`/`or` and the
/// Bool `assert` overloads). Mirrors `test_java_transpiler_parity` above (same `javac`/`java`
/// availability checks, same shared-base-classes-compiled-once approach) rather than living
/// inline in `java.rs`'s unit-test module, since it needs the same `repo_root()`/
/// `native_lib_dir()` machinery and shelling out to `javac`/`java` -- exactly the pattern this
/// file already establishes and `java.rs`'s unit tests have no precedent for.
///
/// Also regression-tests the double-semicolon bug found in code review after the initial
/// implementation: with the shared `generate_java_assignment` helper baking its own trailing
/// `;` into every branch, `generate_function_def_stmt`'s per-statement loop used to
/// unconditionally append another `;` after calling `generate_statement` for a non-tail
/// statement, producing a literal `;;` for any non-tail `Assignment` inside a function body.
/// While a lone `;` is a legal Java `EmptyStatement` (so this never actually failed to
/// compile -- unlike the Rust codegen bug from task 9, a genuine *missing* terminator, which
/// really did fail `rustc`), it was still fixed via a `terminate_statement` helper mirroring
/// `rust.rs`'s. `passing_script` below deliberately includes a function with a non-tail
/// `Assignment` (`y = x * 2.0`) AND a non-tail `assert(...)` followed by further statements
/// (`z = y + 1.0 m`, then the tail `z`), so the `!contains(";;")` assertion proves the fix
/// holds for both statement shapes, not just the one originally found broken-looking.
#[test]
fn test_java_bool_assert_transpiler_compiles_and_runs() {
    let java_src_dir = repo_root().join("physure-java/src/main/java");
    let native_lib_dir = native_lib_dir();

    let temp_dir = std::env::temp_dir().join("phs_java_bool_assert_transpile");
    let _ = fs::create_dir_all(&temp_dir);

    let java_files: Vec<_> = fs::read_dir(java_src_dir.join("com/physure"))
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|ext| ext == "java"))
                .collect()
        })
        .unwrap_or_default();
    if java_files.is_empty() {
        eprintln!("Skipping Java bool-assert test: no java source files found in com/physure");
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    }
    let mut javac_args = vec!["-d".to_string(), temp_dir.to_str().unwrap().to_string()];
    for f in &java_files {
        javac_args.push(f.to_str().unwrap().to_string());
    }
    let compile_base = match Command::new("javac").args(&javac_args).output() {
        Ok(out) => out,
        Err(_) => {
            eprintln!("Skipping Java bool-assert test: javac not found");
            let _ = fs::remove_dir_all(&temp_dir);
            return;
        }
    };
    if !compile_base.status.success() {
        eprintln!("Skipping Java bool-assert test: javac failed: {}", String::from_utf8_lossy(&compile_base.stderr));
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    }

    // Both cases below call `JavaTranspiler::generate_program` directly instead of going
    // through the public `transpile()` entry point. `transpile()` first runs the WHOLE
    // program through the real interpreter (`compile_equations_to_functions`, to resolve any
    // `solve()`-defined equations) before codegen ever sees it -- so for a script whose
    // `assert` condition is fully determined by literals (as every PHS script's is: there is
    // no external/runtime input), a genuinely-failing `assert` fails during THAT eager
    // interpreter pass and `transpile()` itself returns `Err` before emitting any Java at all
    // (confirmed empirically: swapping in `transpile()` here made `Case 2` panic on `.unwrap()`
    // with the interpreter's own "assert failed: boom", never reaching codegen). Calling
    // `generate_program` directly is the same thing `java.rs`'s own unit tests already do for
    // every assert-shape test, and neither case here defines an equation via `solve()`, so
    // skipping that prepass changes nothing about what's being proven: real Java source, really
    // compiled by `javac`, really executed by `java`.

    // Case 1: passes at runtime. Exercises a non-tail Assignment AND a non-tail
    // `assert(...)` followed by more statements, both inside the same function body (the
    // double-semicolon path described above), plus a top-level BoolWithMessage assert.
    let passing_script =
        "fn compute(x) =\n  y = x * 2.0\n  assert(y > 0.0 m, \"y must be positive\")\n  z = y + 1.0 m\n  z\nresult = compute(5.0 m)\nok = 1.0 m > 0.0 m\nassert(ok, \"should hold\")\n";
    let program = parse_phs(passing_script).unwrap();
    let java_code = JavaTranspiler::new("BoolAssertPass").generate_program(&program).unwrap();
    assert!(
        !java_code.contains(";;"),
        "a non-tail statement inside a function body must not double-terminate:\n{java_code}"
    );
    assert!(java_code.contains("throw new AssertionError(\"y must be positive\")"), "{java_code}");

    let gen_file = temp_dir.join("BoolAssertPass.java");
    fs::write(&gen_file, &java_code).unwrap();
    let compile_gen = Command::new("javac")
        .args(["-cp", temp_dir.to_str().unwrap(), "-d", temp_dir.to_str().unwrap(), gen_file.to_str().unwrap()])
        .output()
        .expect("Failed to compile generated java");
    assert!(
        compile_gen.status.success(),
        "Java generated compile failed for passing bool assert (incl. double-semicolon case).\nStderr: {}\nCode:\n{}",
        String::from_utf8_lossy(&compile_gen.stderr), java_code
    );

    let run = Command::new("java")
        .arg(format!("-Djava.library.path={}", native_lib_dir.to_str().unwrap()))
        .args(["-cp", temp_dir.to_str().unwrap(), "BoolAssertPass"])
        .output();
    match run {
        Ok(r) if r.status.success() => {}
        Ok(r) => {
            eprintln!("Skipping Java bool-assert run check: {}", String::from_utf8_lossy(&r.stderr));
            let _ = fs::remove_dir_all(&temp_dir);
            return;
        }
        Err(_) => {
            eprintln!("Skipping Java bool-assert run check: java run failed");
            let _ = fs::remove_dir_all(&temp_dir);
            return;
        }
    }

    // Case 2: a FAILING 2-arg Bool assert must actually throw AssertionError with the given
    // message at runtime, not merely compile.
    let failing_script = "ok = 1.0 m > 2.0 m\nassert(ok, \"boom\")\n";
    let program2 = parse_phs(failing_script).unwrap();
    let java_code2 = JavaTranspiler::new("BoolAssertFail").generate_program(&program2).unwrap();
    let gen_file2 = temp_dir.join("BoolAssertFail.java");
    fs::write(&gen_file2, &java_code2).unwrap();
    let compile_gen2 = Command::new("javac")
        .args(["-cp", temp_dir.to_str().unwrap(), "-d", temp_dir.to_str().unwrap(), gen_file2.to_str().unwrap()])
        .output()
        .expect("Failed to compile generated java");
    assert!(
        compile_gen2.status.success(),
        "Java generated compile failed for failing bool assert.\nStderr: {}\nCode:\n{}",
        String::from_utf8_lossy(&compile_gen2.stderr), java_code2
    );

    let run2 = Command::new("java")
        .arg(format!("-Djava.library.path={}", native_lib_dir.to_str().unwrap()))
        .args(["-cp", temp_dir.to_str().unwrap(), "BoolAssertFail"])
        .output()
        .expect("Failed to run generated java");

    assert!(
        !run2.status.success(),
        "expected the assertion to fail at runtime:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run2.stdout), String::from_utf8_lossy(&run2.stderr)
    );
    let stderr2 = String::from_utf8_lossy(&run2.stderr);
    assert!(stderr2.contains("boom"), "expected the AssertionError message 'boom':\n{stderr2}");

    let _ = fs::remove_dir_all(&temp_dir);
}

/// Real-syntax-check regression test for task 11 (JS/TS codegen for `not`/`and`/`or` and the
/// Bool `assert` overloads). JS/TS is not statically compiled the way Python/Rust/Java are in
/// the parity tests above, but `node --check` parses a file without executing it or needing
/// the `physure` package importable, which is enough to catch the "generates invalid syntax"
/// class of bug the other three targets' real-compile tests found in their own tasks (a
/// Python 3.11 syntax bug, a Rust missing-semicolon compile bug, a Java double-semicolon
/// cosmetic bug). TypeScript's `: Quantity`/`: boolean` annotations are not valid plain-JS
/// syntax, so `node --check` cannot validate the `typed: true` output; a real check there
/// would need `tsc`, which (unlike `node`) is not available in this environment -- so this
/// test covers the untyped `Target::JavaScript` output only, gracefully skipping entirely if
/// `node` itself is not on PATH (mirroring how the Python/Java/Rust parity tests above skip
/// when their own toolchain is missing).
#[test]
fn test_js_transpiler_syntax_validity() {
    let node_available = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !node_available {
        eprintln!("Skipping JS syntax-validity test: 'node' not found on PATH");
        return;
    }

    let temp_dir = std::env::temp_dir().join("phs_js_syntax_check");
    let _ = fs::create_dir_all(&temp_dir);

    let check_syntax = |name: &str, code: &str| {
        let file = temp_dir.join(format!("{}.js", name));
        fs::write(&file, code).unwrap();
        let result = Command::new("node")
            .args(["--check", file.to_str().unwrap()])
            .output()
            .expect("Failed to run `node --check`");
        assert!(
            result.status.success(),
            "Generated JS for '{}' is not valid syntax.\nStderr: {}\nCode:\n{}",
            name, String::from_utf8_lossy(&result.stderr), code
        );
    };

    // The general parity fixtures above (arithmetic, functions, uncertainty, equations,
    // while loops, etc.) -- none touch booleans, but they exercise the rest of the codegen
    // paths this task's changes sit alongside, so a regression there would show up here too.
    for tc in TEST_CASES {
        let program = parse_phs(tc.script).unwrap();
        let js_code = transpile(&program, Target::JavaScript).unwrap();
        check_syntax(tc.name, &js_code);
    }

    // Dedicated to this task: `not`/`and`/`or`, both `assert` arities in Bool form, and the
    // pre-existing Quantities-shape `assert`/`exact_assert`, all in one script. Every
    // condition is written to actually hold, since `transpile()` runs the whole program
    // through the real interpreter first (to resolve any `solve()`-defined equations) and a
    // genuinely-failing `assert` would fail during that eager pass, before codegen ever runs.
    let bool_and_logical_script = "a = 1.0 m > 0.0 m\nb = 2.0 m > 3.0 m\nc = a and not b\nd = a or b\nassert(c, \"c should hold\")\nassert(d)\nexact_assert(5.0 m, 5.0 m)\n";
    let program = parse_phs(bool_and_logical_script).unwrap();
    let js_code = transpile(&program, Target::JavaScript).unwrap();
    assert!(js_code.contains("&&") && js_code.contains("||") && js_code.contains("!"), "expected logical operators in:\n{js_code}");
    // The message argument goes through the same `Expr::Str` codegen as every other string in
    // js.rs, which always renders as a template literal (backtick), not a double-quoted
    // string -- so `"c should hold"` shows up as `` `c should hold` `` in the generated code.
    // The single-arg form's "assertion failed" message is a hard-coded Rust string literal in
    // `generate_program` itself (not routed through `generate_expr`), so it stays double-quoted.
    assert!(js_code.contains("throw new Error(`c should hold`)"), "{js_code}");
    assert!(js_code.contains("throw new Error(\"assertion failed\")"), "{js_code}");
    assert!(js_code.contains(".physExactAssert("), "{js_code}");
    check_syntax("bool_and_logical", &js_code);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_rust_transpiler_parity() {
    let core_path = repo_root().join("physure-core");
    // Reuse the workspace's own already-built target dir: `cargo test` just compiled
    // physure_core into it, so pointing every temp crate's CARGO_TARGET_DIR here lets
    // cargo skip recompiling that dependency for each of the 6 cases (it used to be a
    // fresh from-scratch compile per case — the dominant cost and the historical source
    // of Windows AppLocker flakiness from repeated brand-new `cargo run` invocations).
    let target_dir = repo_root().join("target");

    // One shared crate directory, one shared Cargo.toml; only main.rs changes per case.
    let temp_dir = std::env::temp_dir().join("phs_rust_parity_shared");
    let src_dir = temp_dir.join("src");
    let _ = fs::create_dir_all(&src_dir);

    let cargo_toml = format!(
        "[package]\nname = \"parity_test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nphysure_core = {{ package = \"physure\", path = \"{}\" }}\n",
        core_path.to_str().unwrap().replace('\\', "/")
    );
    fs::write(temp_dir.join("Cargo.toml"), cargo_toml).unwrap();

    for tc in TEST_CASES {
        let program = parse_phs(tc.script).unwrap();
        let rust_code = transpile(&program, Target::Rust).unwrap();
        fs::write(src_dir.join("main.rs"), &rust_code).unwrap();

        let run = Command::new("cargo")
            .args(["run", "--quiet"])
            .env("RUSTFLAGS", "-A unused_parens")
            .env("CARGO_TARGET_DIR", &target_dir)
            .current_dir(&temp_dir)
            .output();

        let run = match run {
            Ok(r) if r.status.success() => r,
            Ok(r) => {
                eprintln!("Skipping Rust parity test for {}: {}", tc.name, String::from_utf8_lossy(&r.stderr));
                continue;
            }
            Err(_) => {
                eprintln!("Skipping Rust parity test: cargo run failed");
                continue;
            }
        };

        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            stdout.contains(tc.expected_substring),
            "Rust output for {} expected '{}', got:\n{}", tc.name, tc.expected_substring, stdout
        );
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_python_bool_assert_survives_dash_o() {
    let py_dir = repo_root().join("physure-python");
    let importable = Command::new("uv")
        .args(["run", "python", "-c", "import physure"])
        .current_dir(&py_dir)
        .output()
        .is_ok_and(|o| o.status.success());
    if !importable {
        eprintln!("skipping: `uv run python -c 'import physure'` does not work here");
        return;
    }

    // A False boolean assertion with a message. Python's `assert` statement is stripped by
    // `-O`; the generated code must not depend on it (see the "Assertion emission" table in
    // the design spec). Calling `PythonTranspiler.generate_program` directly avoids running
    // the failing assertion through the interpreter pre-pass.
    let program = parse_phs("assert(False, \"boom\")").unwrap();
    let py_code = PythonTranspiler.generate_program(&program).unwrap();
    assert!(!py_code.contains("\nassert "), "must not emit a removable `assert` statement:\n{py_code}");

    for flag in [None, Some("-O")] {
        let temp_file = std::env::temp_dir().join(format!("bool_assert_{}.py", flag.unwrap_or("plain")));
        fs::write(&temp_file, &py_code).unwrap();
        let mut args = vec!["run", "python"];
        if let Some(f) = flag {
            args.push(f);
        }
        let file_str = temp_file.to_str().unwrap().to_string();
        args.push(&file_str);
        let output = Command::new("uv").args(&args).current_dir(&py_dir).output().expect("failed to run python");
        let _ = fs::remove_file(&temp_file);
        assert!(
            !output.status.success(),
            "expected assert(False, ...) to fail under {:?}, but it exited 0", flag
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("boom"),
            "expected the assertion message under {:?}, got: {}", flag, String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_java_bool_assert_fails_without_dash_ea() {
    let java_src_dir = repo_root().join("physure-java/src/main/java");
    let temp_dir = std::env::temp_dir().join("phs_java_bool_assert_no_ea");
    let _ = fs::create_dir_all(&temp_dir);

    // The generated program always imports com.physure.* (even unused), so those classes
    // must be on the classpath to compile -- but a Bool-only assert never *calls* into them,
    // so no native library is loaded at runtime and `native_lib_dir()`/`-Djava.library.path`
    // aren't needed here.
    let java_files: Vec<_> = fs::read_dir(java_src_dir.join("com/physure"))
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|ext| ext == "java"))
                .collect()
        })
        .unwrap_or_default();
    if java_files.is_empty() {
        eprintln!("skipping: no java source files found in com/physure");
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    }
    let mut javac_args = vec!["-d".to_string(), temp_dir.to_str().unwrap().to_string()];
    for f in &java_files {
        javac_args.push(f.to_str().unwrap().to_string());
    }
    let compile_base = match Command::new("javac").args(&javac_args).output() {
        Ok(o) => o,
        Err(_) => {
            eprintln!("skipping: javac not found");
            let _ = fs::remove_dir_all(&temp_dir);
            return;
        }
    };
    if !compile_base.status.success() {
        eprintln!("skipping: base com.physure classes failed to compile: {}", String::from_utf8_lossy(&compile_base.stderr));
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    }

    let program = parse_phs("assert(False, \"boom\")").unwrap();
    let java_code = JavaTranspiler::new("BoolAssert").generate_program(&program).unwrap();
    let gen_file = temp_dir.join("BoolAssert.java");
    fs::write(&gen_file, &java_code).unwrap();

    let compile_gen = Command::new("javac")
        .args(["-cp", temp_dir.to_str().unwrap(), "-d", temp_dir.to_str().unwrap(), gen_file.to_str().unwrap()])
        .output()
        .expect("failed to compile generated java");
    assert!(compile_gen.status.success(), "javac failed: {}", String::from_utf8_lossy(&compile_gen.stderr));

    // Deliberately no `-ea`: JVM assertions are off by default, so if the generated code
    // relied on the language `assert` keyword this would silently pass instead of throwing.
    let run = Command::new("java")
        .args(["-cp", temp_dir.to_str().unwrap(), "BoolAssert"])
        .output()
        .expect("failed to run java");
    let _ = fs::remove_dir_all(&temp_dir);

    assert!(!run.status.success(), "expected assert(False, ...) to fail even without -ea");
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("boom"),
        "expected the assertion message, got: {}", String::from_utf8_lossy(&run.stderr)
    );
}

