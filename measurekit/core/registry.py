import importlib.metadata
import logging
from collections.abc import Callable
from typing import Any

log = logging.getLogger(__name__)


class UnitRegistry:
    """A lazy-loading registry for physical units.

    This registry allows for units to be discovered via entry points and
    loaded only when accessed, reducing startup latency and improving
    extensibility.
    """

    def __init__(self):
        """Initializes the registry with empty cache and loaders."""
        self._registry: dict[str, Any] = {}
        self._lazy_loaders: dict[str, Any] = {}
        self._discovered = False

    def clear(self) -> None:
        """Clears all registered units and loaders. Useful for test isolation."""
        self._registry.clear()
        self._lazy_loaders.clear()
        self._discovered = False

    def register(self, name: str, unit: Any) -> None:
        """Adds a concrete unit to the registry.

        Core units should be registered here. If a conflict occurs,
        the first-registered unit (typically core) wins.
        """
        if name in self._registry:
            log.warning(f"Unit '{name}' is already registered. Skipping.")
            return
        self._registry[name] = unit

    def register_lazy(self, name: str, loader_func: Callable[[], Any]) -> None:
        """Adds a lazy loader for a unit."""
        if name in self._registry or name in self._lazy_loaders:
            return
        self._lazy_loaders[name] = loader_func

    def discover_plugins(self) -> None:
        """Scans entry points for the group 'measurekit.units'.

        This method populates the lazy loaders without importing the modules.
        """
        if self._discovered:
            return

        try:
            # importlib.metadata.entry_points(group=...) is standard since 3.10
            eps = importlib.metadata.entry_points(group="measurekit.units")
            for ep in eps:
                if (
                    ep.name not in self._registry
                    and ep.name not in self._lazy_loaders
                ):
                    self._lazy_loaders[ep.name] = ep
        except Exception as e:
            log.error(f"Error discovering unit plugins: {e}")

        self._discovered = True

    def __getattr__(self, name: str) -> Any:
        """Retrieves a unit from the registry, loading it if necessary."""
        if name in self._registry:
            return self._registry[name]

        if name in self._lazy_loaders:
            loader = self._lazy_loaders.pop(name)
            try:
                # If it's an EntryPoint, load it.
                if hasattr(loader, "load"):
                    obj = loader.load()
                    # If the entry point is a factory, call it
                    unit = (
                        obj()
                        if callable(obj) and not hasattr(obj, "exponents")
                        else obj
                    )
                else:
                    # Generic loader function
                    unit = loader()

                self._registry[name] = unit
                return unit
            except Exception as e:
                log.error(f"Failed to load unit plugin '{name}': {e}")
                raise AttributeError(
                    f"Unit '{name}' failed to load: {e}"
                ) from e

        raise AttributeError(f"Unit '{name}' not found in registry.")

    @property
    def available_units(self) -> list[str]:
        """Returns a list of all available unit names."""
        return sorted(
            list(self._registry.keys()) + list(self._lazy_loaders.keys())
        )

    def __dir__(self) -> list[str]:
        """Lists all available units for discovery (e.g. in notebooks)."""
        return sorted(
            list(self._registry.keys()) + list(self._lazy_loaders.keys())
        )
