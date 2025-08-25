"""Test suite for the Dimension class and get_dimension function."""
import unittest

from measurement.dimensions import Dimension, get_dimension

from tests.base_test_class import BaseTestUnit

class TestDimension(BaseTestUnit):
    """Tests for the Dimension class."""

    def test_init_and_new(self):
        """Test initialization and __new__ caching behavior."""
        # Test basic initialization
        dim1 = Dimension({"L": 1})
        self.assertEqual(dim1.exponents, {"L": 1})
        
        # Test caching through __new__ (same exponents should return same instance)
        dim2 = Dimension({"L": 1})
        self.assertIs(dim1, dim2)  # Same instance, not just equality
        
        # Different exponents should create different instances
        dim3 = Dimension({"L": 2})
        self.assertIsNot(dim1, dim3)
        
        # Zero exponents should be removed
        dim4 = Dimension({"L": 1, "M": 0})
        self.assertEqual(dim4.exponents, {"L": 1})
        self.assertIs(dim4, dim1)  # Same as first dimension after normalization

    def test_arithmetic_operations(self):
        """Test arithmetic operations between dimensions."""
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})
        
        # Multiplication
        velocity_dim = length / time
        self.assertEqual(velocity_dim.exponents, {"L": 1, "T": -1})
        
        # Division
        frequency_dim = Dimension({}) / time  # 1/T
        self.assertEqual(frequency_dim.exponents, {"T": -1})
        
        # Power
        area_dim = length ** 2
        self.assertEqual(area_dim.exponents, {"L": 2})
        
        # Complex arithmetic
        force_dim = mass * length / (time ** 2)
        self.assertEqual(force_dim.exponents, {"M": 1, "L": 1, "T": -2})
        
        # Scalar division
        inverse_length = 1 / length
        self.assertEqual(inverse_length.exponents, {"L": -1})

    def test_equality_and_hash(self):
        """Test equality comparison and hashing."""
        dim1 = Dimension({"L": 1, "T": -1})
        dim2 = Dimension({"L": 1, "T": -1})
        dim3 = Dimension({"M": 1})
        
        # Same exponents should be equal
        self.assertEqual(dim1, dim2)
        self.assertEqual(hash(dim1), hash(dim2))
        
        # Different exponents should not be equal
        self.assertNotEqual(dim1, dim3)
        self.assertNotEqual(hash(dim1), hash(dim3))
        
        # Should not be equal to non-Dimension objects
        self.assertNotEqual(dim1, "not a dimension")

    def test_string_representation(self):
        """Test string representation methods."""
        # Simple dimension
        dim1 = Dimension({"L": 1})
        self.assertEqual(str(dim1), "L")
        
        # Dimension with exponent
        dim2 = Dimension({"L": 2})
        self.assertEqual(str(dim2), "L²")
        
        # Complex dimension
        dim3 = Dimension({"L": 1, "M": 1, "T": -2})
        self.assertEqual(str(dim3), "L·M·T⁻²")
        
        # Test repr
        dim4 = Dimension({"L": 1, "T": -1})
        # Convert to sets to handle dictionary ordering issues
        self.assertEqual(
            set(repr(dim4).strip("{}").split(", ")),
            {"'L': 1", "'T': -1"}
        )
        
        # Test empty dimension
        dim5 = Dimension({})
        self.assertEqual(str(dim5), "1")
        self.assertEqual(repr(dim5), "{}")
        
        # Multiple dimensions
        dim6 = Dimension({"L": 1, "M": 1})
        self.assertEqual(str(dim6), "L·M")  # Should be sorted properly
        
        # Negative exponents
        dim7 = Dimension({"L": 1, "T": -1})
        self.assertEqual(str(dim7), "L·T⁻¹")
        
        # Integer division result (should have no decimals)
        dim8 = Dimension({"L": 2}) / Dimension({"L": 1})
        self.assertEqual(str(dim8), "L")
        
        # Tests for equality operators
        dim9 = Dimension({"L": 1})
        dim10 = Dimension({"L": 1})
        dim11 = Dimension({"M": 1})
        self.assertEqual(dim9, dim10)
        self.assertNotEqual(dim9, dim11)


class TestGetDimension(unittest.TestCase):
    """Tests for the get_dimension function."""

    def test_get_dimension(self):
        """Test parsing dimension expressions."""
        # Basic dimensions
        self.assertEqual(get_dimension("L").exponents, {"L": 1})
        self.assertEqual(get_dimension("M").exponents, {"M": 1})
        
        # Multiple dimensions
        self.assertEqual(get_dimension("L·M").exponents, {"L": 1, "M": 1})
        self.assertEqual(get_dimension("L/T").exponents, {"L": 1, "T": -1})
        
        # With exponents
        self.assertEqual(get_dimension("L²").exponents, {"L": 2})
        self.assertEqual(get_dimension("L^2").exponents, {"L": 2})
        self.assertEqual(get_dimension("L⁻¹").exponents, {"L": -1})
        self.assertEqual(get_dimension("L^-1").exponents, {"L": -1})
        
        # Complex expressions
        self.assertEqual(get_dimension("L·M/T²").exponents, {"L": 1, "M": 1, "T": -2})
        self.assertEqual(get_dimension("(L·M)/(T²)").exponents, {"L": 1, "M": 1, "T": -2})
        
        # Parentheses and powers
        self.assertEqual(get_dimension("(L/T)²").exponents, {"L": 2, "T": -2})
        
        # Invalid syntax should raise an error
        with self.assertRaises(ValueError):
            get_dimension("L@T")  # Invalid character


if __name__ == '__main__':
    unittest.main()
