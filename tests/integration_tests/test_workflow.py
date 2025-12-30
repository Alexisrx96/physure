"""Integration tests for complete workflows using pytest."""

import math

import pytest

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension


@pytest.fixture
def workflow_system(system):
    """Provides a system with custom units and dimensions for workflow
    tests.
    """
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    mass = Dimension({"M": 1})
    money = Dimension({"$": 1})

    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit("$", money, LinearConverter(1.0), "dollar")
    system.register_unit("h", time, LinearConverter(3600.0), "hour")
    system.register_unit("EUR", money, LinearConverter(1.1), "euro")

    system.register_alias({"m": 1, "s": -1}, "m/s", "velocity")
    system.register_alias({"$": 1, "h": -1}, "$/h", "hourly_rate")
    system.register_alias({"$": 1, "m": -1}, "$/m", "linear_cost")
    return system


def test_engineering_workflow(workflow_system):
    """Test an engineering workflow with material and cost calculations."""
    Q_ = workflow_system.Q_
    get_unit = workflow_system.get_unit

    density_steel = Q_(7850.0, get_unit("kg/m^3"))
    pipe_length = Q_(100.0, "m")
    pipe_diameter = Q_(0.1, "m")
    pipe_thickness = Q_(0.005, "m")

    outer_radius = pipe_diameter / 2
    inner_radius = outer_radius - pipe_thickness

    pipe_volume = math.pi * pipe_length * (outer_radius**2 - inner_radius**2)
    assert pipe_volume.unit.exponents == {"m": 3}

    pipe_mass = pipe_volume * density_steel
    steel_cost_per_kg = Q_(2.5, get_unit("$/kg"))
    material_cost = pipe_mass * steel_cost_per_kg
    assert material_cost.unit.exponents == {"$": 1}

    installation_rate = Q_(10.0, get_unit("m/h"))
    labor_cost_rate = Q_(25.0, get_unit("$/h"))
    installation_time = pipe_length / installation_rate
    labor_cost = installation_time * labor_cost_rate
    assert labor_cost.unit.exponents == {"$": 1}

    total_cost = material_cost + labor_cost
    total_cost_eur = total_cost.to("EUR")
    assert total_cost_eur.unit.exponents == {"EUR": 1}
    assert (
        pytest.approx(total_cost.magnitude / 1.1) == total_cost_eur.magnitude
    )


def test_physics_workflow(workflow_system):
    """Test a physics workflow with motion and energy calculations."""
    Q_ = workflow_system.Q_

    initial_velocity = Q_(0.0, "m/s")
    acceleration = Q_(9.8, "m/s^2")
    time_interval = Q_(5.0, "s")
    mass = Q_(2.0, "kg")

    final_velocity = initial_velocity + acceleration * time_interval
    assert final_velocity.unit.exponents == {"m": 1, "s": -1}

    kinetic_energy = 0.5 * mass * final_velocity**2
    assert kinetic_energy.unit.exponents == {"kg": 1, "m": 2, "s": -2}


def test_unit_error_handling(workflow_system):
    """Test error handling in operations with incompatible units."""
    Q_ = workflow_system.Q_

    length = Q_(10.0, "m")
    time = Q_(5.0, "s")

    with pytest.raises(IncompatibleUnitsError):
        _ = length + time

    with pytest.raises(IncompatibleUnitsError):
        length.to("s")

    with pytest.raises(IncompatibleUnitsError):
        _ = length < time
