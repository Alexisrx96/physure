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

        assert os.path.exists(generated_types_path)
        assert os.path.exists(registry_stub_path)

        # Verify content
        with open(generated_types_path) as f:
            content = f.read()
            assert "UnitName = Literal" in content
            assert '"meter"' in content
            assert '"second"' in content

        with open(registry_stub_path) as f:
            content = f.read()
            assert "class UnitRegistry:" in content
            assert "meter: Unit" in content
            assert "second: Unit" in content

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
            assert '"mock_unit"' in content

        registry_stub_path = os.path.join(
            base_path, "measurekit", "core", "registry.pyi"
        )
        with open(registry_stub_path) as f:
            content = f.read()
            assert "mock_unit: Unit" in content


if __name__ == "__main__":
    unittest.main()
