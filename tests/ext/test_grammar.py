import math

import pytest

from measurekit.domain.exceptions import (
    DimensionError,
    IncompatibleUnitsError,
    UnknownUnitError,
)
from measurekit.ext.grammar import (
    _FUNCTIONS,
    GrammarError,
    GrammarInterpreter,
    evaluate,
)


@pytest.fixture
def mn():
    return GrammarInterpreter()


def test_assignment_and_query(mn):
    mn.run("force = 500 N")
    result = mn.eval("force = ?")
    assert math.isclose(result.magnitude, 500)
    assert math.isclose(result.to("N").magnitude, 500)


def test_arrow_assignment_sugar(mn):
    mn.run("force -> 500 N")
    assert math.isclose(mn["force"].magnitude, 500)


def test_derived_calculation_and_assertion(mn):
    mn.run(
        """
        force = 500 N
        area = 2 m^2
        stress = force / area
        """
    )
    assert mn.eval("stress == 250 Pa") is True
    assert mn.eval("stress == 251 Pa") is False


def test_conversion(mn):
    mn.run("d = 1500 m")
    result = mn.eval("d => km")
    assert math.isclose(result.magnitude, 1.5)


def test_assignment_with_conversion(mn):
    mn.run("f = 500 N => kN")
    assert math.isclose(mn["f"].magnitude, 0.5)


def test_assignment_query_returns_value(mn):
    result = mn.eval("v = 10 m / 2 s = ?")
    assert math.isclose(result.magnitude, 5)
    assert math.isclose(mn["v"].magnitude, 5)


def test_implicit_mul_precedence(mn):
    # `500 N / 2 m^2` must parse as (500*N) / (2*m^2), not 500*N/2*m^2.
    result = mn.eval("500 N / 2 m^2 => Pa")
    assert math.isclose(result.magnitude, 250)


def test_compound_units_and_powers(mn):
    ke = mn.eval("0.5 * 2 kg * (3 m/s)^2 => J")
    assert math.isclose(ke.magnitude, 9.0)


def test_superscript_exponent(mn):
    result = mn.eval("2 m² == 2 m^2")
    assert result is True


def test_uncertainty_literal(mn):
    mn.run("g = 9.81 +/- 0.02 m/s^2")
    g = mn["g"]
    assert math.isclose(g.magnitude, 9.81)
    assert math.isclose(float(g.std_dev), 0.02, rel_tol=1e-6)


def test_comments_and_semicolons(mn):
    results = mn.run("a = 1 m; b = 2 m  # total\na + b = ?")
    assert math.isclose(results[-1].magnitude, 3)


def test_assignments_return_none(mn):
    assert mn.run("x = 5 m") == [None]


def test_bare_expression(mn):
    result = mn.eval("2 + 3")
    assert result == 5


def test_variables_shadow_units(mn):
    mn.run("m = 5 kg")  # shadows the meter
    assert math.isclose(mn["m"].magnitude, 5)


def test_unknown_unit_raises(mn):
    with pytest.raises(UnknownUnitError, match="furlonx"):
        mn.eval("x = 5 furlonx")


def test_incompatible_assertion_raises(mn):
    with pytest.raises(IncompatibleUnitsError):
        mn.eval("5 m == 5 s")


def test_invalid_assignment_target(mn):
    with pytest.raises(GrammarError):
        mn.eval("2 x = 5 m")


def test_unclosed_paren(mn):
    with pytest.raises(GrammarError):
        mn.eval("(2 + 3 m")


def test_one_shot_evaluate():
    assert evaluate("x = 2 m; y = 3 m; x * y == 6 m^2") is True


def test_negative_and_scientific(mn):
    result = mn.eval("-1.5e3 m => km")
    assert math.isclose(result.magnitude, -1.5)


def test_unicode_multiplication_and_division_operators(mn):
    assert mn.eval("2 × 3") == 6  # noqa: RUF001
    assert mn.eval("6 ÷ 2") == 3


def test_sqrt_unicode_prefix_parenthesized(mn):
    result = mn.eval("√(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_sqrt_unicode_prefix_bare(mn):
    mn.run("x = 16")
    assert math.isclose(mn.eval("√x"), 4)


def test_sqrt_ascii_function_form(mn):
    result = mn.eval("sqrt(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_abs_function(mn):
    result = mn.eval("abs(-3 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_abs_function_on_bare_number(mn):
    assert mn.eval("abs(-5)") == 5


def test_abs_function_identity_on_positive_quantity(mn):
    result = mn.eval("abs(3 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_abs_function_preserves_uncertainty(mn):
    result = mn.eval("abs(-3 +/- 0.1 m)")
    assert math.isclose(result.magnitude, 3)
    assert math.isclose(result.uncertainty, 0.1)


def test_abs_function_on_zero(mn):
    result = mn.eval("abs(0 m)")
    assert math.isclose(result.to("m").magnitude, 0)


def test_function_call_wrong_arity_raises(mn):
    with pytest.raises(GrammarError, match="abs"):
        mn.eval("abs(1 m, 2 m)")


def test_sqrt_function_still_works_after_migration(mn):
    # Regression: sqrt(...) used to be a hardcoded special case in _atom();
    # it now goes through the generic _FUNCTIONS dispatch table instead.
    result = mn.eval("sqrt(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_round_function(mn):
    result = mn.eval("round(3.7 m)")
    assert math.isclose(result.to("m").magnitude, 4)


def test_round_function_with_ndigits(mn):
    result = mn.eval("round(3.14159 m, 2)")
    assert math.isclose(result.to("m").magnitude, 3.14)


def test_round_function_wrong_arity_raises(mn):
    with pytest.raises(GrammarError, match="round"):
        mn.eval("round(3.14 m, 1, 2)")


def test_floor_function(mn):
    result = mn.eval("floor(3.7 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_ceil_function(mn):
    result = mn.eval("ceil(3.2 m)")
    assert math.isclose(result.to("m").magnitude, 4)


def test_floor_function_negative(mn):
    result = mn.eval("floor(-3.2 m)")
    assert math.isclose(result.to("m").magnitude, -4)


def test_ceil_function_negative(mn):
    result = mn.eval("ceil(-3.2 m)")
    assert math.isclose(result.to("m").magnitude, -3)


def test_min_function_cross_unit(mn):
    # 200 cm == 2 m, so the smaller of (3 m, 200 cm) is 200 cm/2 m.
    result = mn.eval("min(3 m, 200 cm)")
    assert math.isclose(result.to("m").magnitude, 2)


def test_max_function_cross_unit(mn):
    result = mn.eval("max(3 m, 200 cm)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_min_function_incompatible_units_raises(mn):
    with pytest.raises(IncompatibleUnitsError):
        mn.eval("min(3 m, 2 s)")


def test_min_function_variadic(mn):
    result = mn.eval("min(5 m, 1 m, 3 m)")
    assert math.isclose(result.to("m").magnitude, 1)


def test_max_function_variadic(mn):
    result = mn.eval("max(5 m, 1 m, 3 m)")
    assert math.isclose(result.to("m").magnitude, 5)


def test_sqrt_of_negative_returns_complex(mn):
    # Matches Python's own `(-4) ** 0.5` semantics: no unit involved, so
    # this is bare-number arithmetic, not a Quantity/GrammarError concern.
    result = mn.eval("sqrt(-4)")
    assert isinstance(result, complex)
    assert math.isclose(result.imag, 2.0)


_LOG_DOMAIN_ERROR = "domain error|positive input"


def test_log_of_zero_raises(mn):
    # Message wording varies by Python version ("math domain error" vs.
    # "expected a positive input" on 3.14+).
    with pytest.raises(ValueError, match=_LOG_DOMAIN_ERROR):
        mn.eval("log(0)")


def test_log_of_negative_raises(mn):
    with pytest.raises(ValueError, match=_LOG_DOMAIN_ERROR):
        mn.eval("log(-1)")


def test_sin_function_dimensionless(mn):
    assert math.isclose(mn.eval("sin(0)"), 0.0, abs_tol=1e-12)


def test_sin_function_angle_unit(mn):
    result = mn.eval("sin(90 deg)")
    assert math.isclose(result.magnitude, 1.0, abs_tol=1e-9)


def test_cos_function_angle_unit(mn):
    result = mn.eval("cos(0 rad)")
    assert math.isclose(result.magnitude, 1.0, abs_tol=1e-9)


def test_tan_function_dimensionless(mn):
    assert math.isclose(mn.eval("tan(0)"), 0.0, abs_tol=1e-12)


def test_sin_function_wrong_dimension_raises(mn):
    with pytest.raises(DimensionError):
        mn.eval("sin(3 kg)")


def test_exp_function_dimensionless(mn):
    assert math.isclose(mn.eval("exp(0)"), 1.0)


def test_log_function_dimensionless(mn):
    assert math.isclose(mn.eval("log(1)"), 0.0, abs_tol=1e-12)


def test_ln_function_is_natural_log(mn):
    # ln and log are the same implementation (no log10 exists in the engine).
    assert mn.eval("ln(1)") == mn.eval("log(1)")


def test_log_function_wrong_dimension_raises(mn):
    with pytest.raises(DimensionError):
        mn.eval("log(3 kg)")


@pytest.mark.parametrize("name", sorted(_FUNCTIONS))
def test_function_names_are_reserved_assignment_targets(mn, name):
    with pytest.raises(GrammarError):
        mn.eval(f"{name} = 5 m")
