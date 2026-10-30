"""A1 stage 2, step 6: a unit string is read by the core's parser, and by no other."""

import subprocess
import sys
import textwrap

import pytest

from physure import Q_
from physure.application.parsing import parse_unit_string
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.units import CompoundUnit


@pytest.mark.parametrize(
    ("power", "caret"),
    [
        ("m**2", "m^2"),
        ("kg*m/s**2", "kg*m/s^2"),
        ("J/(mol*K)", "J/(mol*K)"),
        ("m/s**-1", "m/s^-1"),
    ],
)
def test_the_python_power_is_the_power(power, caret):
    assert parse_unit_string(power, CompoundUnit) == parse_unit_string(
        caret, CompoundUnit
    )


def test_a_decimal_exponent_is_the_exact_fraction():
    unit = parse_unit_string("s**0.5", CompoundUnit)
    assert unit.exponents == {"s": 0.5}
    assert Q_(1.0, "s**0.5").dimension == Dimension({"T": 0.5})
    assert Q_(1.0, "m^2.25").dimension == Dimension({"L": 2.25})
    assert Q_(1.0, "V/Hz**0.5").dimension == Dimension(
        {"L": 2, "M": 1, "T": -2.5, "I": -1}
    )


def test_a_string_that_is_not_a_unit_is_a_value_error():
    for text in ("* /", "m**", "m^^2", "m**2."):
        with pytest.raises(ValueError, match="Parsing failed"):
            parse_unit_string(text, CompoundUnit)


def test_reading_a_unit_does_not_import_sympy():
    # a fresh interpreter: whether an earlier test imported sympy does not matter
    code = textwrap.dedent(
        """
        import sys
        from physure import Q_

        for text in ("m/s**2", "kg*m/s**2", "s**0.5", "J/(mol*K)", "m^2"):
            Q_(1.0, text)
        assert "sympy" not in sys.modules
        """
    )
    done = subprocess.run(
        [sys.executable, "-I", "-c", code], capture_output=True, text=True
    )
    assert done.returncode == 0, done.stderr
