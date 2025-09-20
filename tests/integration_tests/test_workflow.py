# tests/integration_tests/test_workflow.py (Refactored)

"""
Integration tests for complete workflows in MeasureKit after refactoring.
"""

import math
import unittest

from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.units import CompoundUnit
from tests.base_test_class import BaseTestUnit


class TestWorkflowIntegration(BaseTestUnit):
    """Tests for complete workflows from unit definition to calculations."""

    def setUp(self):
        """Set up a custom system of units and dimensions for each test."""
        super().setUp()
        # Create base dimensions for our custom system
        self.length = Dimension({"L": 1})
        self.time = Dimension({"T": 1})
        self.mass = Dimension({"M": 1})
        self.money = Dimension({"$": 1})

        # Register all units into the isolated self.system instance
        self.system.register_unit("m", self.length, 1.0, "meter")
        self.system.register_unit("s", self.time, 1.0, "second")
        self.system.register_unit("kg", self.mass, 1.0, "kilogram")
        self.system.register_unit("$", self.money, 1.0, "dollar")
        self.system.register_unit("h", self.time, 3600.0, "hour")
        self.system.register_unit("EUR", self.money, 1.1, "euro")

        # Register aliases for compound units
        CompoundUnit.register_alias({"m": 1, "s": -1}, "m/s", "velocity")
        CompoundUnit.register_alias({"$": 1, "h": -1}, "$/h", "hourly_rate")
        CompoundUnit.register_alias({"$": 1, "m": -1}, "$/m", "linear_cost")

    def test_engineering_workflow(self):
        """Test an engineering workflow with material and cost calculations."""
        # Use the system-specific factory and unit getter for all operations
        Q_ = self.system.Q_
        get_unit = self.system.get_unit

        # Define material properties
        density_steel = Q_(7850.0, get_unit("kg/m^3"))

        # Define project parameters
        pipe_length = Q_(100.0, "m")
        pipe_diameter = Q_(0.1, "m")
        pipe_thickness = Q_(0.005, "m")

        # Calculate pipe geometry
        outer_radius = pipe_diameter / 2
        inner_radius = outer_radius - pipe_thickness

        # Calculate volume of material
        pipe_volume = (
            math.pi * pipe_length * (outer_radius**2 - inner_radius**2)
        )
        self.assertEqual(pipe_volume.unit.exponents, {"m": 3})

        # Calculate mass and cost
        pipe_mass = pipe_volume * density_steel
        steel_cost_per_kg = Q_(2.5, get_unit("$/kg"))
        material_cost = pipe_mass * steel_cost_per_kg
        self.assertEqual(material_cost.unit.exponents, {"$": 1})

        # Calculate labor
        installation_rate = Q_(10.0, get_unit("m/h"))
        labor_cost_rate = Q_(25.0, get_unit("$/h"))
        installation_time = pipe_length / installation_rate
        labor_cost = installation_time * labor_cost_rate
        self.assertEqual(labor_cost.unit.exponents, {"$": 1})

        # Final calculations
        total_cost = material_cost + labor_cost
        total_cost_eur = total_cost.to("EUR")
        self.assertEqual(total_cost_eur.unit.exponents, {"EUR": 1})
        self.assertAlmostEqual(
            total_cost.magnitude / 1.1, total_cost_eur.magnitude
        )

    def test_physics_workflow(self):
        """Test a physics workflow with motion and energy calculations."""
        Q_ = self.system.Q_

        # Define initial conditions
        initial_velocity = Q_(0.0, "m/s")
        acceleration = Q_(9.8, "m/s^2")
        time_interval = Q_(5.0, "s")
        mass = Q_(2.0, "kg")

        # v = v₀ + at
        final_velocity = initial_velocity + acceleration * time_interval
        self.assertEqual(final_velocity.unit.exponents, {"m": 1, "s": -1})

        # KE = ½mv²
        kinetic_energy = 0.5 * mass * final_velocity**2
        self.assertEqual(
            kinetic_energy.unit.exponents, {"kg": 1, "m": 2, "s": -2}
        )

    def test_unit_error_handling(self):
        """Test error handling in operations with incompatible units."""
        Q_ = self.system.Q_

        length = Q_(10.0, "m")
        time = Q_(5.0, "s")

        # Test addition/subtraction with incompatible units
        with self.assertRaises(ValueError):
            _ = length + time

        # Test conversion between incompatible units
        with self.assertRaises(ValueError):
            length.to("s")

        # Test comparing quantities with different dimensions
        with self.assertRaises(ValueError):
            _ = length < time


if __name__ == "__main__":
    unittest.main()
