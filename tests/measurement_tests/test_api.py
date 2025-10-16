import unittest

from measurekit import default_system, get_unit
from measurekit.application.factories import (
    QuantityFactory,
    SpecializedQuantityFactory,
)
from measurekit.domain.measurement.quantity import Quantity


class TestSpecializedQuantityFactory(unittest.TestCase):
    """Tests for the _SpecializedQuantityFactory class."""

    def test_init(self):
        """Test the initialization of _SpecializedQuantityFactory class."""
        factory = SpecializedQuantityFactory(get_unit("m"), default_system)

        self.assertEqual(factory._system, default_system)
        self.assertEqual(factory._default_unit, get_unit("m"))

    def test_call(self):
        """Test the __call__ method of _SpecializedQuantityFactory class."""
        factory = SpecializedQuantityFactory(get_unit("m"), default_system)
        quantity = factory(5)
        self.assertEqual(quantity.magnitude, 5)
        self.assertEqual(quantity.unit, get_unit("m"))

    def test_repr(self):
        """Test the __repr__ method of _SpecializedQuantityFactory class."""
        factory = SpecializedQuantityFactory(get_unit("m"), default_system)

        self.assertEqual(repr(factory), "<Quantity Factory for unit='m'>")


class TestQuantityFactory(unittest.TestCase):
    def test_call(self):
        """Test the __call__ method of the _QuantityFactory class."""
        factory = QuantityFactory(default_system)
        q = factory(10, "m/s")
        self.assertIsInstance(q, Quantity)
        self.assertEqual(q.magnitude, 10)
        self.assertEqual(q.unit, default_system.get_unit("m/s"))

    def test_getitem(self):
        """Test the __getitem__ method of the _QuantityFactory class."""
        factory = QuantityFactory(default_system)
        meter_factory = factory["m"]
        self.assertIsInstance(meter_factory, SpecializedQuantityFactory)
        self.assertEqual(
            meter_factory._default_unit, default_system.get_unit("m")
        )


if __name__ == "__main__":
    unittest.main()
