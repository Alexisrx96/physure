# tests/tools/test_stub_gen.py
import pytest

from physure.domain.measurement.units import units
from physure.scripts.generate_types import generate


@pytest.fixture
def package_dir(tmp_path):
    """A skeleton package the generator can write into.

    The generator writes tracked files; a test that pointed it at the real
    package would rewrite them (and, with a registered mock unit, leave the
    mock in the committed stubs).
    """
    (tmp_path / "core").mkdir()
    (tmp_path / "units").mkdir()
    return tmp_path


def test_generate_types(package_dir):
    generate(str(package_dir))

    generated_types = (package_dir / "_generated_types.py").read_text(
        encoding="utf-8"
    )
    assert "UnitName = Literal" in generated_types
    assert '"meter"' in generated_types
    assert '"second"' in generated_types

    registry_stub = (package_dir / "core" / "registry.pyi").read_text(
        encoding="utf-8"
    )
    assert "class UnitRegistry:" in registry_stub
    assert "meter: Unit" in registry_stub
    assert "second: Unit" in registry_stub


def test_mock_plugin_discovery(package_dir, monkeypatch):
    # We can't easily mock entry points here without complexity,
    # but we can manually register a lazy loader and see if it's picked up.
    monkeypatch.setattr(units, "_lazy_loaders", dict(units._lazy_loaders))
    units.register_lazy("mock_unit", lambda: "mock_unit")

    generate(str(package_dir))

    generated_types = (package_dir / "_generated_types.py").read_text(
        encoding="utf-8"
    )
    assert '"mock_unit"' in generated_types

    registry_stub = (package_dir / "core" / "registry.pyi").read_text(
        encoding="utf-8"
    )
    assert "mock_unit: Unit" in registry_stub
