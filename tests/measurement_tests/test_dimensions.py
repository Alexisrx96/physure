# tests/measurement_tests/test_dimensions.py (Refactored)

"""Test suite for the Dimension class."""

import unittest

from measurekit.domain.measurement.dimensions import Dimension, get_dimension
from tests.base_test_class import BaseTestUnit


class TestDimension(BaseTestUnit):
    """Tests for the Dimension class."""

    def setUp(self):
        """Set up a fresh system for each test."""
        super().setUp()
        # Register dimension names into our isolated test system
        self.system.register_dimension(Dimension({"L": 1}), "Length")
        self.system.register_dimension(Dimension({"M": 1}), "Mass")
        self.system.register_dimension(Dimension({"T": 1}), "Time")

    def test_init_and_caching(self):
        """Test initialization and caching behavior."""
        dim1 = Dimension({"L": 1})
        self.assertEqual(dim1.exponents, {"L": 1})

        dim2 = Dimension({"L": 1})
        self.assertIs(dim1, dim2)  # Should return the same instance

    def test_arithmetic_operations(self):
        """Test arithmetic operations between dimensions."""
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})

        velocity_dim = length / time
        self.assertEqual(velocity_dim.exponents, {"L": 1, "T": -1})

    def test_string_representation(self):
        """Test the string representation of dimensions."""
        force_dim = Dimension({"M": 1, "L": 1, "T": -2})
        self.assertEqual(str(force_dim), "L·M·T⁻²")

        # Test that the registered name is retrievable from the system
        length_dim = Dimension({"L": 1})
        self.assertEqual(
            self.system._DIMENSION_NAME_REGISTRY.get(length_dim), "Length"
        )


class TestGetDimension(BaseTestUnit):
    """Tests for the get_dimension function."""

    def test_get_dimension_parsing(self):
        """Test parsing dimension expressions."""
        # This function is stateless and should work without a custom system
        self.assertEqual(
            get_dimension("L·M/T²").exponents, {"L": 1, "M": 1, "T": -2}
        )
        self.assertEqual(get_dimension("(L/T)²").exponents, {"L": 2, "T": -2})


if __name__ == "__main__":
    unittest.main()
