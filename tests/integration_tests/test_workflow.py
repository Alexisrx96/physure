"""Integration tests for complete workflows in MeasureKit.

These tests verify that all components work together correctly in real-world scenarios.
"""

import math
import unittest

from measurement.conversions import (
    UNIT_DIMENSIONS,
    UNIT_REGISTRY,
    register_unit,
)
from measurement.dimensions import Dimension
from measurement.quantity import Quantity
from measurement.units import CompoundUnit, get_unit

from tests.base_test_class import BaseTestUnit      

class TestWorkflowIntegration(BaseTestUnit):
    """Tests for complete workflows from unit definition to calculations."""

    def setUp(self):
        """Set up a custom system of units and dimensions."""
        # Create base dimensions for a custom system
        self.length = Dimension({"L": 1})
        self.time = Dimension({"T": 1})
        self.mass = Dimension({"M": 1})
        self.money = Dimension({"$": 1})

        # Register base units
        register_unit("m", self.length, 1.0, "meter")
        register_unit("s", self.time, 1.0, "second")
        register_unit("kg", self.mass, 1.0, "kilogram")
        register_unit("$", self.money, 1.0, "dollar")

        # Register derived units
        register_unit("ft", self.length, 0.3048, "foot")
        register_unit("min", self.time, 60.0, "minute")
        register_unit("h", self.time, 3600.0, "hour")
        register_unit("EUR", self.money, 1.1, "euro")  # Example exchange rate

        # Register common compound units
        CompoundUnit.register_alias({"m": 1, "s": -1}, "m/s", "velocity")
        CompoundUnit.register_alias({"$": 1, "h": -1}, "$/h", "hourly_rate")
        CompoundUnit.register_alias({"$": 1, "m": -1}, "$/m", "linear_cost")


    def test_engineering_workflow(self):
        """Test an engineering workflow with material and cost calculations."""
        # Define material properties
        density_steel = Quantity(7850.0, get_unit("kg/m³"))  # Steel density

        # Define project parameters
        pipe_length = Quantity(100.0, get_unit("m"))
        pipe_diameter = Quantity(0.1, get_unit("m"))  # 10cm diameter
        pipe_thickness = Quantity(0.005, get_unit("m"))  # 5mm wall thickness

        # Calculate pipe geometry
        outer_radius = pipe_diameter / 2
        inner_radius = outer_radius - pipe_thickness

        # Calculate volume of material (πL(R²-r²))
        pipe_volume = (
            math.pi * pipe_length * (outer_radius**2 - inner_radius**2)
        )

        # Verify pipe_volume has the correct unit
        self.assertEqual(pipe_volume.unit.exponents, {"m": 3})

        # Calculate mass of pipe
        pipe_mass = pipe_volume * density_steel
        self.assertEqual(pipe_mass.unit.exponents, {"kg": 1})

        # Define material cost
        steel_cost_per_kg = Quantity(2.5, get_unit("$/kg"))

        # Calculate material cost
        material_cost = pipe_mass * steel_cost_per_kg
        self.assertEqual(material_cost.unit.exponents, {"$": 1})

        # Define labor parameters
        installation_rate = Quantity(10.0, get_unit("m/h"))  # Meters per hour
        labor_cost_rate = Quantity(25.0, get_unit("$/h"))  # Hourly rate

        # Calculate installation time
        installation_time = pipe_length / installation_rate
        self.assertEqual(installation_time.unit.exponents, {"h": 1})

        # Calculate labor cost
        labor_cost = installation_time * labor_cost_rate
        self.assertEqual(labor_cost.unit.exponents, {"$": 1})

        # Calculate total project cost
        total_cost = material_cost + labor_cost
        self.assertEqual(total_cost.unit.exponents, {"$": 1})

        # Convert to different currency
        total_cost_eur = total_cost.to("EUR")
        self.assertEqual(total_cost_eur.unit.exponents, {"EUR": 1})

    def test_physics_workflow(self):
        """Test a physics workflow with motion and energy calculations."""
        # Define initial conditions
        initial_velocity = Quantity(0.0, get_unit("m/s"))
        acceleration = Quantity(9.8, get_unit("m/s²"))
        time_interval = Quantity(5.0, get_unit("s"))
        mass = Quantity(2.0, get_unit("kg"))

        # Calculate final velocity using v = v₀ + at
        final_velocity = initial_velocity + acceleration * time_interval
        self.assertEqual(final_velocity.unit.exponents, {"m": 1, "s": -1})

        # Calculate distance traveled using s = v₀t + ½at²
        distance = (
            initial_velocity * time_interval
            + 0.5 * acceleration * time_interval**2
        )
        self.assertEqual(distance.unit.exponents, {"m": 1})

        # Calculate kinetic energy using KE = ½mv²
        kinetic_energy = 0.5 * mass * final_velocity**2
        self.assertEqual(
            kinetic_energy.unit.exponents, {"kg": 1, "m": 2, "s": -2}
        )

        # Calculate work done using W = F·d = m·a·d
        work_done = mass * acceleration * distance
        self.assertEqual(work_done.unit.exponents, {"kg": 1, "m": 2, "s": -2})

        # Verify that work equals change in kinetic energy
        self.assertLess(abs(work_done.value - kinetic_energy.value), 1e-10)

        # Calculate power as P = W/t
        power = work_done / time_interval
        self.assertEqual(power.unit.exponents, {"kg": 1, "m": 2, "s": -3})

    def test_unit_error_handling(self):
        """Test error handling in operations with incompatible units."""
        # Create quantities with different dimensions
        length = Quantity(10.0, get_unit("m"))
        time = Quantity(5.0, get_unit("s"))
        mass = Quantity(2.0, get_unit("kg"))
        money = Quantity(100.0, get_unit("$"))

        # Test addition/subtraction with incompatible units
        with self.assertRaises(ValueError):
            length + time

        with self.assertRaises(ValueError):
            mass - money

        # Test conversion between incompatible units
        with self.assertRaises(ValueError):
            length.to("s")

        with self.assertRaises(ValueError):
            money.to("kg")

        # Test comparing quantities with different dimensions
        with self.assertRaises(ValueError):
            # This expression will cause a ValueError because you can't compare different dimensions
            self.assertTrue(length < time)

        with self.assertRaises(ValueError):
            # This expression will cause a ValueError because you can't compare different dimensions
            self.assertTrue(mass == money)

        # Verify that compatible operations work
        length_ft = Quantity(50.0, get_unit("ft"))
        sum_length = length + length_ft.to("m")
        self.assertEqual(sum_length.unit.exponents, {"m": 1})

        # Verify that dimensionally consistent operations work
        velocity = length / time
        self.assertEqual(velocity.unit.exponents, {"m": 1, "s": -1})

        force = mass * length / (time**2)
        self.assertEqual(force.unit.exponents, {"kg": 1, "m": 1, "s": -2})


if __name__ == "__main__":
    unittest.main()
