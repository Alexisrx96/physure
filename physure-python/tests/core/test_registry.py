from unittest.mock import MagicMock, patch

import pytest

from physure._core import UnitRegistry as CoreUnitRegistry
from physure.core.registry import UnitRegistry


def test_registry_lazy_loading():
    """Verify that units are loaded only when accessed."""
    registry = UnitRegistry()

    # We use a plain function to avoid MagicMock's automatic 'load' attribute
    call_count = 0

    def mock_loader_func():
        nonlocal call_count
        call_count += 1
        return "mock_unit"

    registry.register_lazy("lazy_unit", mock_loader_func)

    # Should not be called yet
    assert call_count == 0

    # Access unit
    unit = registry.lazy_unit

    # Now it should be called
    assert call_count == 1
    assert unit == "mock_unit"

    # Subsequent access should use cache
    unit2 = registry.lazy_unit
    assert call_count == 1
    assert unit2 == "mock_unit"


def test_registry_discovery():
    """Verify that entry points are discovered as lazy loaders."""
    registry = UnitRegistry()

    mock_ep = MagicMock()
    mock_ep.name = "plugin_unit"
    mock_ep.load.return_value = "plugin_value"

    with patch("importlib.metadata.entry_points", return_value=[mock_ep]):
        registry.discover_plugins()

        # Should be in dir() but not yet loaded
        assert "plugin_unit" in dir(registry)
        mock_ep.load.assert_not_called()

        # Access
        unit = registry.plugin_unit
        assert unit == "plugin_value"
        mock_ep.load.assert_called_once()


def test_registry_conflict_resolution():
    """Verify that existing units cannot be overwritten by lazy loaders."""
    registry = UnitRegistry()
    registry.register("core_unit", "core_value")

    # Try to register lazy with same name
    mock_loader = MagicMock()
    registry.register_lazy("core_unit", mock_loader)

    assert registry.core_unit == "core_value"
    mock_loader.assert_not_called()


def test_registry_error_handling():
    """Verify that failing loaders raise AttributeError."""
    registry = UnitRegistry()

    def failing_loader():
        raise RuntimeError("Load failed")

    registry.register_lazy("broken_unit", failing_loader)

    with pytest.raises(
        AttributeError, match=r"Unit 'broken_unit' failed to load"
    ):
        _ = registry.broken_unit


def test_core_units_available():
    """Verify that core units are available in the global registry."""
    from physure import units
    from physure.domain.measurement.units import CompoundUnit

    # Test a few core units
    assert isinstance(units.meter, CompoundUnit)
    assert isinstance(units.kilogram, CompoundUnit)
    assert isinstance(units.second, CompoundUnit)


def test_registry_dir_completeness():
    """Verify that dir() lists both cached and lazy units."""
    registry = UnitRegistry()
    registry.register("cached", 1)
    registry.register_lazy("lazy", lambda: 2)

    d = dir(registry)
    assert "cached" in d
    assert "lazy" in d
    # Our __dir__ only returns units
    assert "register" not in d


def _registry_of_the_catalog():
    registry = UnitRegistry()
    registry.register_catalog(
        CoreUnitRegistry.from_conf(), lambda name: ("unit", name)
    )
    return registry


def test_a_catalog_decides_which_names_are_units():
    registry = _registry_of_the_catalog()
    assert registry.m == ("unit", "m")
    assert registry.km == ("unit", "km")
    assert registry.kilometer == ("unit", "kilometer")
    # a level takes no prefix, nor does the number one, and a name that is not
    # a unit or cannot be an attribute is none of them
    for name in ("attodB", "kunity", "E1", "xyzzy", "_hidden"):
        with pytest.raises(AttributeError):
            getattr(registry, name)
    names = dir(registry)
    assert {"m", "km", "kPa", "kilometer", "dB"} <= set(names)
    assert not {"attodB", "E1"} & set(names)
    assert all(name.isidentifier() for name in names)


def test_a_unit_of_the_catalog_wins_over_a_plugin_of_the_same_name():
    registry = _registry_of_the_catalog()
    registry.register_lazy("km", lambda: "plugin")
    registry.register_lazy("plugin_unit", lambda: "plugin")
    assert registry.km == ("unit", "km")
    assert registry.plugin_unit == "plugin"
    assert "plugin_unit" in dir(registry)


def test_the_units_package_answers_like_the_registry():
    import physure.units as package
    from physure.domain.measurement.units import CompoundUnit, units

    assert package.km is units.km
    assert isinstance(package.km, CompoundUnit)
    assert "km" in dir(package)
    assert package.available_units == units.available_units
    with pytest.raises(AttributeError, match=r"physure\.units"):
        _ = package.attodB
