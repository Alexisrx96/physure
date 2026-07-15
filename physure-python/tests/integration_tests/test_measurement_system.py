"""Integration tests for the MeasureKit measurement system using pytest."""

import math

import pytest

from physure.domain.exceptions import IncompatibleUnitsError


@pytest.fixture
def integrated_system(common_system):
    """Provides a system with common units and aliases."""
    common_system.register_alias({"m": 1, "s": -1}, "velocity")
    common_system.register_alias({"kg": 1, "m": 1, "s": -2}, "newton")
    common_system.register_alias({"kg": 1, "m": 2, "s": -2}, "joule")
    return common_system


def test_unit_creation_and_conversion(integrated_system):
    """Test creating units and converting between them."""
    meter = integrated_system.get_unit("m")
    centimeter = integrated_system.get_unit("cm")
    kilometer = integrated_system.get_unit("km")

    assert meter.conversion_factor_to(centimeter) == 100.0
    assert kilometer.conversion_factor_to(meter) == 1000.0


def test_quantity_creation_and_conversion(integrated_system):
    """Test creating quantities and converting them."""
    length1 = integrated_system.Q_(5.0, "m")
    length2 = integrated_system.Q_(300.0, "cm")

    length2_m = length2.to("m")
    assert length2_m.magnitude == 3.0
    assert length2_m.unit == integrated_system.get_unit("m")
    assert length1.dimension == length2.dimension


def test_quantity_arithmetic(integrated_system):
    """Test arithmetic operations with quantities."""
    length1 = integrated_system.Q_(5.0, "m")
    length2 = integrated_system.Q_(300.0, "cm")
    time = integrated_system.Q_(2.0, "s")

    total_length_m = length1 + length2
    assert total_length_m.magnitude == 8.0
    assert total_length_m.unit == integrated_system.get_unit("m")

    velocity = length1 / time
    assert velocity.magnitude == 2.5
    assert velocity.unit.exponents == {"m": 1, "s": -1}


def test_dimension_consistency(integrated_system):
    """Test that dimension consistency is maintained."""
    length = integrated_system.Q_(5.0, "m")
    time = integrated_system.Q_(2.0, "s")

    with pytest.raises(IncompatibleUnitsError):
        _ = length + time


def test_end_to_end_calculation(integrated_system):
    """Test a complete physics calculation end-to-end."""
    mass = integrated_system.Q_(75.0, "kg")
    height = integrated_system.Q_(10.0, "m")
    g = integrated_system.Q_(9.81, "m/s^2")

    # E = m*g*h
    potential_energy = mass * g * height
    assert pytest.approx(potential_energy.magnitude) == 7357.5
    assert potential_energy.unit.exponents == {"kg": 1, "m": 2, "s": -2}

    # Convert to the aliased "joule" unit
    energy_in_joules = potential_energy.to("J")
    assert pytest.approx(energy_in_joules.magnitude) == 7357.5
    assert energy_in_joules.unit == integrated_system.get_unit("J")

    # t = sqrt(2*h/g)
    time_to_fall = (2 * height / g) ** 0.5
    assert pytest.approx(time_to_fall.magnitude) == math.sqrt(2 * 10 / 9.81)
    assert time_to_fall.unit.exponents == {"s": 1.0}
