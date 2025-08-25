"""Test suite for the base_entity module."""
import unittest

from notation.base_entity import BaseExponentEntity


class TestBaseExponentEntity(unittest.TestCase):
    """Tests for the BaseExponentEntity class."""

    def test_initialization(self):
        """Test initialization and normalization."""
        # Basic initialization
        entity = BaseExponentEntity({"x": 1, "y": 2})
        self.assertEqual(entity.exponents, {"x": 1, "y": 2})
        
        # Zero exponents should be removed
        entity = BaseExponentEntity({"x": 1, "y": 0, "z": 2})
        self.assertEqual(entity.exponents, {"x": 1, "z": 2})
        self.assertNotIn("y", entity.exponents)
        
        # Empty initialization
        entity = BaseExponentEntity({})
        self.assertEqual(entity.exponents, {})

    def test_arithmetic_operations(self):
        """Test arithmetic operations between entities."""
        # Multiplication
        entity1 = BaseExponentEntity({"x": 1, "y": 2})
        entity2 = BaseExponentEntity({"y": 1, "z": 3})
        
        result = entity1 * entity2
        self.assertEqual(result.exponents, {"x": 1, "y": 3, "z": 3})
        
        # Division
        result = entity1 / entity2
        self.assertEqual(result.exponents, {"x": 1, "y": 1, "z": -3})
        
        # Power
        result = entity1 ** 2
        self.assertEqual(result.exponents, {"x": 2, "y": 4})
        
        result = entity1 ** 0.5
        self.assertEqual(result.exponents, {"x": 0.5, "y": 1})
        
        # Complex operations
        entity3 = BaseExponentEntity({"a": 2, "b": -1})
        result = (entity1 * entity2) / entity3
        self.assertEqual(result.exponents, {"x": 1, "y": 3, "z": 3, "a": -2, "b": 1})

    def test_equality_and_hash(self):
        """Test equality comparison and hashing."""
        entity1 = BaseExponentEntity({"x": 1, "y": 2})
        entity2 = BaseExponentEntity({"x": 1, "y": 2})
        entity3 = BaseExponentEntity({"x": 2, "y": 1})
        
        # Equality
        self.assertEqual(entity1, entity2)
        self.assertNotEqual(entity1, entity3)
        
        # Hash
        self.assertEqual(hash(entity1), hash(entity2))
        self.assertNotEqual(hash(entity1), hash(entity3))
        
        # Equality with non-BaseExponentEntity objects
        self.assertNotEqual(entity1, "not an entity")
        self.assertNotEqual(entity1, 123)

    def test_string_representation(self):
        """Test string representation methods."""
        # Simple entity
        entity = BaseExponentEntity({"x": 1})
        self.assertEqual(str(entity), "x")
        
        # Entity with multiple exponents
        entity = BaseExponentEntity({"x": 1, "y": 2})
        # Order may vary, so we check for both possible representations
        self.assertTrue(str(entity) == "x·y²" or str(entity) == "y²·x")
        
        # Entity with negative exponents
        entity = BaseExponentEntity({"x": 1, "y": -1})
        self.assertEqual(str(entity), "x/y")
        
        # Mixed positive and negative exponents
        entity = BaseExponentEntity({"x": 2, "y": 1, "z": -3})
        # Order may vary, but format should be consistent
        self.assertIn("x²·y/z³", str(entity))
        
        # Only negative exponents
        entity = BaseExponentEntity({"x": -1, "y": -2})
        # Order may vary
        self.assertTrue(str(entity) == "1/(x·y²)" or str(entity) == "1/(y²·x)")
        
        # Empty entity
        entity = BaseExponentEntity({})
        self.assertEqual(str(entity), "1")
        
        # Repr should show the internal dictionary
        entity = BaseExponentEntity({"x": 1, "y": 2})
        # Order may vary in dictionary representation
        self.assertTrue(repr(entity) == "{'x': 1, 'y': 2}" or repr(entity) == "{'y': 2, 'x': 1}")

    def test_rtruediv(self):
        """Test the __rtruediv__ method."""
        entity = BaseExponentEntity({"x": 1, "y": 2})
        
        # Divide a scalar by an entity
        result = 1 / entity
        self.assertEqual(result.exponents, {"x": -1, "y": -2})
        
        # Should work with any numeric type
        result = 2.5 / entity
        self.assertEqual(result.exponents, {"x": -1, "y": -2})
        
        result = complex(1, 2) / entity
        self.assertEqual(result.exponents, {"x": -1, "y": -2})


if __name__ == '__main__':
    unittest.main()
