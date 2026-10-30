"""A1 stage 2, step 2b: what a unit system registers, written as conf text."""

from fractions import Fraction

import pytest

from physure._core import UnitRegistry
from physure.domain.measurement.definitions_text import (
    prefix_definition_text,
    unit_definition_text,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.units import CompoundUnit

MONEY = Dimension({"$": 1})
POWER = Dimension({"M": 1, "L": 2, "T": -3})


def _the_core_takes(text):
    registry = UnitRegistry()
    registry.extend_conf(text)
    return registry


def test_a_scaled_unit_is_written_with_its_names():
    text = unit_definition_text(
        "USD",
        MONEY,
        1.0,
        ["dollar", "USD"],
        allow_prefixes=False,
    )
    assert text == "[Units]\nUSD = 1.0, $, noprefix, [USD, dollar]\n"
    assert _the_core_takes(text).facts("dollar")["dimension"] == {"$": 1}


def test_a_scale_that_is_omitted_is_one():
    text = unit_definition_text("m", Dimension({"L": 1}))
    assert text == "[Units]\nm = 1.0, L, [m]\n"


@pytest.mark.parametrize(
    ("scale", "written"),
    [
        (Fraction(5, 9), "5/9"),
        ("5/9", "5/9"),
        ("0.3048", "381/1250"),
        (3, "3"),
    ],
)
def test_an_exact_scale_is_written_as_the_fraction(scale, written):
    text = unit_definition_text("F", Dimension({"O": 1}), scale)
    assert text == f"[Units]\nF = {written}, O, [F]\n"
    _the_core_takes(text)


def test_a_unit_with_a_zero_point_is_an_absolute_scale():
    text = unit_definition_text(
        "degX", Dimension({"O": 1}), 1.0, offset=Fraction(5463, 20)
    )
    assert text == "[Units]\ndegX = 1.0, 5463/20, O, [degX]\n"
    assert _the_core_takes(text).facts("degX")["kind"] == "absolute"
    # a zero point written as a decimal is the same fraction
    assert (
        unit_definition_text("degX", Dimension({"O": 1}), 1.0, offset="273.15")
        == text
    )


def test_a_zero_that_is_not_a_zero_point_leaves_a_difference():
    text = unit_definition_text("x", Dimension({"O": 1}), 2.0, offset=0)
    assert text == "[Units]\nx = 2.0, O, [x]\n"
    assert _the_core_takes(text).facts("x")["kind"] == "delta"


def test_a_level_of_a_quantity():
    text = unit_definition_text(
        "dBW", POWER, level=(10.0, 1.0), allow_prefixes=False
    )
    assert (
        text == "[Units]\ndBW = Log, 10.0, 1.0, L^2*M*T^-3, noprefix, [dBW]\n"
    )
    facts = _the_core_takes(text).facts("dBW")
    assert facts["dimension"] == {"L": 2, "M": 1, "T": -3}
    assert facts["is_linear"] is False


def test_a_recipe_is_written_for_a_derived_unit_only():
    energy = Dimension({"M": 1, "L": 2, "T": -2})
    recipe = CompoundUnit({"kg": 1, "m": 2, "s": -2})
    text = unit_definition_text("J", energy, 1.0, ["joule"], recipe=recipe)
    assert text == "[Units]\nJ = 1.0, L^2*M*T^-2, kg*m^2*s^-2, [J, joule]\n"
    _the_core_takes(text)
    # a recipe next to a dimensionless "1" would read as a zero point of 1
    unity = unit_definition_text(
        "one", Dimension({}), 1.0, recipe=CompoundUnit({})
    )
    assert unity == "[Units]\none = 1.0, 1, [one]\n"
    _the_core_takes(unity)


@pytest.mark.parametrize("name", ["", "a,b", "a=b", "a[b", " a", "#a", "a\nb"])
def test_a_name_a_conf_cannot_hold_is_refused(name):
    with pytest.raises(ValueError, match="cannot be a name"):
        unit_definition_text(name, Dimension({"L": 1}), 1.0)


@pytest.mark.parametrize("number", [float("inf"), float("nan")])
def test_a_number_that_is_not_finite_is_refused(number):
    with pytest.raises(ValueError, match="cannot be written"):
        unit_definition_text("x", Dimension({"L": 1}), number)
    with pytest.raises(ValueError, match="cannot be written"):
        unit_definition_text("x", Dimension({"O": 1}), 1.0, offset=number)


@pytest.mark.parametrize("number", ["", "abc", "1/0", "one"])
def test_a_text_that_is_not_a_number_is_refused(number):
    with pytest.raises(ValueError, match="is not a number"):
        unit_definition_text("x", Dimension({"L": 1}), number)


def test_a_level_has_no_scale_no_zero_point_and_no_recipe():
    with pytest.raises(ValueError, match="no scale"):
        unit_definition_text("dBx", POWER, 2.0, level=(10.0, 1.0))
    with pytest.raises(ValueError, match="no zero point"):
        unit_definition_text("dBx", POWER, level=(10.0, 1.0), offset=1.0)
    with pytest.raises(ValueError, match="no recipe"):
        unit_definition_text(
            "dBx", POWER, level=(10.0, 1.0), recipe=CompoundUnit({"W": 1})
        )


def test_a_unit_with_a_zero_point_has_no_recipe():
    with pytest.raises(ValueError, match="zero point has no recipe"):
        unit_definition_text(
            "x",
            Dimension({"O": 1}),
            1.0,
            offset=5.0,
            recipe=CompoundUnit({"K": 1}),
        )


def test_a_prefix_keeps_its_exact_fraction():
    text = prefix_definition_text("th", 1 / 3, "third", Fraction(1, 3))
    assert text == "[Prefixes]\nthird = th, 1/3\n"
    registry = _the_core_takes(text + "[Units]\nmetre = 1.0, L, [m, metre]\n")
    factor = registry.factor_between(
        registry.parse_atomic("thm"), registry.parse_atomic("m")
    )
    assert factor == 1 / 3
    assert prefix_definition_text("k", 1000.0) == "[Prefixes]\nk = k, 1000.0\n"
