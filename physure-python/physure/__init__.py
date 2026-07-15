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
from typing import Any

# ── Version ───────────────────────────────────────────────────────────────────
__version__ = "0.2.0"

# ── Rust-first: hard import of native core ────────────────────────────────────
# Unlike physure, physure has NO pure-Python fallback.
# If the compiled extension is missing, we fail fast with a clear message.
try:
    from physure._core import (  # type: ignore[import]
        RationalUnit,
        UnitRegistry,
        Quantity as CoreQuantity,
        PruningConfig,
        CovarianceStore,
        DimVector,
        UnitDefinition,
        parse_unit_expression,
        eval_dual_number,
        propagate_hessian_uncertainty,
        convert_units_inplace,
        batch_to_si_inplace,
        step_euler_inplace,
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


# ── Lazy-loaded public API ────────────────────────────────────────────────────
# These attributes are deferred to keep the import-time budget low.
# Only the Rust core and its Python shim load eagerly.

_IO_ATTRS      = {"save_state", "load_state"}
_UNITS_ATTRS   = {"units", "CompoundUnit"}
_STARTUP_ATTRS = {"create_system", "create_default_system"}
_CONTEXT_ATTRS = {
    "get_current_system", "get_active_system", "get_propagation_mode",
    "propagation_mode", "uncertainty_mode", "system_context",
}
_EXCEPTION_ATTRS = {
    "ConversionError", "PhysureError", "UnitNotFoundError", "UnknownUnitError",
}
_VECTORIZED_ATTRS = {"PhysureContext"}
_PLOT_ATTRS = {
    "plot", "plot_slices", "plot_interactive",
    "plot_parallel_coordinates", "plot_pairplot", "plot_covariance",
}


def __getattr__(name: str) -> Any:
    """Lazy-load public API members on first access."""
    if name in _IO_ATTRS:
        from physure.application.io import load_state, save_state
        return save_state if name == "save_state" else load_state

    if name == "Q_":
        from physure.application.factories import QuantityFactory
        return QuantityFactory()

    if name == "Quantity":
        from physure.domain.measurement.quantity import Quantity
        return Quantity

    if name in _UNITS_ATTRS:
        from physure.domain.measurement.units import CompoundUnit, units
        return units if name == "units" else CompoundUnit

    if name == "Uncertainty":
        from physure.domain.measurement.uncertainty import Uncertainty
        return Uncertainty

    if name in _STARTUP_ATTRS:
        from physure.application.startup import create_default_system, create_system
        return create_system if name == "create_system" else create_default_system

    if name == "default_system":
        from physure.domain.measurement.units import get_default_system
        return get_default_system()

    if name == "jit":
        from physure._jit import jit
        return jit

    if name in _CONTEXT_ATTRS:
        from physure.application.context import (
            get_current_system, get_propagation_mode,
            propagation_mode, uncertainty_mode, use_system,
        )
        _map = {
            "get_current_system": get_current_system,
            "get_active_system":  get_current_system,
            "get_propagation_mode": get_propagation_mode,
            "propagation_mode":   propagation_mode,
            "uncertainty_mode":   uncertainty_mode,
            "system_context":     use_system,
        }
        return _map[name]

    if name == "get_unit":
        def get_unit(unit_expression: str) -> RationalUnit:
            from physure.application.context import get_current_system
            return get_current_system().get_unit(unit_expression)
        return get_unit

    if name in _EXCEPTION_ATTRS:
        import physure.domain.exceptions as exc
        return getattr(exc, name)

    if name in _VECTORIZED_ATTRS:
        from physure.domain.measurement.vectorized_uncertainty import PhysureContext
        return PhysureContext

    if name in {"equivalencies", "spectral", "thermodynamic"}:
        from physure.domain.measurement.equivalencies import (
            equivalencies, spectral, thermodynamic,
        )
        _eq = {"equivalencies": equivalencies, "spectral": spectral, "thermodynamic": thermodynamic}
        return _eq[name]

    if name in _PLOT_ATTRS:
        import physure.plotting as plotting
        return getattr(plotting, name)

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


# ── Extension auto-registration ───────────────────────────────────────────────
if "pandas" in sys.modules:
    with contextlib.suppress(ImportError, AttributeError):
        from physure.ext import pandas_support  # pyright: ignore[reportUnusedImport]

if "numba" in sys.modules:
    with contextlib.suppress(ImportError, AttributeError):
        import physure.ext.numba_support  # pyright: ignore[reportUnusedImport]


# ── Public API ────────────────────────────────────────────────────────────────
__all__ = [
    # Core Rust types (eagerly loaded)
    "RationalUnit",
    "UnitRegistry",
    "Quantity",
    "PruningConfig",
    "CovarianceStore",
    "DimVector",
    "UnitDefinition",
    "parse_unit_expression",
    "eval_dual_number",
    "propagate_hessian_uncertainty",
    "convert_units_inplace",
    # Zero-copy buffer helpers
    "batch_to_si_inplace",
    "step_euler_inplace",
    # Lazy-loaded API
    "Q_",
    "CompoundUnit",
    "ConversionError",
    "PhysureContext",
    "PhysureError",
    "PruningConfig",
    "Uncertainty",
    "UnitNotFoundError",
    "UnknownUnitError",
    "create_default_system",
    "create_system",
    "default_system",
    "equivalencies",
    "get_active_system",
    "get_current_system",
    "get_unit",
    "jit",
    "load_state",
    "plot",
    "plot_covariance",
    "plot_interactive",
    "plot_pairplot",
    "plot_parallel_coordinates",
    "plot_slices",
    "save_state",
    "spectral",
    "system_context",
    "thermodynamic",
    "uncertainty_mode",
    "units",
]
