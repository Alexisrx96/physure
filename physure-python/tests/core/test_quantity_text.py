"""The text of a quantity ("10 m/s") is parsed by the model, not by a factory."""

import pytest

from physure.domain.measurement.quantity_text import parse_quantity_text
from physure.domain.measurement.units import get_default_system


@pytest.fixture
def system():
    return get_default_system()


@pytest.mark.parametrize(
    ("text", "value", "unit"),
    [
        ("10 m", 10, "m"),
        ("2.5 kg", 2.5, "kg"),
        ("-3 m/s", -3, "m/s"),
        ("1e3 s", 1000.0, "s"),
        ("7", 7, "dimensionless"),
    ],
)
def test_the_number_and_the_unit_are_separated(system, text, value, unit):
    parsed_value, parsed_unit = parse_quantity_text(text, system)
    assert parsed_value == value
    assert type(parsed_value) is type(value)
    assert parsed_unit == system.get_unit(unit)


def test_text_without_a_number_is_left_alone(system):
    parsed = parse_quantity_text("m/s", system)
    assert parsed == ("m/s", system.get_unit("dimensionless"))


def test_the_factory_reads_the_same_text():
    from physure import Q_

    assert Q_("10 m/s") == Q_(10, "m/s")
    assert Q_("2.5 kg").magnitude == 2.5
    assert str(Q_("7").unit) == "1"
