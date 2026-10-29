import importlib.metadata
import keyword
import logging
from collections.abc import Callable
from typing import Any

log = logging.getLogger(__name__)


class UnitRegistry:
    """A lazy-loading registry for physical units.

    This registry allows for units to be discovered via entry points and
    loaded only when accessed, reducing startup latency and improving
    extensibility. The units of a catalog (see register_catalog) are its
    units too, and they win over a plugin that has the same name.
    """

    def __init__(self):
        """Initializes the registry with empty cache and loaders."""
        self._registry: dict[str, Any] = {}
        self._lazy_loaders: dict[str, Any] = {}
        # A catalog and what makes a unit of one of its names. See
        # register_catalog.
        self._catalog: Any = None
        self._make_unit: Callable[[str], Any] | None = None
        self._catalog_names: list[str] | None = None
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
        if (
            name in self._registry
            or name in self._lazy_loaders
            or self._in_catalog(name)
        ):
            return
        self._lazy_loaders[name] = loader_func

    def register_catalog(
        self, catalog: Any, make_unit: Callable[[str], Any]
    ) -> None:
        """Makes every name a catalog knows a unit of this registry.

        `catalog` is a core registry (`physure._core.UnitRegistry`): it says
        which names are units (`knows`) and lists them (`resolvable_names`),
        prefixed ones such as `km` included, so nothing is listed here. The
        unit of a name is `make_unit(name)`, made the first time it is asked
        for.
        """
        self._catalog = catalog
        self._make_unit = make_unit
        self._catalog_names = None

    def _in_catalog(self, name: str) -> bool:
        return (
            self._catalog is not None
            and name.isidentifier()
            and not name.startswith("_")
            and self._catalog.knows(name)
        )

    def _catalog_unit_names(self) -> list[str]:
        """The names of the catalog that can be written as an attribute."""
        if self._catalog is None:
            return []
        if self._catalog_names is None:
            self._catalog_names = [
                name
                for name in self._catalog.resolvable_names()
                if name.isidentifier() and not keyword.iskeyword(name)
            ]
        return self._catalog_names

    def discover_plugins(self) -> None:
        """Scans entry points for the group 'physure.units'.

        This method populates the lazy loaders without importing the modules.
        """
        if self._discovered:
            return

        try:
            # importlib.metadata.entry_points(group=...) is standard since 3.10
            eps = importlib.metadata.entry_points(group="physure.units")
            for ep in eps:
                if (
                    ep.name not in self._registry
                    and ep.name not in self._lazy_loaders
                ):
                    self._lazy_loaders[ep.name] = ep
        except Exception:
            log.exception("Error discovering unit plugins")

        self._discovered = True

    def __getattr__(self, name: str) -> Any:
        """Retrieves a unit from the registry, loading it if necessary."""
        if name in self._registry:
            return self._registry[name]

        if self._make_unit is not None and self._in_catalog(name):
            unit = self._make_unit(name)
            self._registry[name] = unit
            return unit

        # Trigger discovery lazily
        if not self._discovered:
            self.discover_plugins()

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
                log.exception("Failed to load unit plugin '%s'", name)
                raise AttributeError(
                    f"Unit '{name}' failed to load: {e}"
                ) from e

        raise AttributeError(f"Unit '{name}' not found in registry.")

    @property
    def available_units(self) -> list[str]:
        """Returns a list of all available unit names."""
        if not self._discovered:
            self.discover_plugins()
        return sorted(
            set(self._registry)
            | set(self._lazy_loaders)
            | set(self._catalog_unit_names())
        )

    def __dir__(self) -> list[str]:
        """Lists all available units for discovery (e.g. in notebooks)."""
        if not self._discovered:
            self.discover_plugins()
        return sorted(
            set(self._registry)
            | set(self._lazy_loaders)
            | set(self._catalog_unit_names())
        )
