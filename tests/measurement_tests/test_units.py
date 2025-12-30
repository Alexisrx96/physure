"""Test suite for the CompoundUnit class and get_unit function using pytest."""

import pytest

from measurekit import get_unit
from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.units import CompoundUnit


@pytest.fixture
def unit_system(system):
    """Set up test fixtures for unit tests."""
    # Aliases are registered on the instance
    system.register_alias({"m": 1, "s": -1}, "velocity", "speed")

    # Register units into our isolated test system
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    mass = Dimension({"M": 1})
    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    system.register_unit("km", length, LinearConverter(1000.0), "kilometer")
    return system


def test_init_and_new():
    """Test initialization and __new__ caching behavior."""
    unit1 = CompoundUnit({"m": 1})
    assert unit1.exponents == {"m": 1}

    unit2 = CompoundUnit({"m": 1})
    assert unit1 is unit2


def test_arithmetic_operations():
    """Test arithmetic operations between units."""
    meter = CompoundUnit({"m": 1})
    second = CompoundUnit({"s": 1})
    kilogram = CompoundUnit({"kg": 1})

    velocity = meter / second
    assert velocity.exponents == {"m": 1, "s": -1}

    area = meter**2
    assert area.exponents == {"m": 2}

    force = kilogram * meter / (second**2)
    assert force.exponents == {"kg": 1, "m": 1, "s": -2}


def test_dimension(unit_system):
    """Test dimension calculation, which now requires a system."""
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})

    meter = CompoundUnit({"m": 1})
    assert meter.dimension(unit_system) == length

    velocity = CompoundUnit({"m": 1, "s": -1})
    assert velocity.dimension(unit_system) == length / time

    # Test with an unknown unit
    with pytest.raises(ValueError):
        CompoundUnit({"unknown_unit": 1}).dimension(unit_system)


def test_conversion_methods(unit_system):
    """Test methods for unit conversion, which now require a system."""
    meter = CompoundUnit({"m": 1})
    centimeter = CompoundUnit({"cm": 1})
    kilometer = CompoundUnit({"km": 1})

    # Test conversion factor calculation using the system
    assert meter.conversion_factor_to(centimeter) == 100.0
    assert centimeter.conversion_factor_to(meter) == 0.01
    assert kilometer.conversion_factor_to(meter) == 1000.0


def test_get_unit_simple():
    """Test get_unit with simple expressions."""
    # Note: get_unit uses the default_system, which should have basic units.
    assert get_unit("m").exponents == {"m": 1}
    assert get_unit("kg").exponents == {"kg": 1}
    assert get_unit("m/s").exponents == {"m": 1, "s": -1}


def test_get_unit_complex():
    """Test get_unit with complex expressions."""
    assert get_unit("(kg*m)/s^2").exponents == {"kg": 1, "m": 1, "s": -2}
