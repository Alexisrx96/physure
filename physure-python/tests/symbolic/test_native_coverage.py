# tests/symbolic/test_native_coverage.py
"""Targeted coverage for physure/domain/symbolic/native.py's internal branches.

Calls private helpers directly to exercise unit-inference, differentiation,
integration, and simplification branches not reached by the shared
dual-engine behavioral suite in test_native_expr.py.
"""

from __future__ import annotations

import pytest

from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.units import CompoundUnit
from physure.domain.symbolic.native import (
    Add,
    Cos,
    Div,
    Exp,
    Ln,
    Mul,
    Number,
    Pow,
    PyExpr,
    Quantity,
    Sin,
    Sub,
    Symbol,
    _antiderivative_of_outer,
    _arg_form,
    _collect_with_multiplicity,
    _const_coeff,
    _inner_arg,
    _try_u_substitution,
    check_add_compat,
    depends_on,
    diff_node,
    flatten_add,
    flatten_mul,
    infer_unit,
    integrate_cos,
    integrate_div,
    integrate_exp,
    integrate_ln,
    integrate_mul,
    integrate_node,
    integrate_pow,
    integrate_sin,
    linear_coeff,
    simplify,
    simplify_add,
    simplify_div,
    simplify_mul,
    simplify_pow,
    simplify_sub,
)

LENGTH = CompoundUnit({"length": 1})


# --- unit inference (§6) ----------------------------------------------


def test_add_like_unit_mixed_none_and_unit_terms():
    node = Add((Symbol("x"), Quantity("a", LENGTH)))
    assert infer_unit(node) == LENGTH


def test_infer_unit_sub():
    node = Sub(Quantity("a", LENGTH), Quantity("b", LENGTH))
    assert infer_unit(node) == LENGTH


def test_infer_unit_div_only_numerator_has_unit():
    node = Div(Quantity("x", LENGTH), Symbol("t"))
    assert infer_unit(node) == LENGTH


def test_infer_unit_div_only_denominator_has_unit():
    node = Div(Symbol("x"), Quantity("t", LENGTH))
    assert infer_unit(node) == CompoundUnit({}) / LENGTH


def test_infer_unit_div_neither_has_unit():
    node = Div(Symbol("x"), Symbol("t"))
    assert infer_unit(node) is None


def test_infer_unit_pow_non_constant_exponent_raises():
    node = Pow(Quantity("x", LENGTH), Symbol("n"))
    with pytest.raises(ValueError, match="non-constant power"):
        infer_unit(node)


def test_infer_unit_transcendental_dimensionless_ok():
    assert infer_unit(Sin(Symbol("x"))) is None


def test_infer_unit_transcendental_dimensioned_raises():
    with pytest.raises(ValueError, match="dimensionless"):
        infer_unit(Cos(Quantity("x", LENGTH)))


def test_infer_unit_unknown_node_type_raises():
    with pytest.raises(TypeError, match="Unknown node type"):
        infer_unit(object())


def test_check_add_compat_ok_when_units_match():
    check_add_compat(Quantity("a", LENGTH), Quantity("c", LENGTH))


# --- differentiation (§4) ----------------------------------------------


def test_diff_add():
    x, y = Symbol("x"), Symbol("y")
    assert simplify(diff_node(Add((x, y)), "x")) == Number(1.0)


def test_diff_sub():
    x, y = Symbol("x"), Symbol("y")
    assert simplify(diff_node(Sub(x, y), "x")) == Number(1.0)


def test_diff_div():
    x, y = Symbol("x"), Symbol("y")
    expected = Div(
        Sub(Mul((Number(1.0), y)), Mul((x, Number(0.0)))),
        Pow(y, Number(2.0)),
    )
    assert diff_node(Div(x, y), "x") == expected


def test_diff_cos():
    x = Symbol("x")
    assert diff_node(Cos(x), "x") == Mul((Number(-1.0), Sin(x), Number(1.0)))


def test_diff_ln():
    x = Symbol("x")
    assert diff_node(Ln(x), "x") == Div(Number(1.0), x)


def test_diff_exp():
    x = Symbol("x")
    assert diff_node(Exp(x), "x") == Mul((Exp(x), Number(1.0)))


def test_diff_node_unknown_type_raises():
    with pytest.raises(TypeError, match="Unknown node type"):
        diff_node(object(), "x")


# --- depends_on ----------------------------------------------------------


def test_depends_on_quantity():
    assert depends_on(Quantity("x", LENGTH), "x") is True
    assert depends_on(Quantity("x", LENGTH), "y") is False


def test_depends_on_add():
    assert depends_on(Add((Symbol("x"), Symbol("y"))), "x") is True


def test_depends_on_unknown_type_raises():
    with pytest.raises(TypeError, match="Unknown node type"):
        depends_on(object(), "x")


# --- linear_coeff ----------------------------------------------------------


def test_linear_coeff_number():
    assert linear_coeff(Number(5.0), "x") == (0.0, 5.0)


def test_linear_coeff_leaf_wrong_name_is_none():
    assert linear_coeff(Symbol("y"), "x") is None


def test_linear_coeff_add_normal():
    node = Add((Symbol("x"), Number(3.0)))
    assert linear_coeff(node, "x") == (1.0, 3.0)


def test_linear_coeff_add_non_affine_term_is_none():
    node = Add((Symbol("x"), Pow(Symbol("x"), Number(2.0))))
    assert linear_coeff(node, "x") is None


def test_linear_coeff_sub_normal():
    node = Sub(Symbol("x"), Number(3.0))
    assert linear_coeff(node, "x") == (1.0, -3.0)


def test_linear_coeff_sub_non_affine_is_none():
    node = Sub(Symbol("x"), Pow(Symbol("x"), Number(2.0)))
    assert linear_coeff(node, "x") is None


def test_linear_coeff_mul_two_var_dependent_factors_is_none():
    node = Mul((Symbol("x"), Symbol("x")))
    assert linear_coeff(node, "x") is None


def test_linear_coeff_mul_non_affine_var_factor_is_none():
    node = Mul((Number(2.0), Pow(Symbol("x"), Number(2.0))))
    assert linear_coeff(node, "x") is None


def test_linear_coeff_mul_non_number_non_dependent_factor_is_none():
    node = Mul((Symbol("x"), Symbol("y")))
    assert linear_coeff(node, "x") is None


def test_linear_coeff_unhandled_type_is_none():
    assert linear_coeff(Pow(Symbol("x"), Number(2.0)), "x") is None


# --- _arg_form ----------------------------------------------------------


def test_arg_form_const():
    assert _arg_form(Number(5.0), "x") == ("const", 0.0)


def test_arg_form_none_when_not_affine():
    assert _arg_form(Pow(Symbol("x"), Number(2.0)), "x") is None


# --- integrate_sin / integrate_cos / integrate_exp / integrate_ln --------


def test_integrate_sin_linear():
    u = Mul((Number(2.0), Symbol("x")))
    expected = Div(Mul((Number(-1.0), Cos(u))), Number(2.0))
    assert integrate_sin(u, "x") == expected


def test_integrate_sin_const():
    u = Number(5.0)
    assert integrate_sin(u, "x") == Mul((Sin(u), Symbol("x")))


def test_integrate_cos_const():
    u = Number(5.0)
    assert integrate_cos(u, "x") == Mul((Cos(u), Symbol("x")))


def test_integrate_exp_var():
    u = Symbol("x")
    assert integrate_exp(u, "x") == Exp(u)


def test_integrate_exp_linear():
    u = Mul((Number(2.0), Symbol("x")))
    assert integrate_exp(u, "x") == Div(Exp(u), Number(2.0))


def test_integrate_exp_const():
    u = Number(5.0)
    assert integrate_exp(u, "x") == Mul((Exp(u), Symbol("x")))


def test_integrate_exp_non_affine_raises():
    u = Pow(Symbol("x"), Number(2.0))
    with pytest.raises(NotImplementedError):
        integrate_exp(u, "x")


def test_integrate_ln_var():
    u = Symbol("x")
    assert integrate_ln(u, "x") == Sub(Mul((u, Ln(u))), u)


def test_integrate_ln_const():
    u = Number(5.0)
    assert integrate_ln(u, "x") == Mul((Ln(u), Symbol("x")))


# --- integrate_pow ----------------------------------------------------------


def test_integrate_pow_linear_general():
    base = Mul((Number(2.0), Symbol("x")))
    result = integrate_pow(base, Number(3.0), "x")
    expected = Div(Pow(base, Number(4.0)), Number(2.0 * 4.0))
    assert result == expected


def test_integrate_pow_linear_reciprocal():
    base = Mul((Number(2.0), Symbol("x")))
    result = integrate_pow(base, Number(-1.0), "x")
    assert result == Div(Ln(base), Number(2.0))


def test_integrate_pow_const_base():
    base = Number(5.0)
    result = integrate_pow(base, Number(2.0), "x")
    assert result == Mul((Pow(base, Number(2.0)), Symbol("x")))


# --- u-substitution machinery ---------------------------------------------


def test_antiderivative_of_outer_sin():
    u = Symbol("u")
    assert _antiderivative_of_outer(Sin(u), u) == Mul((Number(-1.0), Cos(u)))


def test_antiderivative_of_outer_exp():
    u = Symbol("u")
    assert _antiderivative_of_outer(Exp(u), u) == Exp(u)


def test_antiderivative_of_outer_unhandled_is_none():
    u = Symbol("u")
    assert _antiderivative_of_outer(Ln(u), u) is None


def test_inner_arg_non_transcendental_is_none():
    assert _inner_arg(Symbol("x")) is None


def test_try_u_substitution_remaining_coeff_branch():
    result = _try_u_substitution(Number(1.0), Cos(Symbol("x")), "x", coeff=3.0)
    assert result == (Sin(Symbol("x")), 3.0)


def test_try_u_substitution_no_match_is_none():
    result = _try_u_substitution(Symbol("z"), Cos(Symbol("x")), "x", coeff=1.0)
    assert result is None


def test_const_coeff_non_number_is_none():
    assert _const_coeff([Number(2.0), Symbol("x")]) is None


# --- integrate_mul / integrate_div ------------------------------------------


def test_integrate_mul_single_with_numeric_const():
    result = integrate_mul((Number(3.0), Symbol("x")), "x")
    inner = integrate_node(Symbol("x"), "x")
    assert result == Mul((Number(3.0), inner))


def test_integrate_mul_single_with_non_numeric_const():
    result = integrate_mul((Symbol("y"), Symbol("x")), "x")
    inner = integrate_node(Symbol("x"), "x")
    assert result == Mul((Symbol("y"), inner))


def test_integrate_mul_no_var_dependent_factors():
    factors = (Number(2.0), Number(3.0))
    assert integrate_mul(factors, "x") == Mul((Mul(factors), Symbol("x")))


def test_integrate_mul_pair_no_pattern_matches_raises():
    with pytest.raises(NotImplementedError):
        integrate_mul((Symbol("x"), Ln(Symbol("x"))), "x")


def test_integrate_mul_pair_u_substitution_with_leftover_coeff():
    x_sq = Pow(Symbol("x"), Number(2.0))
    two_x = Mul((Number(2.0), Symbol("x")))
    factors = (Number(3.0), two_x, Cos(x_sq))
    result = integrate_mul(factors, "x")
    assert result == Mul(
        (Number(3.0), _antiderivative_of_outer(Cos(x_sq), x_sq))
    )


def test_integrate_div_denominator_independent_of_var():
    result = integrate_div(Symbol("x"), Number(5.0), "x")
    assert result == Div(integrate_node(Symbol("x"), "x"), Number(5.0))


def test_integrate_div_reciprocal_var():
    result = integrate_div(Number(1.0), Symbol("x"), "x")
    assert result == Ln(Symbol("x"))


def test_integrate_div_reciprocal_linear():
    b = Mul((Number(2.0), Symbol("x")))
    result = integrate_div(Number(1.0), b, "x")
    assert result == Div(Ln(b), Number(2.0))


def test_integrate_div_no_pattern_matches_raises():
    with pytest.raises(NotImplementedError):
        integrate_div(Symbol("z"), Symbol("x"), "x")


# --- integrate_node dispatch -------------------------------------------


def test_integrate_leaf_matching_var():
    result = integrate_node(Symbol("x"), "x")
    assert result == Div(Pow(Symbol("x"), Number(2.0)), Number(2.0))


def test_integrate_leaf_non_matching_var():
    result = integrate_node(Symbol("y"), "x")
    assert result == Mul((Symbol("y"), Symbol("x")))


def test_integrate_add_dispatch():
    node = Add((Symbol("x"), Number(2.0)))
    expected = Add(
        (integrate_node(Symbol("x"), "x"), integrate_node(Number(2.0), "x"))
    )
    assert integrate_node(node, "x") == expected


def test_integrate_sub_dispatch():
    node = Sub(Symbol("x"), Number(2.0))
    expected = Sub(
        integrate_node(Symbol("x"), "x"), integrate_node(Number(2.0), "x")
    )
    assert integrate_node(node, "x") == expected


def test_integrate_div_dispatch():
    node = Div(Symbol("x"), Number(5.0))
    assert integrate_node(node, "x") == integrate_div(
        Symbol("x"), Number(5.0), "x"
    )


def test_integrate_ln_dispatch():
    node = Ln(Symbol("x"))
    assert integrate_node(node, "x") == integrate_ln(Symbol("x"), "x")


def test_integrate_exp_dispatch():
    node = Exp(Symbol("x"))
    assert integrate_node(node, "x") == integrate_exp(Symbol("x"), "x")


def test_integrate_node_unknown_type_raises():
    with pytest.raises(TypeError, match="Unknown node type"):
        integrate_node(object(), "x")


# --- simplification (§3.1) ----------------------------------------------


def test_flatten_add_nested():
    nested = Add((Symbol("x"), Symbol("y")))
    result = flatten_add([nested, Symbol("z")])
    assert result == [Symbol("x"), Symbol("y"), Symbol("z")]


def test_flatten_mul_nested():
    nested = Mul((Symbol("x"), Symbol("y")))
    result = flatten_mul([nested, Symbol("z")])
    assert result == [Symbol("x"), Symbol("y"), Symbol("z")]


def test_collect_with_multiplicity_skips_non_matching_entries():
    result = _collect_with_multiplicity(
        [Number(1.0), Number(2.0), Number(2.0)]
    )
    assert result == [[Number(1.0), 1.0], [Number(2.0), 2.0]]


def test_simplify_add_single_term():
    assert simplify_add([Symbol("x")]) == Symbol("x")


def test_simplify_add_multiple_terms():
    result = simplify_add([Symbol("x"), Symbol("y")])
    assert result == Add((Symbol("x"), Symbol("y")))


def test_simplify_sub_rhs_zero():
    assert simplify_sub(Symbol("x"), Number(0.0)) == Symbol("x")


def test_simplify_sub_both_numbers():
    assert simplify_sub(Number(5.0), Number(2.0)) == Number(3.0)


def test_simplify_sub_fallback():
    assert simplify_sub(Symbol("x"), Symbol("y")) == Sub(
        Symbol("x"), Symbol("y")
    )


def test_simplify_mul_collects_and_folds():
    result = simplify_mul([Symbol("x"), Number(2.0), Symbol("x")])
    assert result == Mul((Number(2.0), Pow(Symbol("x"), Number(2.0))))


def test_simplify_div_rhs_one():
    assert simplify_div(Symbol("x"), Number(1.0)) == Symbol("x")


def test_simplify_div_both_numbers():
    assert simplify_div(Number(6.0), Number(2.0)) == Number(3.0)


def test_simplify_div_fallback():
    assert simplify_div(Symbol("x"), Symbol("y")) == Div(
        Symbol("x"), Symbol("y")
    )


def test_simplify_pow_both_numbers():
    assert simplify_pow(Number(2.0), Number(3.0)) == Number(8.0)


def test_simplify_pow_exponent_one():
    assert simplify_pow(Symbol("x"), Number(1.0)) == Symbol("x")


def test_simplify_pow_base_one():
    assert simplify_pow(Number(1.0), Symbol("n")) == Number(1.0)


def test_simplify_pow_fallback():
    assert simplify_pow(Symbol("x"), Symbol("n")) == Pow(
        Symbol("x"), Symbol("n")
    )


def test_simplify_exp_node():
    assert simplify(Exp(Symbol("x"))) == Exp(Symbol("x"))


def test_simplify_unknown_type_raises():
    with pytest.raises(TypeError, match="Unknown node type"):
        simplify(object())


# --- PyExpr wrapper --------------------------------------------------------


def test_pyexpr_exp_builder():
    assert PyExpr.exp(PyExpr.symbol("x")) == PyExpr(Exp(Symbol("x")))


def test_pyexpr_repr():
    assert repr(PyExpr.symbol("x")) == repr(Symbol("x"))


def test_pyexpr_eq_non_pyexpr_is_false():
    assert (PyExpr.symbol("x") == 5) is False


def test_pyexpr_hash():
    assert hash(PyExpr.symbol("x")) == hash(repr(Symbol("x")))


def test_add_mismatched_units_message_type():
    a = Quantity("a", LENGTH)
    b = Quantity("b", CompoundUnit({"mass": 1}))
    with pytest.raises(IncompatibleUnitsError):
        check_add_compat(a, b)
