"""IO utilities for saving and loading application state."""

from __future__ import annotations

import pickle
from pathlib import Path

from measurekit.domain.measurement.vectorized_uncertainty import (
    get_active_store,
)


def save_state(
    filepath: str | Path, protocol: int = pickle.HIGHEST_PROTOCOL
) -> None:
    """Saves the current application state to a file.

    Currently saves:
    - The active CovarianceStore (including all variable correlations).

    Args:
        filepath: Path to the output file.
        protocol: Pickle protocol version.
    """
    path = Path(filepath)
    # Use active store fallback logic
    store = get_active_store()

    state = {
        "covariance_store": store,
        # Future: "unit_system": get_current_system() if customized
        "version": 1,
    }

    with path.open("wb") as f:
        pickle.dump(state, f, protocol=protocol)


def load_state(filepath: str | Path) -> None:
    """Loads application state from a file.

    Restores the CovarianceStore to the active context.

    Args:
        filepath: Path to the input file.
    """
    path = Path(filepath)
    if not path.exists():
        raise FileNotFoundError(f"State file not found: {filepath}")

    with path.open("rb") as f:
        state = pickle.load(f)

    if not isinstance(state, dict) or "version" not in state:
        # Fallback or error?
        # Check if it is raw store from early versions?
        # Assuming structured format for now.
        raise ValueError(
            "Invalid state file format (missing version/structure)."
        )

    store = state.get("covariance_store")
    if store is not None:
        # Restore to context
        from measurekit.domain.measurement.vectorized_uncertainty import (
            _current_store,
        )

        _current_store.set(store)

        # If store is loaded, backend needs to be set if None?
        # Typically pickle saves backend if it's serializable (like NumpyBackend).
