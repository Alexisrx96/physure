import unittest

from measurekit.system import UnitSystem


class BaseTestUnit(unittest.TestCase):
    def setUp(self):
        """
        Creates a fresh, isolated UnitSystem instance before each test.

        Any test class that inherits from BaseTestUnit will automatically
        have a `self.system` attribute available for its tests.
        """
        self.system = UnitSystem()

    def tearDown(self):
        """
        This method is no longer needed.

        Because each test gets its own UnitSystem instance, there is no
        shared global state to clean up. The old system instance is
        automatically discarded when the test finishes.
        """
        pass
