import unittest

from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.domain.measurement.system import UnitSystem


class BaseTestUnit(unittest.TestCase):
    def setUp(self):
        """
        Creates a fresh, isolated UnitSystem instance before each test.

        Any test class that inherits from BaseTestUnit will automatically
        have a `self.system` attribute available for its tests.
        """
        self.system = UnitSystem()

    def add_common_units(self):
        """A helper method to populate the system with standard units."""
        length = Dimension({"L": 1})
        time = Dimension({"T": 1})
        mass = Dimension({"M": 1})
        force = mass * length / (time**2)
        energy = force * length

        self.system.register_unit("m", length, LinearConverter(1.0), "meter")
        self.system.register_unit(
            "cm", length, LinearConverter(0.01), "centimeter"
        )
        self.system.register_unit(
            "km", length, LinearConverter(1000.0), "kilometer"
        )
        self.system.register_unit("s", time, LinearConverter(1.0), "second")
        self.system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
        self.system.register_unit(
            "N",
            Dimension({"M": 1, "L": 1, "T": -2}),
            LinearConverter(1.0),
            "newton",
            recipe=CompoundUnit({"kg": 1, "m": 1, "s": -2}),
        )
        self.system.register_unit("J", energy, LinearConverter(1.0), "joule")

    def tearDown(self):
        """
        This method is no longer needed.

        Because each test gets its own UnitSystem instance, there is no
        shared global state to clean up. The old system instance is
        automatically discarded when the test finishes.
        """
        pass
