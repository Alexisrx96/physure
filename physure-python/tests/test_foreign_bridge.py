"""Tests for the Python ergonomic interface over `.phs` modules (Task 5 of
`docs/superpowers/plans/2026-08-27-phs-foreign-bridge.md`): `physure.load_phs` /
`physure.load_dir`, and the `PhsModuleWrapper`/`PhsFunctionWrapper` they build on.

Task 4's `tests/core/test_module_bridge.py` already covers the raw `physure._core
.PhsModuleCore` binding in isolation. This file stays one level up: real `Quantity`
arguments in, real `Quantity` results out, kwargs dispatch, and the error paths a caller
of `load_phs(...).some_fn(...)` can hit.
"""

from __future__ import annotations

import copy

import pytest

import physure
from physure.units import kg, m, s


def test_load_phs_and_invoke(tmp_path):
    phs_file = tmp_path / "kinetics.phs"
    phs_file.write_text("fn E_k(m: kg, v: m/s) = 0.5 * m * v^2\n")

    mod = physure.load_phs(phs_file)
    res = mod.E_k(m=10.0 * kg, v=5.0 * m / s)

    assert res.magnitude == 125.0
    # `res` is a domain `physure.Quantity` (not the bare `physure._core.Quantity` the FFI
    # boundary itself returns), so the alias-aware `str()` -- not `str(res.unit)`, which
    # shows the expanded base-unit form -- is what renders "J".
    assert str(res) == "125.0 J"


def test_load_phs_with_chaining(tmp_path):
    # Declared in plain base-SI compositions (kg, m, s), not named/derived/prefixed units
    # ("bar", "mm", "Pa", ...) as the original plan sketch used. While investigating this
    # task we found those trigger a separate, pre-existing `physure-script` bug (see
    # `_to_core_quantity`'s docstring in `physure/module.py`): a foreign `Quantity` built in
    # a named/derived/prefixed unit is rejected as dimensionally incompatible with a
    # declared parameter of that very same unit, because `bind_param_value` parses the
    # declared unit with a different (registry-expanding) parser than `parse_unit_expression`
    # and this module's own bridge use (atomic) -- `UnitRegistry.get_unit` expands correctly
    # and is not implicated. That bug is in `physure-script`, out of scope here, and
    # cleanly reported separately -- this test instead proves the actual Task 5 deliverable
    # (kwargs dispatch, cross-module chaining, real dimensional coercion, `.to()` conversion
    # on the result) on the unit family that works correctly today.
    geom_file = tmp_path / "geom.phs"
    geom_file.write_text("fn area_tubo(d: m) = 3.1415926535 * (d / 2)^2\n")
    hydr_file = tmp_path / "hydr.phs"
    hydr_file.write_text("fn fuerza_empuje(P: kg/(m*s^2), A: m^2) = P * A\n")

    geom = physure.load_phs(geom_file)
    hydr = physure.load_phs(hydr_file)

    # Chaining the domain-Quantity output of one loaded module directly into another's
    # kwarg -- the real point of the bridge: A never passes through a string.
    fuerza = hydr.fuerza_empuje(
        P="500000 kg/(m*s^2)", A=geom.area_tubo(d="0.05 m")
    )
    assert fuerza.to("N").magnitude == pytest.approx(981.7477, rel=1e-6)


def test_load_phs_accepts_positional_args(tmp_path):
    phs_file = tmp_path / "add.phs"
    phs_file.write_text("fn add(a, b) = a + b\n")

    mod = physure.load_phs(phs_file)

    assert mod.add(3.0, 4.0) == 7.0


def test_missing_function_raises_attribute_error(tmp_path):
    phs_file = tmp_path / "geom.phs"
    phs_file.write_text("fn area(w, h) = w * h\n")

    mod = physure.load_phs(phs_file)

    with pytest.raises(AttributeError):
        mod.does_not_exist(1.0, 2.0)


def test_missing_required_kwarg_raises_value_error(tmp_path):
    phs_file = tmp_path / "kinetics.phs"
    phs_file.write_text("fn E_k(m: kg, v: m/s) = 0.5 * m * v^2\n")

    mod = physure.load_phs(phs_file)

    with pytest.raises(ValueError, match="m"):
        mod.E_k(v=5.0 * m / s)


def test_wrong_dimension_argument_raises_value_error(tmp_path):
    """A `Quantity` of the wrong dimension must raise, not silently coerce.

    This is the real hazard `_to_core_quantity` exists to prevent: a domain `Quantity`
    passed straight to `PhsModuleCore.invoke` defines `__float__`, so without the bridge
    this would silently drop the "seconds" unit and succeed with a bogus dimensionless
    result instead of failing (confirmed by hand while investigating this task).
    """
    phs_file = tmp_path / "mass.phs"
    phs_file.write_text("fn double_mass(m: kg) = 2.0 * m\n")

    mod = physure.load_phs(phs_file)

    with pytest.raises(ValueError, match="incompatible"):
        mod.double_mass(m=10.0 * s)


def test_load_phs_raises_file_not_found_for_missing_file(tmp_path):
    missing = tmp_path / "does_not_exist.phs"

    with pytest.raises(FileNotFoundError):
        physure.load_phs(missing)


def test_load_dir_loads_every_phs_file_by_stem(tmp_path):
    (tmp_path / "geom.phs").write_text("fn area(w, h) = w * h\n")
    (tmp_path / "kinetics.phs").write_text(
        "fn E_k(m: kg, v: m/s) = 0.5 * m * v^2\n"
    )
    (tmp_path / "not_phs.txt").write_text("ignore me\n")

    modules = physure.load_dir(tmp_path)

    assert set(modules.keys()) == {"geom", "kinetics"}
    assert modules["geom"].area(3.0, 4.0) == 12.0
    assert (
        modules["kinetics"].E_k(m=10.0 * kg, v=5.0 * m / s).magnitude == 125.0
    )


def test_module_wrapper_dir_lists_functions(tmp_path):
    phs_file = tmp_path / "geom.phs"
    phs_file.write_text(
        "fn area(w, h) = w * h\nfn perimeter(w, h) = 2 * (w + h)\n"
    )

    mod = physure.load_phs(phs_file)

    assert {"area", "perimeter"}.issubset(set(dir(mod)))


@pytest.mark.xfail(
    reason=(
        "Pre-existing physure-script bug, out of scope for Task 5 (see "
        "physure/module.py's _to_core_quantity docstring): PhsModule::bind_param_value "
        "(physure-script/src/interpreter/expressions.rs) parses a declared parameter unit "
        "with the registry-expanding parser, while parse_unit_expression and this module's "
        "own bridge (_to_core_quantity, reconstructing atomically from a CompoundUnit's raw "
        ".exponents) stay atomic/unexpanded. UnitRegistry.get_unit -- used internally by "
        "domain Quantity objects like 10 * N -- was independently verified to expand "
        "correctly and is NOT implicated. RationalUnit::same_dimensions compares raw symbol "
        "keys rather than reduced dimensions, so a dimensionally-identical foreign Quantity "
        "in a named/derived/prefixed unit is rejected. Pinned here (rather than silently "
        "working around it) so a physure-script fix flips this to an unexpected pass and "
        "gets noticed."
    ),
    strict=True,
)
def test_named_unit_parameter_rejects_matching_foreign_quantity(tmp_path):
    grams_file = tmp_path / "g_mass.phs"
    grams_file.write_text("fn identity(m: g) = m\n")

    mod = physure.load_phs(grams_file)

    # 5 g is exactly what `m: g` declares -- this should succeed, not raise.
    from physure.units import g

    result = mod.identity(m=5.0 * g)
    assert result.magnitude == pytest.approx(5.0)


def test_mixed_positional_and_keyword_arguments(tmp_path):
    phs_file = tmp_path / "math_ops.phs"
    phs_file.write_text("fn calc(a, b, c) = a * 100 + b * 10 + c\n")

    mod = physure.load_phs(phs_file)

    # 1. All positional
    assert mod.calc(1, 2, 3) == 123.0
    # 2. Mixed: 1 positional, 2 keyword
    assert mod.calc(1, b=2, c=3) == 123.0
    # 3. Mixed: 2 positional, 1 keyword
    assert mod.calc(1, 2, c=3) == 123.0
    # 4. All keyword out of order
    assert mod.calc(c=3, a=1, b=2) == 123.0


def test_argument_binding_error_cases(tmp_path):
    phs_file = tmp_path / "math_ops.phs"
    phs_file.write_text("fn add(x, y) = x + y\n")

    mod = physure.load_phs(phs_file)

    # Too many positional arguments
    with pytest.raises(TypeError, match="positional arguments"):
        mod.add(1, 2, 3)

    # Unexpected keyword argument
    with pytest.raises(TypeError, match="unexpected keyword argument"):
        mod.add(1, unexpected=2)

    # Multiple values for argument
    with pytest.raises(TypeError, match="multiple values"):
        mod.add(1, x=2)

    # Missing required parameter
    with pytest.raises(ValueError, match="Missing required parameter 'y'"):
        mod.add(1)


def test_plain_string_arguments_do_not_crash(tmp_path):
    phs_file = tmp_path / "strings.phs"
    phs_file.write_text("fn greet(name) = name\n")

    mod = physure.load_phs(phs_file)
    assert mod.greet("World") == "World"


def test_function_wrapper_introspection_and_signature(tmp_path):
    import inspect

    phs_file = tmp_path / "geom.phs"
    phs_file.write_text("fn rectangle_area(width, height) = width * height\n")

    mod = physure.load_phs(phs_file)
    fn = mod.rectangle_area

    assert fn.__name__ == "rectangle_area"
    sig = inspect.signature(fn)
    assert list(sig.parameters.keys()) == ["width", "height"]


def test_module_wrapper_subscript_and_membership(tmp_path):
    phs_file = tmp_path / "calc.phs"
    phs_file.write_text("fn square(x) = x^2\n")

    mod = physure.load_phs(phs_file)

    # 1. Membership test
    assert "square" in mod
    assert "cube" not in mod

    # 2. Subscript access
    fn = mod["square"]
    assert fn(4.0) == 16.0

    # 3. Key error on missing function
    with pytest.raises(KeyError, match="Module has no function 'cube'"):
        _ = mod["cube"]


def test_getattr_on_uninitialized_instance_raises_attribute_error_not_recursion():
    """`__getattr__` must not infinitely recurse when `_fn_cache` was never set.

    `__getattr__` only runs when normal attribute lookup already failed, so a bare
    `self._fn_cache` read inside it would -- on an instance produced via
    `cls.__new__(cls)` with `__init__` never run -- itself miss and re-enter
    `__getattr__` forever (confirmed: this used to raise `RecursionError`). This is
    exactly the state `copy.copy`'s default reconstruction path puts an object in before
    restoring `__dict__`.
    """
    from physure.module import PhsModuleWrapper

    uninitialized = PhsModuleWrapper.__new__(PhsModuleWrapper)

    with pytest.raises(AttributeError):
        _ = uninitialized.some_attr


def test_copy_copy_does_not_recurse(tmp_path):
    """`copy.copy(mod)` must not raise `RecursionError` (the original Bug 1 repro).

    `copy.copy`'s default path builds a fresh, un-`__init__`-ed instance via
    `cls.__new__(cls)` before copying `__dict__` onto it -- triggering the same
    uninitialized-instance path as the test above. Whether the resulting shallow copy is
    fully usable (it wraps a PyO3 handle that may not support copying) is out of scope
    here; only the absence of infinite recursion is being asserted.
    """
    phs_file = tmp_path / "geom.phs"
    phs_file.write_text("fn area(w, h) = w * h\n")
    mod = physure.load_phs(phs_file)

    try:
        copy.copy(mod)
    except RecursionError:
        raise
    except Exception:
        pass
