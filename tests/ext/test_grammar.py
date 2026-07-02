import math

import pytest

from measurekit.domain.exceptions import (
    IncompatibleUnitsError,
    UnknownUnitError,
)
from measurekit.ext.grammar import GrammarError, GrammarInterpreter, evaluate


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
