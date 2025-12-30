"""Test suite for the UnitSystem's conversion and registration logic using
pytest.
"""

import pytest

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.conversions import UnitDefinition
from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.units import CompoundUnit


@pytest.fixture
def dims():
    """Provides common dimensions."""
    return {
        "length": Dimension({"L": 1}),
        "time": Dimension({"T": 1}),
        "mass": Dimension({"M": 1}),
    }


def test_unit_definition_initialization_and_caching(dims):
    """Test initialization and caching behavior of UnitDefinition."""
    length = dims["length"]
    unit1 = UnitDefinition("m", length, LinearConverter(1.0), "meter")
    assert unit1.symbol == "m"
    assert unit1.dimension == length
    assert unit1.name == "meter"

    unit2 = UnitDefinition("m", length, LinearConverter(1.0), "meter")
    assert unit1 is unit2


def test_register_unit(system, dims):
    """Test registering units in the system's registries."""
    length = dims["length"]
    system.register_unit("m", length, LinearConverter(1.0), "meter")

    assert "m" in system.UNIT_REGISTRY[length]
    assert system.UNIT_DIMENSIONS["m"] == length

    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    assert "cm" in system.UNIT_REGISTRY[length]


def test_find_dimension_for_unit(system, dims):
    """Test finding the dimension for a registered unit."""
    length = dims["length"]
    time = dims["time"]
    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")

    assert system.UNIT_DIMENSIONS["m"] == length
    assert system.UNIT_DIMENSIONS["s"] == time

    with pytest.raises(KeyError):
        _ = system.UNIT_DIMENSIONS["unknown_unit"]


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
    assert meter.conversion_factor_to(centimeter) == 100.0

    velocity_mps = CompoundUnit({"m": 1, "s": -1})
    velocity_kmph = CompoundUnit({"km": 1, "h": -1})
    assert (
        pytest.approx(velocity_mps.conversion_factor_to(velocity_kmph)) == 3.6
    )

    force_newton = CompoundUnit({"kg": 1, "m": 1, "s": -2})
    force_dyne = CompoundUnit({"g": 1, "cm": 1, "s": -2})
    assert pytest.approx(force_newton.conversion_factor_to(force_dyne)) == 1e5

    # Test incompatible dimensions
    length_unit = CompoundUnit({"m": 1})
    time_unit = CompoundUnit({"s": 1})
    with pytest.raises(IncompatibleUnitsError):
        length_unit.conversion_factor_to(time_unit)
