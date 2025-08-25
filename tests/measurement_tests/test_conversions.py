"""Test suite for the conversions module."""
import unittest

from measurement.conversions import (
    UNIT_DIMENSIONS,
    UNIT_REGISTRY,
    UnitDefinition,
    compound_factor,
    find_dimension_for_unit,
    convert_compound_unit,
    get_conversion_factor,
    register_unit,
)
from measurement.dimensions import Dimension
from measurement.units import CompoundUnit

from tests.base_test_class import BaseTestUnit

class TestUnitDefinition(BaseTestUnit):
    """Tests for the UnitDefinition class."""

    def test_initialization_and_caching(self):
        """Test initialization and caching behavior."""
        # Create a dimension
        length = Dimension({"L": 1})
        
        # Create a unit definition
        unit1 = UnitDefinition("m", length, 1.0, "meter")
        self.assertEqual(unit1.symbol, "m")
        self.assertEqual(unit1.dimension, length)
        self.assertEqual(unit1.factor_to_base, 1.0)
        self.assertEqual(unit1.name, "meter")
        
        # Test caching - creating the same unit returns the same instance
        unit2 = UnitDefinition("m", length, 1.0, "meter")
        self.assertIs(unit1, unit2)
        
        # Different parameters should create different instances
        unit3 = UnitDefinition("cm", length, 0.01, "centimeter")
        self.assertIsNot(unit1, unit3)

    def test_string_representation(self):
        """Test string representation methods."""
        length = Dimension({"L": 1})
        unit = UnitDefinition("m", length, 1.0, "meter")
        
        # String representation should include essential information
        self.assertEqual(str(unit), "UnitDefinition(m, L, 1.0)")
        
        # Repr should be more detailed and include the name
        self.assertEqual(repr(unit), "UnitDefinition(m, L, 1.0, meter)")


class TestUnitRegistry(unittest.TestCase):
    """Tests for unit registration and lookup."""

    def setUp(self):
        """Set up test dimensions."""
        self.length = Dimension({"L": 1})
        self.time = Dimension({"T": 1})
        self.mass = Dimension({"M": 1})

    def test_register_unit(self):
        """Test registering units in the registry."""
        # Register a unit
        register_unit("m", self.length, 1.0, "meter")
        
        # Check it's in the registry
        self.assertIn("m", UNIT_REGISTRY[self.length])
        self.assertEqual(UNIT_DIMENSIONS["m"], self.length)
        
        # Register another unit of the same dimension
        register_unit("cm", self.length, 0.01, "centimeter")
        self.assertIn("cm", UNIT_REGISTRY[self.length])
        
        # Register a unit of a different dimension
        register_unit("s", self.time, 1.0, "second")
        self.assertIn("s", UNIT_REGISTRY[self.time])
        self.assertEqual(UNIT_DIMENSIONS["s"], self.time)

    def test_find_dimension_for_unit(self):
        """Test finding the dimension for a registered unit."""
        # Register units
        register_unit("m", self.length, 1.0, "meter")
        register_unit("s", self.time, 1.0, "second")
        
        # Find dimensions
        self.assertEqual(find_dimension_for_unit("m"), self.length)
        self.assertEqual(find_dimension_for_unit("s"), self.time)
        
        # Finding dimension for unregistered unit should raise error
        with self.assertRaises(ValueError):
            find_dimension_for_unit("unknown_unit")

    def test_get_conversion_factor(self):
        """Test getting conversion factors between units."""
        # Register units with conversion factors
        register_unit("m", self.length, 1.0, "meter")
        register_unit("cm", self.length, 0.01, "centimeter")
        register_unit("km", self.length, 1000.0, "kilometer")
        
        # Check conversion factors
        self.assertEqual(get_conversion_factor(self.length, "m", "cm"), 100.0)
        self.assertEqual(get_conversion_factor(self.length, "cm", "m"), 0.01)
        self.assertEqual(get_conversion_factor(self.length, "km", "m"), 1000.0)
        self.assertEqual(get_conversion_factor(self.length, "km", "cm"), 100000.0)
        
        # Invalid conversions should raise error
        with self.assertRaises(ValueError):
            get_conversion_factor(self.length, "unknown", "m")
        
        with self.assertRaises(ValueError):
            get_conversion_factor(self.time, "m", "s")  # Different dimensions


class TestCompoundUnitConversion(unittest.TestCase):
    """Tests for compound unit conversion functions."""

    def setUp(self):
        """Set up test dimensions and units."""
        # Create dimensions
        self.length = Dimension({"L": 1})
        self.time = Dimension({"T": 1})
        self.mass = Dimension({"M": 1})
        
        # Register units with conversion factors
        register_unit("m", self.length, 1.0, "meter")
        register_unit("cm", self.length, 0.01, "centimeter")
        register_unit("km", self.length, 1000.0, "kilometer")
        register_unit("s", self.time, 1.0, "second")
        register_unit("min", self.time, 60.0, "minute")
        register_unit("h", self.time, 3600.0, "hour")
        register_unit("kg", self.mass, 1.0, "kilogram")
        register_unit("g", self.mass, 0.001, "gram")

    def test_compound_factor(self):
        """Test calculating conversion factors for compound units."""
        # Simple unit
        meter = CompoundUnit({"m": 1})
        self.assertEqual(compound_factor(meter), 1.0)
        
        centimeter = CompoundUnit({"cm": 1})
        self.assertEqual(compound_factor(centimeter), 0.01)
        
        # Compound units
        velocity_mps = CompoundUnit({"m": 1, "s": -1})
        self.assertEqual(compound_factor(velocity_mps), 1.0)
        
        velocity_cmps = CompoundUnit({"cm": 1, "s": -1})
        self.assertEqual(compound_factor(velocity_cmps), 0.01)
        
        # More complex compound units
        force = CompoundUnit({"kg": 1, "m": 1, "s": -2})
        self.assertEqual(compound_factor(force), 1.0)
        
        force_cgs = CompoundUnit({"g": 1, "cm": 1, "s": -2})
        self.assertEqual(compound_factor(force_cgs), 0.001 * 0.01)  # 1e-5
        
        # Unknown unit should raise error
        with self.assertRaises(ValueError):
            compound_factor(CompoundUnit({"unknown": 1}))

    def test_get_compound_unit_conversion_factor(self):
        """Test getting conversion factors between compound units."""
        # Simple units
        meter = CompoundUnit({"m": 1})
        centimeter = CompoundUnit({"cm": 1})
        self.assertEqual(convert_compound_unit(meter, centimeter), 100.0)
        
        # Compound units
        velocity_mps = CompoundUnit({"m": 1, "s": -1})
        velocity_kmph = CompoundUnit({"km": 1, "h": -1})
        # 1 m/s = 3.6 km/h
        self.assertAlmostEqual(convert_compound_unit(velocity_mps, velocity_kmph), 3.6)
        
        # More complex compound units
        force_newton = CompoundUnit({"kg": 1, "m": 1, "s": -2})
        force_dyne = CompoundUnit({"g": 1, "cm": 1, "s": -2})
        # 1 N = 10^5 dyne
        self.assertAlmostEqual(convert_compound_unit(force_newton, force_dyne), 1e5)
        
        # Incompatible dimensions should raise error
        length = CompoundUnit({"m": 1})
        time = CompoundUnit({"s": 1})
        with self.assertRaises(ValueError):
            convert_compound_unit(length, time)


if __name__ == '__main__':
    unittest.main()
