"""physure — High-performance physical dimension engine.

Successor to ``physure`` v0.1.9. Rust-first, zero-copy FFI.

Quick start::

    from physure import Q_

    d = Q_(10, "km")
    t = Q_(2, "hr")
    v = d / t
    print(v.to("m/s"))   # 1.3888... m/s

The native Rust core (``physure._core``) is a hard dependency.
There is no pure-Python fallback — install with::

    pip install physure          # maturin wheel, includes compiled core
    pip install "physure[numpy]" # + NumPy/SciPy/Numba integration
    pip install "physure[all]"   # all backends

"""

import contextlib
import sys
from collections.abc import Callable
from typing import TYPE_CHECKING, Any

# ── Version ───────────────────────────────────────────────────────────────────
__version__ = "0.2.1"

# ── Rust-first: hard import of native core ────────────────────────────────────
# Unlike physure, physure has NO pure-Python fallback.
# If the compiled extension is missing, we fail fast with a clear message.
try:
    from physure._core import (  # type: ignore[import]
        CovarianceStore,
        DimVector,
        PruningConfig,
        RationalUnit,
        UnitDefinition,
        UnitRegistry,
        batch_to_si_inplace,
        convert_units_inplace,
        eval_dual_number,
        parse_unit_expression,
        propagate_hessian_uncertainty,
        step_euler_inplace,
    )
    from physure._core import (
        Quantity as CoreQuantity,
    )
except ImportError as _err:
    raise ImportError(
        "\n"
        "  physure requires its compiled Rust extension (physure._core).\n"
        "  The extension was not found. To build it:\n"
        "\n"
        "    pip install physure          # install from PyPI (preferred)\n"
        "    maturin develop --release    # build from source\n"
        "\n"
        "  See: https://github.com/Alexisrx96/physure#installation\n"
    ) from _err

if TYPE_CHECKING:
    from physure._jit import jit
    from physure.application.context import (
        get_current_system,
        get_propagation_mode,
        propagation_mode,
        uncertainty_mode,
    )
    from physure.application.context import (
        get_current_system as get_active_system,
    )
    from physure.application.context import (
        use_system as system_context,
    )
    from physure.application.factories import QuantityFactory
    from physure.application.io import load_state, save_state
    from physure.application.startup import (
        create_default_system,
        create_system,
    )
    from physure.domain.exceptions import (
        ConversionError,
        PhysureError,
        UnitNotFoundError,
        UnknownUnitError,
    )
    from physure.domain.measurement.equivalencies import (
        equivalencies,
        spectral,
        thermodynamic,
    )
    from physure.domain.measurement.quantity import Quantity
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.measurement.uncertainty import Uncertainty
    from physure.domain.measurement.units import CompoundUnit, units
    from physure.domain.measurement.vectorized_uncertainty import (
        PhysureContext,
    )
    from physure.plotting import (
        plot,
        plot_covariance,
        plot_interactive,
        plot_pairplot,
        plot_parallel_coordinates,
        plot_slices,
    )

    default_system: UnitSystem
    Q_: QuantityFactory
    get_unit: Callable[[str], CompoundUnit]


# ── Lazy-loaded public API ────────────────────────────────────────────────────
# These attributes are deferred to keep the import-time budget low.
# Only the Rust core and its Python shim load eagerly.

_IO_ATTRS = {"save_state", "load_state"}
_UNITS_ATTRS = {"units", "CompoundUnit"}
_STARTUP_ATTRS = {"create_system", "create_default_system"}
_CONTEXT_ATTRS = {
    "get_current_system",
    "get_active_system",
    "get_propagation_mode",
    "propagation_mode",
    "uncertainty_mode",
    "system_context",
}
_EXCEPTION_ATTRS = {
    "ConversionError",
    "PhysureError",
    "UnitNotFoundError",
    "UnknownUnitError",
}
_VECTORIZED_ATTRS = {"PhysureContext"}
_PLOT_ATTRS = {
    "plot",
    "plot_slices",
    "plot_interactive",
    "plot_parallel_coordinates",
    "plot_pairplot",
    "plot_covariance",
}
_HELPER_ATTRS = {
    "approx_eq",
    "linspace",
    "sqrt",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "sinh",
    "cosh",
    "tanh",
    "exp",
    "log",
    "log10",
    "pi",
    "e",
}


def _load_io(name: str) -> Any:
    from physure.application.io import load_state, save_state

    return save_state if name == "save_state" else load_state


def _load_helpers(name: str) -> Any:
    import physure.ext.helpers as helpers

    return getattr(helpers, name)


def _load_q(name: str) -> Any:
    from physure.application.factories import QuantityFactory

    return QuantityFactory()


def _load_quantity(name: str) -> Any:
    from physure.domain.measurement.quantity import Quantity

    return Quantity


def _load_units(name: str) -> Any:
    from physure.domain.measurement.units import CompoundUnit, units

    return units if name == "units" else CompoundUnit


def _load_uncertainty(name: str) -> Any:
    from physure.domain.measurement.uncertainty import Uncertainty

    return Uncertainty


def _load_startup(name: str) -> Any:
    from physure.application.startup import (
        create_default_system,
        create_system,
    )

    return create_system if name == "create_system" else create_default_system


def _load_default_system(name: str) -> Any:
    from physure.domain.measurement.units import get_default_system

    return get_default_system()


def _load_jit(name: str) -> Any:
    from physure._jit import jit

    return jit


def _load_context(name: str) -> Any:
    from physure.application.context import (
        get_current_system,
        get_propagation_mode,
        propagation_mode,
        uncertainty_mode,
        use_system,
    )

    _map = {
        "get_current_system": get_current_system,
        "get_active_system": get_current_system,
        "get_propagation_mode": get_propagation_mode,
        "propagation_mode": propagation_mode,
        "uncertainty_mode": uncertainty_mode,
        "system_context": use_system,
    }
    return _map[name]


def _load_get_unit(name: str) -> Any:
    def get_unit(unit_expression: str) -> RationalUnit:
        from physure.application.context import get_current_system

        return get_current_system().get_unit(unit_expression)

    return get_unit


def _load_exceptions(name: str) -> Any:
    import physure.domain.exceptions as exc

    return getattr(exc, name)


def _load_vectorized(name: str) -> Any:
    from physure.domain.measurement.vectorized_uncertainty import (
        PhysureContext,
    )

    return PhysureContext


def _load_equivalencies(name: str) -> Any:
    from physure.domain.measurement.equivalencies import (
        equivalencies,
        spectral,
        thermodynamic,
    )

    _eq = {
        "equivalencies": equivalencies,
        "spectral": spectral,
        "thermodynamic": thermodynamic,
    }
    return _eq[name]


def _load_plotting(name: str) -> Any:
    import physure.plotting as plotting

    return getattr(plotting, name)


_ATTR_LOADERS: dict[str, Callable[[str], Any]] = {}
for _attr in _IO_ATTRS:
    _ATTR_LOADERS[_attr] = _load_io
_ATTR_LOADERS["Q_"] = _load_q
_ATTR_LOADERS["Quantity"] = _load_quantity
for _attr in _UNITS_ATTRS:
    _ATTR_LOADERS[_attr] = _load_units
_ATTR_LOADERS["Uncertainty"] = _load_uncertainty
for _attr in _STARTUP_ATTRS:
    _ATTR_LOADERS[_attr] = _load_startup
_ATTR_LOADERS["default_system"] = _load_default_system
_ATTR_LOADERS["jit"] = _load_jit
for _attr in _CONTEXT_ATTRS:
    _ATTR_LOADERS[_attr] = _load_context
_ATTR_LOADERS["get_unit"] = _load_get_unit
for _attr in _EXCEPTION_ATTRS:
    _ATTR_LOADERS[_attr] = _load_exceptions
for _attr in _VECTORIZED_ATTRS:
    _ATTR_LOADERS[_attr] = _load_vectorized
for _attr in ("equivalencies", "spectral", "thermodynamic"):
    _ATTR_LOADERS[_attr] = _load_equivalencies
for _attr in _PLOT_ATTRS:
    _ATTR_LOADERS[_attr] = _load_plotting
for _attr in _HELPER_ATTRS:
    _ATTR_LOADERS[_attr] = _load_helpers
del _attr


def __getattr__(name: str) -> Any:
    """Lazy-load public API members on first access."""
    loader = _ATTR_LOADERS.get(name)
    if loader is None:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    return loader(name)


# ── Extension auto-registration ───────────────────────────────────────────────
if "pandas" in sys.modules:
    with contextlib.suppress(ImportError, AttributeError):
        from physure.ext import (
            pandas_support,  # pyright: ignore[reportUnusedImport]
        )

if "numba" in sys.modules:
    with contextlib.suppress(ImportError, AttributeError):
        import physure.ext.numba_support  # pyright: ignore[reportUnusedImport]


# ── Public API ────────────────────────────────────────────────────────────────
__all__ = [
    # Core Rust types (eagerly loaded)
    "CovarianceStore",
    "DimVector",
    "PruningConfig",
    "Quantity",
    "RationalUnit",
    "UnitDefinition",
    "UnitRegistry",
    "convert_units_inplace",
    "eval_dual_number",
    "parse_unit_expression",
    "propagate_hessian_uncertainty",
    # Zero-copy buffer helpers
    "batch_to_si_inplace",
    "step_euler_inplace",
    # Lazy-loaded API
    "CompoundUnit",
    "ConversionError",
    "PhysureContext",
    "PhysureError",
    "Q_",
    "Uncertainty",
    "UnitNotFoundError",
    "UnknownUnitError",
    "acos",
    "approx_eq",
    "asin",
    "atan",
    "atan2",
    "cos",
    "cosh",
    "create_default_system",
    "create_system",
    "default_system",
    "e",
    "equivalencies",
    "exp",
    "get_active_system",
    "get_current_system",
    "get_unit",
    "jit",
    "linspace",
    "load_state",
    "log",
    "log10",
    "pi",
    "plot",
    "plot_covariance",
    "plot_interactive",
    "plot_pairplot",
    "plot_parallel_coordinates",
    "plot_slices",
    "save_state",
    "sin",
    "sinh",
    "sqrt",
    "spectral",
    "system_context",
    "tan",
    "tanh",
    "thermodynamic",
    "uncertainty_mode",
    "units",
]
