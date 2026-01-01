import pytest

from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.system import UnitSystem
from measurekit.domain.measurement.units import CompoundUnit


@pytest.fixture
def system():
    """Provides a fresh, isolated UnitSystem instance for each test."""
    return UnitSystem()


@pytest.fixture
def common_system(system):
    """Provides a UnitSystem populated with standard units."""
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    mass = Dimension({"M": 1})
    force = mass * length / (time**2)
    energy = force * length

    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    system.register_unit("km", length, LinearConverter(1000.0), "kilometer")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit(
        "N",
        Dimension({"M": 1, "L": 1, "T": -2}),
        LinearConverter(1.0),
        "newton",
        recipe=CompoundUnit({"kg": 1, "m": 1, "s": -2}),
    )
    system.register_unit("J", energy, LinearConverter(1.0), "joule")
    return system


@pytest.fixture(params=["python", "numpy", "jax", "torch"])
def backend_instance(request):
    """Provides a backend instance, skipping if dependencies are missing."""
    from measurekit.core.dispatcher import BackendManager

    name = request.param
    try:
        if name == "python":
            return BackendManager._get_python_backend()
        return BackendManager._get_or_load_backend(name)
    except (ImportError, ModuleNotFoundError):
        pytest.skip(f"Backend {name} dependencies not installed.")
