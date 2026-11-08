"""Python under a frozen configuration it cannot honour yet (Phase 5 wires it).

The ``UnitSystem`` is built from ``default_conf()`` (the library's embedded catalog) with ``configparser``; with a
frozen configuration in force that would be a different catalog from the one the core and PHS use, in the same
process, without a word. Until Phase 5 the binding refuses instead: an ordinary ``RuntimeError``, never a Rust
panic (``PanicException`` is a ``BaseException`` that ``except Exception`` does not catch).

Each case runs in a fresh interpreter in a temporary directory: the configuration in force is resolved from the
current directory and the environment, and is process-wide.
"""

import os
import subprocess
import sys
import textwrap

import pytest


def run(code: str, cwd, **env: str) -> subprocess.CompletedProcess:
    environment = {
        k: v for k, v in os.environ.items() if k not in ("PHYSURE_CONF", "PHYSURE_REQUIRE_FROZEN")
    }
    environment.update(env)
    return subprocess.run(
        [sys.executable, "-c", textwrap.dedent(code)],
        cwd=cwd,
        env=environment,
        capture_output=True,
        text=True,
        timeout=120,
    )


PROBE = """
    import physure._core as core
    for name, call in [
        ("default_conf", core.default_conf),
        ("from_conf", core.UnitRegistry.from_conf),
        ("unit_facts", lambda: core.unit_facts("m")),
    ]:
        try:
            call()
        except RuntimeError as e:
            print(name, "RuntimeError", "physure.frozen.conf" in str(e) or "PHYSURE" in str(e))
        else:
            print(name, "ok")
"""


@pytest.fixture
def directory(tmp_path):
    return tmp_path


def test_without_a_frozen_configuration_nothing_changes(directory):
    result = run(PROBE, directory)
    assert result.returncode == 0, result.stderr
    assert result.stdout.split("\n")[:3] == ["default_conf ok", "from_conf ok", "unit_facts ok"]


def test_a_frozen_file_in_force_is_refused_with_an_ordinary_exception(directory):
    (directory / "physure.frozen.conf").write_text("not a frozen configuration", encoding="utf-8")
    result = run(PROBE, directory)
    assert result.returncode == 0, result.stderr
    assert "PanicException" not in result.stderr
    assert result.stdout.split("\n")[:3] == [
        "default_conf RuntimeError True",
        "from_conf RuntimeError True",
        "unit_facts RuntimeError True",
    ]


def test_a_required_frozen_file_that_is_missing_is_refused(directory):
    result = run(PROBE, directory, PHYSURE_REQUIRE_FROZEN="1")
    assert result.returncode == 0, result.stderr
    assert result.stdout.split("\n")[:3] == [
        "default_conf RuntimeError True",
        "from_conf RuntimeError True",
        "unit_facts RuntimeError True",
    ]


def test_importing_physure_under_a_frozen_file_raises_runtime_error(directory):
    (directory / "physure.frozen.conf").write_text("not a frozen configuration", encoding="utf-8")
    code = """
        try:
            import physure
            physure.Q_(1, "m")
        except RuntimeError as e:
            print("RuntimeError", "physure.frozen.conf" in str(e))
        else:
            print("ok")
    """
    result = run(code, directory)
    assert "PanicException" not in result.stderr
    assert result.stdout.strip() == "RuntimeError True", result.stdout + result.stderr
