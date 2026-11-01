"""One rule for how a unit or a dimension is written, and what is written can be read back.

A whole power is superscript digits and a fractional power is the exact fraction in parentheses. Python used
to format powers on its own (a `Fraction` as its decimal: a third was `s⁰⋅³³³³³³³³³³³³³³³`, which the unit parser
refuses); `CompoundUnit` and `Dimension` now ask the core.
"""

import pytest

from physure import Q_
from physure._core import formula_text, unit_facts, unit_text
from physure.domain.measurement.dimensions import Dimension

FRACTIONAL = [
    "m^0.5",
    "s^(1/3)",
    "kg^0.25*m/s^0.5",
    "m^(-3/2)",
    "s^-0.5",
    "um^(1/3)",
    "GHz^0.5",
]
WHOLE = ["m", "m/s", "kg*m^2/s^2", "N", "J", "W", "1/s", "km/h", "m^2*s^-1"]


@pytest.mark.parametrize(
    ("expression", "written"),
    [
        ("m^0.5", "m^(1/2)"),
        ("s^(1/3)", "s^(1/3)"),
        ("s^-0.5", "1/s^(1/2)"),
        ("kg^0.25*m/s^0.5", "kg^(1/4)·m/s^(1/2)"),
        ("m^2", "m²"),
        ("kg*m^2/s^2", "kg·m²/s²"),
        ("m^2*s^-1", "m²/s"),
    ],
)
def test_a_unit_is_written_by_the_one_rule(expression, written):
    assert str(Q_(1, expression).unit) == written


def test_a_third_is_not_a_decimal():
    text = str(Q_(1, "s^(1/3)").unit)
    assert "⋅" not in text
    assert str(Q_(1, "s^(1/3)").dimension) == "T^(1/3)"


@pytest.mark.parametrize("expression", FRACTIONAL + WHOLE)
def test_what_a_unit_is_written_as_reads_back_as_the_same_unit(expression):
    unit = Q_(1, expression).unit
    again = Q_(1, str(unit)).unit
    assert again == unit
    # the same quantity, to the last bit, in the unit it was read back in
    assert Q_(2.5, expression).to(str(unit)).magnitude == pytest.approx(
        2.5, rel=1e-12
    )


@pytest.mark.parametrize("expression", FRACTIONAL + WHOLE)
def test_the_unit_and_the_core_write_the_same_text(expression):
    # the core's simplified form is the text of a unit made of symbols
    assert str(Q_(1, expression).unit) == unit_facts(expression)["simplified"]


def test_the_python_classes_ask_the_core_and_do_not_format_powers_themselves():
    assert unit_text([("m", (1, 2))]) == "m^(1/2)"
    assert unit_text([("kg", (1, 1)), ("s", (-2, 1))]) == "kg/s²"
    assert formula_text([("L", (2, 1)), ("T", (-1, 3))]) == "L²·T^(-1/3)"
    assert str(Dimension({"L": 2, "T": (-1, 3)})) == formula_text(
        [("L", (2, 1)), ("T", (-1, 3))]
    )


def test_a_dimension_writes_its_powers_as_a_unit_does():
    dimension = Q_(1, "kg^0.25*m/s^0.5").dimension
    assert str(dimension) == "L·M^(1/4)·T^(-1/2)"
    assert str(Q_(1, "m^2/s").dimension) == "L²·T⁻¹"


def test_a_unit_prints_as_its_symbols_and_a_name_is_asked_for():
    work = (Q_(2, "N") * Q_(3, "m")).unit
    assert str(work) == "kg·m²/s²"
    assert work.to_string() == "kg·m²/s²"
    # the name the system gives it, which `use_alias` always promised and needed an explicit system to get
    assert work.to_string(use_alias=True) == "J"
    assert format(work, "alias") == "J"
    assert format(work, "") == "kg·m²/s²"
    # a unit with no name of its own is its symbols
    assert Q_(1, "m^0.5").unit.to_string(use_alias=True) == "m^(1/2)"


def test_a_quantity_prints_the_name_of_its_unit():
    assert str(Q_(2, "N") * Q_(3, "m")) == "6.0 J"


@pytest.mark.parametrize("prefix", ["\u00b5", "\u03bc", "u"])
def test_the_micro_sign_is_the_prefix_u(prefix):
    # MICRO SIGN (what a keyboard types), GREEK SMALL LETTER MU, and u
    assert Q_(1, prefix + "m").to("m").magnitude == pytest.approx(1e-6)
    assert Q_(1, prefix + "s").to("s").magnitude == pytest.approx(1e-6)
    assert Q_(1, prefix + "g").to("kg").magnitude == pytest.approx(1e-9)
