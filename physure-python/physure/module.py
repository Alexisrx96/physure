"""Ergonomic Python interface for `.phs` modules.

Wraps the raw PyO3 binding `physure._core.PhsModuleCore` (Task 4 of
`docs/superpowers/plans/2026-08-27-phs-foreign-bridge.md`) so a `.phs` file's functions can
be called like ordinary Python functions/methods -- `mod.some_fn(x=..., y=...)` -- using and
returning real physure `Quantity` objects, not the bare `physure._core.Quantity` the Rust
FFI boundary itself speaks.

Not imported eagerly: `physure/__init__.py` only loads this module (and the heavier
`physure.domain`/`physure._jit` imports pulled in below) on first access to `load_phs`/
`load_dir`, to keep the ~0.5s first-use import budget in the root CLAUDE.md's Philosophy
section intact.
"""

from __future__ import annotations

import inspect
from pathlib import Path
from typing import Any

from physure._core import PhsModuleCore
from physure._core import Quantity as _RustCoreQuantity


def _to_core_quantity(value: Any) -> Any:
    """Bridges a domain-level ``Quantity`` into a real ``physure._core.Quantity``.

    ``PhsModuleCore.invoke()``'s argument converter (``py_to_phs_value`` in
    ``physure-python/src/lib.rs``) only recognizes an object as a PHS quantity if it IS a
    ``physure._core.Quantity`` instance. ``physure.domain.measurement.quantity.Quantity`` --
    what ``Q_()`` and unit multiplication (``10 * kg``) actually return -- is a separate,
    composition-based class whose magnitude stays a plain Python ``float`` in the default
    ("python") propagation mode, and which defines ``__float__``. Passed straight through, it
    falls into ``py_to_phs_value``'s ``f64`` branch: the unit (and any dimension check) is
    silently dropped instead of raising. Confirmed by hand: invoking a ``kg``-declared
    parameter with a ``Quantity`` built in meters returns a bogus dimensionless result rather
    than failing.

    Reconstructing a real core ``Quantity`` here -- using the same magnitude/unit/std_dev
    fields and ``RationalUnit`` construction that
    ``Quantity._maybe_wrap_in_rust_core``/``_ensure_rational`` already use for the library's
    own Rust-backed (Monte Carlo/unscented/Gaussian mode) construction -- keeps the dimension
    check intact at the FFI boundary instead of reimplementing it.

    NOTE on a separate, pre-existing `physure-script` bug found while building this bridge
    (out of scope here -- see the Task 5 report in
    `docs/superpowers/plans/2026-08-27-phs-foreign-bridge.md`): a `.phs` function parameter
    declared with a *named/derived/prefixed* unit ("N", "Pa", "bar", "mm", "km", "g", ...)
    will reject a dimensionally-identical foreign `Quantity` in that same unit, because
    `bind_param_value` (`physure-script/src/interpreter/expressions.rs`) parses the declared
    unit with the registry-*expanding* parser while every foreign constructor
    (`parse_unit_expression`, `UnitRegistry.get_unit`, this module) uses the *atomic* one, and
    `RationalUnit::same_dimensions` compares raw symbol keys rather than reduced dimensions.
    Converting to base SI units before bridging would dodge that mismatch, but was tried and
    rejected: the resulting core `Quantity` can carry a scale `CompoundUnit.from_rational_unit`
    (used by `_to_domain_value` below) does not reconstruct, which silently produced a wrong
    answer instead of the honest `ValueError` this now raises -- worse, per this project's
    "wrong answer with confident units" rule. A `.phs` function declared entirely in base SI
    units (kg, m, s, ...) is unaffected and works correctly today.
    """
    from physure._jit.tracer import _ensure_rational
    from physure.domain.measurement.quantity import Quantity as DomainQuantity

    if isinstance(value, DomainQuantity):
        r_unit = _ensure_rational(value.unit)
        std_dev = float(value.std_dev or 0.0)
        return _RustCoreQuantity(float(value.magnitude), r_unit, std_dev)
    return value


def _to_domain_value(value: Any) -> Any:
    """Promotes a raw ``physure._core.Quantity`` PHS result into the domain ``Quantity``.

    ``phs_value_to_py`` (``physure-python/src/lib.rs``) returns the bare
    ``physure._core.Quantity`` for a PHS ``Quantity`` result -- it has ``.magnitude``/
    ``.unit`` but no ``.to()``/conversion method at all. The rest of physure-python's public
    API (``Q_()`` and everything built on it) hands out
    ``physure.domain.measurement.quantity.Quantity``, which does. Promote here via
    ``CompoundUnit.from_rational_unit``, the library's own documented bridge from a Rust
    ``RationalUnit`` to the domain unit type, so `load_phs(...).fn(...)` results behave like
    every other physure Quantity (chainable into further `load_phs` calls, convertible with
    `.to()`, printable) instead of a second, thinner quantity type.
    """
    if isinstance(value, _RustCoreQuantity):
        from physure.application.context import get_active_system
        from physure.domain.measurement.quantity import (
            Quantity as DomainQuantity,
        )
        from physure.domain.measurement.units import CompoundUnit

        unit = CompoundUnit.from_rational_unit(value.unit)
        return DomainQuantity(
            magnitude=value,
            unit=unit,
            uncertainty=value.std_dev,
            system=get_active_system(),
        )
    return value


def _coerce_scalar(arg: Any) -> Any:
    """Coerces a unit-string argument into a real Quantity.

    Turns a ``"5 bar"``-style string into a real ``Quantity``; a bare numeric string into a
    ``float``; leaves everything else (``Quantity``, ``float``, ``bool``, text ``str``,
    list, ...) alone.
    """
    if isinstance(arg, str):
        if " " in arg:
            mag_str, unit_str = arg.split(" ", 1)
            try:
                from physure import Q_

                return Q_(float(mag_str), unit_str)
            except Exception:
                return arg
        try:
            return float(arg)
        except ValueError:
            return arg
    return arg


class PhsFunctionWrapper:
    """Callable proxy for a single function exported by a `.phs` module."""

    def __init__(
        self,
        module_core: PhsModuleCore,
        name: str,
        params: list[str],
        docstring: str | None = None,
    ) -> None:
        self._core = module_core
        self._name = name
        self._params = params
        self.__name__ = name
        self.__doc__ = docstring or f"PHS function {name}({', '.join(params)})"
        self.__signature__ = inspect.Signature(
            [
                inspect.Parameter(p, inspect.Parameter.POSITIONAL_OR_KEYWORD)
                for p in params
            ]
        )

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        """Invokes the wrapped `.phs` function with positional or keyword arguments."""
        if len(args) > len(self._params):
            raise TypeError(
                f"{self._name}() takes {len(self._params)} positional arguments "
                f"but {len(args)} were given"
            )

        # Check for unexpected or duplicated keyword arguments
        for k in kwargs:
            if k not in self._params:
                raise TypeError(
                    f"{self._name}() got an unexpected keyword argument {k!r}"
                )
            idx = self._params.index(k)
            if idx < len(args):
                raise TypeError(
                    f"{self._name}() got multiple values for argument {k!r}"
                )

        ordered_args: list[Any] = list(args)
        for i in range(len(args), len(self._params)):
            p = self._params[i]
            if p in kwargs:
                ordered_args.append(kwargs[p])
            else:
                raise ValueError(
                    f"Missing required parameter '{p}' for {self._name}()"
                )

        coerced = [_to_core_quantity(_coerce_scalar(a)) for a in ordered_args]
        result = self._core.invoke(self._name, coerced)
        return _to_domain_value(result)


class PhsModuleWrapper:
    """Ergonomic wrapper around a loaded `.phs` module: ``mod.some_fn(x=..., y=...)``."""

    def __init__(self, core: PhsModuleCore) -> None:
        self._core = core
        self._fn_cache: dict[str, PhsFunctionWrapper] = {}
        for fn in self._core.list_functions():
            params = self._core.get_params(fn)
            self._fn_cache[fn] = PhsFunctionWrapper(self._core, fn, params)

    def __getattr__(self, item: str) -> PhsFunctionWrapper:
        if item in self._fn_cache:
            return self._fn_cache[item]
        raise AttributeError(f"Module has no function {item!r}")

    def __dir__(self) -> list[str]:
        return [*super().__dir__(), *self._fn_cache.keys()]


def load_phs(path: str | Path) -> PhsModuleWrapper:
    """Loads a single `.phs` file, returning its functions as callables.

    Examples:
        >>> from pathlib import Path
        >>> import tempfile
        >>> with tempfile.TemporaryDirectory() as d:
        ...     p = Path(d) / "geom.phs"
        ...     _ = p.write_text("fn area(w, h) = w * h")
        ...     mod = load_phs(p)
        ...     mod.area(3.0, 4.0)
        12.0
    """
    core = PhsModuleCore.from_file(str(path))
    return PhsModuleWrapper(core)


def load_dir(path: str | Path) -> dict[str, PhsModuleWrapper]:
    """Loads every top-level `.phs` file in ``path``, keyed by filename stem.

    A thin convenience over calling `load_phs` once per file -- there is no
    cross-module composition/import graph here (no ``PhsProject``); that is out of scope
    for this task (see Tasks 6/7 of the foreign-bridge plan).
    """
    directory = Path(path)
    return {p.stem: load_phs(p) for p in sorted(directory.glob("*.phs"))}
