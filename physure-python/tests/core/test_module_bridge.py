"""Exercises the raw PyO3 `PhsModuleCore` binding (`physure._core.PhsModuleCore`), the thin
wrapper around `physure_script::PhsModule` added by Task 4 of the foreign-bridge plan
(`docs/superpowers/plans/2026-08-27-phs-foreign-bridge.md`).

This deliberately stays at the `_core` level -- no `physure.load_phs()` ergonomics, no kwargs
dispatch. Task 5 builds that layer on top and gets its own tests; this file's job is only to
prove the compiled extension itself round-trips introspection and invocation correctly,
including unit coercion and the error paths a foreign caller can hit.
"""

import pytest

from physure._core import PhsModuleCore, Quantity, UnitRegistry

PHYSICS_SOURCE = """
/// Computes kinetic energy in Joules
/// @param m Mass of the body in kg
/// @param v Velocity in m/s
fn E_k(m: kg, v: m/s) = 0.5 * m * v^2

fn add(a, b) = a + b
"""


def test_from_source_lists_every_top_level_function():
    module = PhsModuleCore.from_source("physics", PHYSICS_SOURCE)
    assert set(module.list_functions()) == {"E_k", "add"}


def test_get_params_returns_declared_parameter_names_in_order():
    module = PhsModuleCore.from_source("physics", PHYSICS_SOURCE)
    assert module.get_params("E_k") == ["m", "v"]
    assert module.get_params("add") == ["a", "b"]


def test_get_params_raises_key_error_for_unknown_function():
    module = PhsModuleCore.from_source("physics", PHYSICS_SOURCE)
    with pytest.raises(KeyError):
        module.get_params("does_not_exist")


def test_invoke_computes_with_plain_numbers():
    module = PhsModuleCore.from_source("mathy", "fn add(a, b) = a + b")
    assert module.invoke("add", [3.0, 4.0]) == 7.0


def test_invoke_coerces_a_differently_scaled_but_compatible_quantity_unit():
    # Mirrors physure-script's own
    # `test_invoke_coerces_a_differently_scaled_but_compatible_unit`: passing 5000 g into a
    # `kg`-declared parameter must convert, not just pass through -- proving real dimensional
    # coercion survives the PyO3 boundary rather than only same-unit no-ops.
    reg = UnitRegistry.from_conf()
    grams = Quantity(5000.0, reg.get_unit("g"))
    module = PhsModuleCore.from_source(
        "massy", "fn double_mass(m: kg) = 2.0 * m"
    )

    result = module.invoke("double_mass", [grams])

    assert repr(result.unit) == "kg"
    assert result.magnitude == 10.0


def test_invoke_rejects_dimensionally_incompatible_quantity():
    reg = UnitRegistry.from_conf()
    seconds = Quantity(5.0, reg.get_unit("s"))
    module = PhsModuleCore.from_source(
        "ke", "fn E_k(m: kg, v: m/s) = 0.5 * m * v^2"
    )

    with pytest.raises(ValueError, match="parameter 'v'"):
        module.invoke("E_k", [Quantity(10.0, reg.get_unit("kg")), seconds])


def test_invoke_raises_value_error_for_unknown_function():
    module = PhsModuleCore.from_source("mathy", "fn add(a, b) = a + b")
    with pytest.raises(ValueError, match="is not a function this module"):
        module.invoke("subtract", [1.0, 2.0])


def test_invoke_raises_value_error_on_argument_count_mismatch():
    module = PhsModuleCore.from_source("mathy", "fn add(a, b) = a + b")
    with pytest.raises(ValueError, match="expects 2 args"):
        module.invoke("add", [1.0])


def test_from_source_raises_value_error_on_parse_failure():
    with pytest.raises(ValueError, match="Parse error"):
        PhsModuleCore.from_source("broken", "fn (((( invalid")


def test_from_file_loads_a_phs_file_and_can_invoke_its_functions(tmp_path):
    phs_file = tmp_path / "geometry.phs"
    phs_file.write_text("fn area(w, h) = w * h")

    module = PhsModuleCore.from_file(str(phs_file))

    assert module.list_functions() == ["area"]
    assert module.invoke("area", [3.0, 4.0]) == 12.0


def test_from_file_raises_file_not_found_error_when_file_does_not_exist(
    tmp_path,
):
    # A missing file must surface as the `FileNotFoundError` a Python caller would naturally
    # guard for, not a generic `ValueError` wrapping a raw (OS-locale-dependent) io::Error
    # message -- see the PyO3-boundary check in `PyPhsModule::from_file`.
    missing = tmp_path / "does_not_exist.phs"
    with pytest.raises(FileNotFoundError):
        PhsModuleCore.from_file(str(missing))


def test_invoke_raises_type_error_for_an_unconvertible_argument():
    # A plain object with no `__iter__` (and not a bool/float/str/Quantity/None) can't be
    # turned into any `PhsValue` by `py_to_phs_value` -- confirm that comes back as a `TypeError`
    # instead of silently doing something surprising. (A `dict` argument is a separate,
    # pre-existing case: it silently becomes a `Vector` of its keys via `py_to_phs_value`'s
    # `try_iter()` fallback, inherited behavior from before this task and out of scope here.)
    class NotConvertible:
        pass

    module = PhsModuleCore.from_source("mathy", "fn add(a, b) = a + b")
    with pytest.raises(TypeError):
        module.invoke("add", [NotConvertible(), 1.0])
