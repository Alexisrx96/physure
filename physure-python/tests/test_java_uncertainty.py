"""The Java fallback combines uncertainties the way Rust and Python do.

`Quantity.java` used to add them linearly, so the same calculation reported an error
about 40% larger in Java than in the other two. There is no JUnit in the build, so the
check compiles the classes with `javac` and runs a small program against them — which
also keeps it away from `NativeEngine`, whose static initializer needs the shared library.
"""

import os
import shutil
import subprocess

import pytest

REPO_ROOT = os.path.abspath(
    os.path.join(os.path.dirname(__file__), "..", "..")
)
JAVA_SRC = os.path.join(REPO_ROOT, "physure-java", "src", "main", "java")

PROBE = """
package com.physure;

public class UncertaintyProbe {
    public static void main(String[] args) {
        System.out.println(Quantity.combineUncertainty(0.3, 0.3));
        System.out.println(Quantity.combineUncertainty(0.0, 0.5));
        System.out.println(Quantity.combineUncertainty(3.0, 4.0));
    }
}
"""


@pytest.mark.skipif(
    shutil.which("javac") is None or shutil.which("java") is None,
    reason="needs a JDK on PATH",
)
def test_java_adds_uncertainties_in_quadrature(tmp_path):
    probe = tmp_path / "UncertaintyProbe.java"
    probe.write_text(PROBE)

    compile_base = subprocess.run(
        ["javac", "-d", str(tmp_path), *_java_sources()],
        capture_output=True,
        text=True,
        check=False,
    )
    assert compile_base.returncode == 0, compile_base.stderr

    compile_probe = subprocess.run(
        ["javac", "-cp", str(tmp_path), "-d", str(tmp_path), str(probe)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert compile_probe.returncode == 0, compile_probe.stderr

    run = subprocess.run(
        ["java", "-cp", str(tmp_path), "com.physure.UncertaintyProbe"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert run.returncode == 0, run.stderr

    combined = [float(line) for line in run.stdout.split()]
    # 0.3 and 0.3 give 0.4243, not the 0.6 a linear sum reports.
    assert combined[0] == pytest.approx(0.42426406871, abs=1e-9)
    # An exact operand contributes nothing.
    assert combined[1] == pytest.approx(0.5)
    assert combined[2] == pytest.approx(5.0)


def _java_sources():
    src = os.path.join(JAVA_SRC, "com", "physure")
    return [
        os.path.join(src, name)
        for name in sorted(os.listdir(src))
        if name.endswith(".java")
    ]
