"""Test suite for the UnitSystem's conversion and registration logic using
pytest.
"""

import pytest

from physure.domain.exceptions import (
    IncompatibleUnitsError,
    UnknownUnitError,
)
from physure.domain.measurement.converters import LinearConverter
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.units import CompoundUnit


@pytest.fixture
def dims():
    """Provides common dimensions."""
    return {
        "length": Dimension({"L": 1}),
        "time": Dimension({"T": 1}),
        "mass": Dimension({"M": 1}),
    }


def test_register_unit(system, dims):
    """Test registering units: the registry of the system holds them."""
    length = dims["length"]
    system.register_unit("m", length, LinearConverter(1.0), "meter")

    assert system.get_unit("m").dimension(system) == length

    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    registry = system.core_registry
    factor = registry.factor_between(
        registry.parse_atomic("m"), registry.parse_atomic("cm")
    )
    assert factor == 100.0


def test_find_dimension_for_unit(system, dims):
    """Test finding the dimension for a registered unit."""
    length = dims["length"]
    time = dims["time"]
    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")

    assert system.get_unit("m").dimension(system) == length
    assert system.get_unit("s").dimension(system) == time

    with pytest.raises(UnknownUnitError):
        system.get_unit("unknown_unit")


def test_compound_unit_conversion_factor(system, dims):
    """Test getting conversion factors between compound units."""
    length = dims["length"]
    time = dims["time"]
    mass = dims["mass"]

    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    system.register_unit("km", length, LinearConverter(1000.0), "kilometer")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("min", time, LinearConverter(60.0), "minute")
    system.register_unit("h", time, LinearConverter(3600.0), "hour")
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit("g", mass, LinearConverter(0.001), "gram")

    meter = CompoundUnit({"m": 1})
    centimeter = CompoundUnit({"cm": 1})
    assert meter.conversion_factor_to(centimeter, system) == 100.0

    velocity_mps = CompoundUnit({"m": 1, "s": -1})
    velocity_kmph = CompoundUnit({"km": 1, "h": -1})
    assert (
        pytest.approx(velocity_mps.conversion_factor_to(velocity_kmph, system))
        == 3.6
    )

    force_newton = CompoundUnit({"kg": 1, "m": 1, "s": -2})
    force_dyne = CompoundUnit({"g": 1, "cm": 1, "s": -2})
    assert (
        pytest.approx(force_newton.conversion_factor_to(force_dyne, system))
        == 1e5
    )

    # Test incompatible dimensions
    length_unit = CompoundUnit({"m": 1})
    time_unit = CompoundUnit({"s": 1})
    with pytest.raises(IncompatibleUnitsError):
        length_unit.conversion_factor_to(time_unit)


def test_exact_rational_conversion_factors():
    """Conversion factors defined exactly in .conf must not drift in float.

    0.3048 / 0.0254 in f64 is 12.000000000000002; exact rational
    arithmetic on the .conf-declared factors gives exactly 12.
    """
    from physure import Q_

    assert Q_(1.0, "ft").to("in").magnitude == 12.0
    assert Q_(1.0, "mi").to("in").magnitude == 63360.0
    assert Q_(1.0, "lb").to("oz").magnitude == 16.0


def test_exact_factors_survive_prefixes():
    """Prefixed units (built as prefix_factor * scale) keep an exact scale."""
    from fractions import Fraction

    from physure import Q_
    from physure.application.context import get_current_system

    assert Q_(1.0, "km").to("mm").magnitude == 1_000_000.0
    assert Q_(1.0, "kg").to("g").magnitude == 1000.0

    registry = get_current_system().core_registry

    def factor(unit):
        return registry.factor_between(
            registry.parse_atomic(unit), registry.parse_atomic("m")
        )

    assert factor("km") == 1000.0
    # prefix * imperial scale: milli-foot = 3048/10^7 m exactly
    assert factor("mft") == float(Fraction(3048, 10_000_000))


def test_fractional_exponent_falls_back_to_float():
    """Non-integer exponents can't use the exact path; float fallback."""
    from physure.application.context import get_current_system

    u = CompoundUnit({"km": (1, 2)})
    v = CompoundUnit({"m": (1, 2)})
    factor = u.conversion_factor_to(v, get_current_system())
    assert factor == pytest.approx(1000.0**0.5)
