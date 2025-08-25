import unittest

from measurement.conversions import UNIT_DIMENSIONS, UNIT_REGISTRY
from measurement.units import CompoundUnit


class BaseTestUnit(unittest.TestCase):
    def setUp(self):
        pass

    def tearDown(self):
        """Reset the unit registry after each test."""
        UNIT_REGISTRY.clear()
        UNIT_DIMENSIONS.clear()
        CompoundUnit._aliases.clear()
        CompoundUnit._alias_to_exponents.clear()
        CompoundUnit._cache.clear()