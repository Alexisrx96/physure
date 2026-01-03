# tests/tools/test_stub_gen.py
import os
import unittest

from measurekit.domain.measurement.units import units
from measurekit.scripts.generate_types import generate


class TestStubGeneration(unittest.TestCase):
    def test_generate_types(self):
        # Run generator
        generate()

        # Verify files exist
        base_path = os.path.dirname(os.path.dirname(os.path.dirname(__file__)))
        generated_types_path = os.path.join(
            base_path, "measurekit", "_generated_types.py"
        )
        registry_stub_path = os.path.join(
            base_path, "measurekit", "core", "registry.pyi"
        )

        self.assertTrue(os.path.exists(generated_types_path))
        self.assertTrue(os.path.exists(registry_stub_path))

        # Verify content
        with open(generated_types_path) as f:
            content = f.read()
            self.assertIn("UnitName = Literal", content)
            self.assertIn('"meter"', content)
            self.assertIn('"second"', content)

        with open(registry_stub_path) as f:
            content = f.read()
            self.assertIn("class UnitRegistry:", content)
            self.assertIn("meter: Unit", content)
            self.assertIn("second: Unit", content)

    def test_mock_plugin_discovery(self):
        # We can't easily mock entry points here without complexity,
        # but we can manually register a lazy loader and see if it's picked up.
        units.register_lazy("mock_unit", lambda: "mock_unit")

        generate()

        base_path = os.path.dirname(os.path.dirname(os.path.dirname(__file__)))
        generated_types_path = os.path.join(
            base_path, "measurekit", "_generated_types.py"
        )

        with open(generated_types_path) as f:
            content = f.read()
            self.assertIn('"mock_unit"', content)

        registry_stub_path = os.path.join(
            base_path, "measurekit", "core", "registry.pyi"
        )
        with open(registry_stub_path) as f:
            content = f.read()
            self.assertIn("mock_unit: Unit", content)


if __name__ == "__main__":
    unittest.main()
