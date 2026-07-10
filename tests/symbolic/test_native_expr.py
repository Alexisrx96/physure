# tests/symbolic/test_native_expr.py
"""Shared behavioral suite for the native symbolic engine.

Runs identical assertions against the Rust `Expr` (when built) and the pure
-Python fallback `PyExpr`, per CLAUDE.md's "Rust core is always optional"
invariant — divergence between engines shows up as a red test.
"""

from __future__ import annotations

import pytest

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.domain.symbolic.native import Expr, PyExpr

ENGINES = list({Expr, PyExpr})


@pytest.fixture(
    params=ENGINES, ids=lambda e: e.__module__ + "." + e.__qualname__
)
def engine(request: pytest.FixtureRequest):
    return request.param


# --- simplification laws (§3.1) ------------------------------------------


def test_add_zero_identity(engine):
    x = engine.symbol("x")
    assert (x + engine.number(0.0)).simplify() == x


def test_mul_one_identity(engine):
    x = engine.symbol("x")
    assert (x * engine.number(1.0)).simplify() == x


def test_mul_zero(engine):
    x = engine.symbol("x")
    assert (x * engine.number(0.0)).simplify() == engine.number(0.0)


def test_pow_zero(engine):
    x = engine.symbol("x")
    assert (x ** engine.number(0.0)).simplify() == engine.number(1.0)


def test_sub_self_is_zero(engine):
    x = engine.symbol("x")
    assert (x - x).simplify() == engine.number(0.0)


def test_div_self_is_one(engine):
    x = engine.symbol("x")
    assert (x / x).simplify() == engine.number(1.0)


def test_add_collects_equal_terms(engine):
    x = engine.symbol("x")
    assert (x + x).simplify() == (engine.number(2.0) * x).simplify()


def test_mul_collects_equal_factors(engine):
    x = engine.symbol("x")
    assert (x * x).simplify() == (x ** engine.number(2.0)).simplify()


def test_constant_folding(engine):
    result = (engine.number(2.0) + engine.number(3.0)).simplify()
    assert result == engine.number(5.0)


# --- differentiation (§4, SymEngine-aligned §5) --------------------------


def test_diff_constant_is_zero(engine):
    assert engine.number(5.0).diff("x") == engine.number(0.0)


def test_diff_symbol_wrt_self_is_one(engine):
    assert engine.symbol("x").diff("x") == engine.number(1.0)


def test_diff_symbol_wrt_other_is_zero(engine):
    assert engine.symbol("x").diff("y") == engine.number(0.0)


def test_diff_power_rule(engine):
    x = engine.symbol("x")
    expected = (engine.number(3.0) * x ** engine.number(2.0)).simplify()
    assert (x ** engine.number(3.0)).diff("x") == expected


def test_diff_sin_chain_rule(engine):
    x = engine.symbol("x")
    assert engine.sin(x).diff("x") == engine.cos(x)


def test_diff_product_rule(engine):
    x = engine.symbol("x")
    y = engine.symbol("y")
    assert (x * y).diff("x") == y


# --- integration (§4.2, SymEngine-aligned §5) -----------------------------


def test_integrate_power_rule(engine):
    x = engine.symbol("x")
    expected = (x ** engine.number(3.0) / engine.number(3.0)).simplify()
    assert (x ** engine.number(2.0)).integrate("x") == expected


def test_integrate_cos_is_sin(engine):
    x = engine.symbol("x")
    assert engine.cos(x).integrate("x") == engine.sin(x)


def test_integrate_sin_is_neg_cos(engine):
    x = engine.symbol("x")
    expected = (engine.number(-1.0) * engine.cos(x)).simplify()
    assert engine.sin(x).integrate("x") == expected


def test_integrate_constant(engine):
    expected = (engine.number(5.0) * engine.symbol("x")).simplify()
    assert engine.number(5.0).integrate("x") == expected


def test_integrate_linear_chain_rule(engine):
    x = engine.symbol("x")
    arg = (engine.number(2.0) * x).simplify()
    expected = (engine.sin(arg) / engine.number(2.0)).simplify()
    assert engine.cos(arg).integrate("x") == expected


def test_integrate_u_substitution(engine):
    x = engine.symbol("x")
    g = x ** engine.number(2.0)
    g_prime = engine.number(2.0) * x
    expr = g_prime * engine.cos(g)
    assert expr.integrate("x") == engine.sin(g)


def test_integrate_reciprocal_is_ln(engine):
    x = engine.symbol("x")
    assert (x ** engine.number(-1.0)).integrate("x") == engine.ln(x)


def test_integrate_non_matching_pattern_raises(engine):
    x = engine.symbol("x")
    with pytest.raises(NotImplementedError):
        (x * engine.ln(x)).integrate("x")


# --- unit-awareness (§6) --------------------------------------------------


def test_add_mismatched_units_raises(engine):
    a = engine.quantity("a", CompoundUnit({"length": 1}))
    b = engine.quantity("b", CompoundUnit({"mass": 1}))
    # ponytail: the Rust engine raises a plain PyValueError (no custom
    # exception type crosses the PyO3 boundary); the Python fallback raises
    # the richer IncompatibleUnitsError. Both signal the same thing —
    # "raise, don't silently continue" — so accept either here.
    with pytest.raises((IncompatibleUnitsError, ValueError)):
        a + b


def test_add_matching_units_reports_shared_unit(engine):
    length = CompoundUnit({"length": 1})
    a = engine.quantity("a", length)
    c = engine.quantity("c", length)
    assert (a + c).unit() == length


def test_div_composes_units(engine):
    x = engine.quantity("x", CompoundUnit({"length": 1}))
    t = engine.quantity("t", CompoundUnit({"time": 1}))
    assert (x / t).unit() == CompoundUnit({"length": 1, "time": -1})


def test_mul_composes_units(engine):
    x = engine.quantity("x", CompoundUnit({"length": 1}))
    t = engine.quantity("t", CompoundUnit({"time": 1}))
    assert (x * t).unit() == CompoundUnit({"length": 1, "time": 1})


def test_diff_propagates_units(engine):
    x = engine.quantity("x", CompoundUnit({"length": 1}))
    t = engine.quantity("t", CompoundUnit({"time": 1}))
    # d/dt(x * t) = x, so the derivative's unit is x's unit: length.
    assert (x * t).diff("t").unit() == CompoundUnit({"length": 1})
