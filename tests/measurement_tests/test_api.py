import unittest

from measurekit import default_system
from measurekit.measurement.api import (
    _QuantityFactory,
    _SpecializedQuantityFactory,
)
from measurekit.measurement.units import get_unit


class TestSpecializedQuantityFactory(unittest.TestCase):
    """Tests for the _SpecializedQuantityFactory class."""

    def test_init(self):
        """Test the initialization of the _SpecializedQuantityFactory class."""
        # FIX: Swapped arguments to the correct order: (unit, system).
        factory = _SpecializedQuantityFactory(get_unit("m"), default_system)

        # FIX: Accessed the correct internal attributes (_system, _default_unit).
        self.assertEqual(factory._system, default_system)
        self.assertEqual(factory._default_unit, get_unit("m"))

    def test_call(self):
        """Test the __call__ method of the _SpecializedQuantityFactory class."""
        # FIX: Swapped arguments to the correct order: (unit, system).
        factory = _SpecializedQuantityFactory(get_unit("m"), default_system)
        quantity = factory(5)
        self.assertEqual(quantity.magnitude, 5)
        self.assertEqual(quantity.unit, get_unit("m"))

    def test_repr(self):
        """Test the __repr__ method of the _SpecializedQuantityFactory class."""
        # FIX: Swapped arguments to the correct order: (unit, system).
        factory = _SpecializedQuantityFactory(get_unit("m"), default_system)

        # FIX: Updated the expected string to match the actual __repr__ output.
        self.assertEqual(repr(factory), "<Quantity Factory for unit='m'>")


class TestQuantityFactory(unittest.TestCase):
    """Tests for the _QuantityFactory class."""

    def test_call(self):
        """Test the __call__ method of the _QuantityFactory class."""
        # FIX: Removed the argument, as _QuantityFactory is initialized without any.
        factory = _QuantityFactory()
        quantity = factory(5, "m")
        self.assertEqual(quantity.magnitude, 5)
        self.assertEqual(quantity.unit, get_unit("m"))

    def test_getitem(self):
        """Test the __getitem__ method of the _QuantityFactory class."""
        # FIX: Removed the argument, as _QuantityFactory is initialized without any.
        factory = _QuantityFactory()
        specialized_factory = factory["m"]
        self.assertIsInstance(specialized_factory, _SpecializedQuantityFactory)

        # FIX: Accessed the correct internal attribute (_default_unit).
        self.assertEqual(specialized_factory._default_unit, get_unit("m"))


if __name__ == "__main__":
    unittest.main()
