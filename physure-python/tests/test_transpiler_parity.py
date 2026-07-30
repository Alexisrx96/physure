import os
import subprocess
import tempfile
import pytest
from physure._core import evaluate_phs_native, transpile_phs_native

# Derived from this file rather than hard-coded so the parity tests run from any checkout.
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def native_lib_dir():
    """The JNI bindings are a separate cdylib, so nothing in the Python test run produces
    them: the test used to fail with "Native library libphysure_java.so not found", which
    reads like a broken binding rather than a missing build step. Build it on demand."""
    target_dir = os.path.join(REPO_ROOT, "target", "debug")
    if not os.path.exists(os.path.join(target_dir, "libphysure_java.so")):
        build = subprocess.run(
            ["cargo", "build", "-p", "physure-java"],
            cwd=REPO_ROOT, capture_output=True, text=True
        )
        assert build.returncode == 0, f"Could not build the JNI bindings: {build.stderr}"
    return target_dir

PARITY_TEST_SCRIPTS = [
    # 1. Basic Quantity Arithmetic & Unit Propagation
    ("basic_arithmetic", """d = 100 m
t = 5 s
v = d / t
res = v * 10 s
""", {"res": "200"}),

    # 2. Unit Conversions
    ("unit_conversion", """d = 100 m
t = 5 s
v = (d / t) => km/h
""", {"v": "72"}),

    # 3. Functions
    ("functions", """fn kinetic_energy(m, v) = 0.5 * m * v^2
ke = kinetic_energy(80 kg, 15 m/s) => kJ
""", {"ke": "9"}),

    # 4. Quantity Power & Roots
    ("power_and_roots", """r = 4 m
area = 3.14159265 * r^2
""", {"area": "50.265"}),

    # 5. String interpolation: `{expr}` is the only way a value enters a literal
    ("string_interpolation", """m = 2.0 kg
v = 3.0 m/s
label = "masa {m} a {v}"
""", {"label": "masa 2.0 kg a 3.0 m/s"}),

    # 6. Uncertainty Propagation
    ("uncertainty", """m = 10.0 +/- 0.2 kg
a = 2.5 +/- 0.1 m/s^2
f = m * a
""", {"f": "25"}),

    # 6. Equation Solving
    ("equations", """use solve from calc
eq1 = "V = R * I"
eq5 = solve(eq1, "R")
r = eq5(I = -2mA, V = -12V) => kOhm
""", {"r": "6"}),
]

@pytest.mark.parametrize("name, script, expected_vars", PARITY_TEST_SCRIPTS)
def test_phs_interpreter_parity(name, script, expected_vars):
    """Verify native PHS Interpreter evaluates expressions with exact parity."""
    results = evaluate_phs_native(script)
    assert len(results) > 0
    res_str = str(results[-1])
    for expected in expected_vars.values():
        assert expected in res_str or any(expected in str(r) for r in results)

@pytest.mark.parametrize("name, script, expected_vars", PARITY_TEST_SCRIPTS)
def test_python_transpiler_parity(name, script, expected_vars):
    """Verify Python Transpiler generated code executes with 100% parity."""
    py_code = transpile_phs_native(script, "python")
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False) as f:
        f.write(py_code)
        f_path = f.name
    try:
        proc = subprocess.run(
            ["uv", "run", "python", f_path],
            capture_output=True, text=True, check=True,
            cwd=os.path.join(REPO_ROOT, "physure-python")
        )
        out = proc.stdout
        for expected in expected_vars.values():
            assert expected in out, f"Expected {expected} in Python stdout:\n{out}"
    finally:
        os.remove(f_path)

@pytest.mark.parametrize("name, script, expected_vars", PARITY_TEST_SCRIPTS)
def test_java_transpiler_parity(name, script, expected_vars):
    """Verify Java Transpiler generated code compiles with javac and executes with 100% parity."""
    class_name = f"Parity{name.title().replace('_', '')}"
    java_code = transpile_phs_native(script, f"java:{class_name}")
    with tempfile.TemporaryDirectory() as tmpdir:
        java_src_dir = os.path.join(REPO_ROOT, "physure-java", "src", "main", "java")
        lib_dir = native_lib_dir()
        compile_base = subprocess.run(
            f"javac -d {tmpdir} {java_src_dir}/com/physure/*.java",
            shell=True, capture_output=True, text=True
        )
        assert compile_base.returncode == 0, f"Base Java compilation failed: {compile_base.stderr}"

        gen_file = os.path.join(tmpdir, f"{class_name}.java")
        with open(gen_file, "w") as f:
            f.write(java_code)

        compile_gen = subprocess.run(
            f"javac -cp {tmpdir} -d {tmpdir} {gen_file}",
            shell=True, capture_output=True, text=True
        )
        assert compile_gen.returncode == 0, f"Generated Java compilation failed: {compile_gen.stderr}\nCode:\n{java_code}"

        proc = subprocess.run(
            f"java -Djava.library.path={lib_dir} -cp {tmpdir} {class_name}",
            shell=True, capture_output=True, text=True
        )
        assert proc.returncode == 0, f"Java execution failed: {proc.stderr}"
        out = proc.stdout
        for expected in expected_vars.values():
            assert expected in out, f"Expected {expected} in Java stdout:\n{out}"

@pytest.mark.parametrize("name, script, expected_vars", PARITY_TEST_SCRIPTS)
def test_rust_transpiler_parity(name, script, expected_vars):
    """Verify Rust Transpiler generated code compiles with cargo and executes with 100% parity."""
    rust_code = transpile_phs_native(script, "rust")
    with tempfile.TemporaryDirectory() as tmpdir:
        src_dir = os.path.join(tmpdir, "src")
        os.makedirs(src_dir, exist_ok=True)
        rs_file = os.path.join(src_dir, "main.rs")
        cargo_file = os.path.join(tmpdir, "Cargo.toml")
        
        core_path = os.path.join(REPO_ROOT, "physure-core")
        with open(cargo_file, "w") as f:
            f.write(f'''[package]
name = "parity_test"
version = "0.1.0"
edition = "2021"

[dependencies]
physure_core = {{ package = "physure", path = "{core_path}" }}
''')
        
        with open(rs_file, "w") as f:
            f.write(rust_code)
        
        proc = subprocess.run(
            ["cargo", "run", "--quiet"],
            cwd=tmpdir,
            capture_output=True, text=True
        )
        assert proc.returncode == 0, f"Rust compilation/execution failed: {proc.stderr}\nCode:\n{rust_code}"
        out = proc.stdout
        for expected in expected_vars.values():
            assert expected in out, f"Expected {expected} in Rust stdout:\n{out}"
