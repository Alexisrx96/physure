import os
import subprocess
import tempfile

import pytest

from physure._core import transpile_phs_native

# Derived from this file rather than hard-coded so the parity tests run from any checkout.
REPO_ROOT = os.path.dirname(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
)


PARITY_TEST_SCRIPTS = [
    # 1. Basic Quantity Arithmetic & Unit Propagation
    (
        "basic_arithmetic",
        """d = 100 m
t = 5 s
v = d / t
res = v * 10 s
""",
        {"res": "200"},
    ),
    # 2. Unit Conversions
    (
        "unit_conversion",
        """d = 100 m
t = 5 s
v = (d / t) => km/h
""",
        {"v": "72"},
    ),
    # 3. Functions
    (
        "functions",
        """fn kinetic_energy(m, v) = 0.5 * m * v^2
ke = kinetic_energy(80 kg, 15 m/s) => kJ
""",
        {"ke": "9"},
    ),
    # 4. Quantity Power & Roots
    (
        "power_and_roots",
        """r = 4 m
area = 3.14159265 * r^2
""",
        {"area": "50.265"},
    ),
    # 5. String interpolation: `{expr}` is the only way a value enters a literal.
    # Named `mass`, not `m`: the parser flags a quantity literal's unit word against every
    # bound name in scope, including the first, and `v = 3.0 m/s` would otherwise collide
    # with a variable named `m` (see physure-script/tests/unit_shadowing.rs).
    (
        "string_interpolation",
        """mass = 2.0 kg
v = 3.0 m/s
label = "masa {mass} a {v}"
""",
        {"label": "masa 2.0 kg a 3.0 m/s"},
    ),
    # 6. Uncertainty Propagation
    (
        "uncertainty",
        """mass = 10.0 +/- 0.2 kg
a = 2.5 +/- 0.1 m/s^2
f = mass * a
""",
        {"f": "25"},
    ),
    # 7. Equation Solving
    (
        "equations",
        """use solve from calc
eq1 = "V = R * I"
eq5 = solve(eq1, "R")
r = eq5(I = -2mA, V = -12V) => kOhm
""",
        {"r": "6"},
    ),
    # 8. `where` clause: inline local bindings scoped to a single expression
    (
        "where_clause",
        """duplo = a + b where a = 2.0 m, b = a * 3.0
""",
        {"duplo": "8"},
    ),
]


# Cross-language parity (native interpreter, Java transpiler, Rust transpiler) is
# owned entirely by physure-script/tests/transpile_parity_tests.rs — none of it depends
# on the Python interpreter, so re-running it here per Python-version matrix leg would
# just be duplicated, slower coverage of the same assertions. This file's only job is
# what genuinely is a Python concern: does the transpiled Python code run correctly
# under *this* interpreter.
@pytest.mark.parametrize(
    ("name", "script", "expected_vars"), PARITY_TEST_SCRIPTS
)
def test_python_transpiler_parity(name, script, expected_vars):
    """Verify Python Transpiler generated code executes with 100% parity."""
    py_code = transpile_phs_native(script, "python")
    with tempfile.NamedTemporaryFile(
        suffix=".py", mode="w", delete=False
    ) as f:
        f.write(py_code)
        f_path = f.name
    try:
        proc = subprocess.run(
            ["uv", "run", "python", f_path],
            capture_output=True,
            text=True,
            check=True,
            cwd=os.path.join(REPO_ROOT, "physure-python"),
        )
        out = proc.stdout
        for expected in expected_vars.values():
            assert expected in out, (
                f"Expected {expected} in Python stdout:\n{out}"
            )
    finally:
        os.remove(f_path)
