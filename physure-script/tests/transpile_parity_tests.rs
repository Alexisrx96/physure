use physure_script::codegen::{transpile, Target};
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
        name: "uncertainty",
        script: "m = 10.0 +/- 0.2 kg\na = 2.5 +/- 0.1 m/s^2\nf = m * a\n",
        expected_substring: "25",
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

    for tc in TEST_CASES {
        let class_name = format!("Parity{}", tc.name.replace("_", ""));
        let program = parse_phs(tc.script).unwrap();
        let java_code = transpile(&program, Target::JavaWithClass(class_name.clone())).unwrap();

        let temp_dir = std::env::temp_dir().join(format!("phs_java_{}", class_name));
        let _ = fs::create_dir_all(&temp_dir);

        let compile_base = Command::new("sh")
            .arg("-c")
            .arg(format!("javac -d {} {}/com/physure/*.java", temp_dir.to_str().unwrap(), java_src_dir.to_str().unwrap()))
            .output()
            .expect("Failed to run javac for base");

        assert!(
            compile_base.status.success(),
            "Java base compile failed for {}: {}", tc.name, String::from_utf8_lossy(&compile_base.stderr)
        );

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
            .output()
            .expect("Failed to run java binary");

        let _ = fs::remove_dir_all(&temp_dir);
        assert!(
            run.status.success(),
            "Java execution failed for {}.\nStderr: {}", tc.name, String::from_utf8_lossy(&run.stderr)
        );

        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            stdout.contains(tc.expected_substring),
            "Java output for {} expected '{}', got:\n{}", tc.name, tc.expected_substring, stdout
        );
    }
}

#[test]
fn test_rust_transpiler_parity() {
    let core_path = repo_root().join("physure-core");

    for tc in TEST_CASES {
        let program = parse_phs(tc.script).unwrap();
        let rust_code = transpile(&program, Target::Rust).unwrap();

        let temp_dir = std::env::temp_dir().join(format!("phs_rust_{}", tc.name));
        let src_dir = temp_dir.join("src");
        let _ = fs::create_dir_all(&src_dir);

        let cargo_toml = format!(
            "[package]\nname = \"parity_test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nphysure_core = {{ package = \"physure\", path = \"{}\" }}\n",
            core_path.to_str().unwrap()
        );
        fs::write(temp_dir.join("Cargo.toml"), cargo_toml).unwrap();
        fs::write(src_dir.join("main.rs"), &rust_code).unwrap();

        let run = Command::new("cargo")
            .args(["run", "--quiet"])
            .current_dir(&temp_dir)
            .output()
            .expect("Failed to run cargo");

        let _ = fs::remove_dir_all(&temp_dir);
        assert!(
            run.status.success(),
            "Rust execution failed for {}.\nStderr: {}\nCode:\n{}",
            tc.name, String::from_utf8_lossy(&run.stderr), rust_code
        );

        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            stdout.contains(tc.expected_substring),
            "Rust output for {} expected '{}', got:\n{}", tc.name, tc.expected_substring, stdout
        );
    }
}
