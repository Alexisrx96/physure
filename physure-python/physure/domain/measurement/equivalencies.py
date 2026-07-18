"""Dimension-crossing equivalencies (astropy-style) for Quantity.to().

An equivalency is a tuple ``(dim_a, dim_b, a_to_b, b_to_a)`` where the
callables map magnitudes expressed in the *base units* of each dimension.
Activate them with the :func:`equivalencies` context manager or pass them
directly to ``Quantity.to(..., equivalencies=...)``.
"""

from __future__ import annotations

from contextlib import contextmanager
from contextvars import ContextVar
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Callable, Generator, Sequence

    from physure.core.protocols import Numeric
    from physure.domain.measurement.dimensions import Dimension

EquivalencyEntry = tuple[
    "Dimension",
    "Dimension",
    "Callable[[Numeric], Numeric]",
    "Callable[[Numeric], Numeric]",
]
EquivalencyList = list[EquivalencyEntry]

_ACTIVE_EQUIVALENCIES: ContextVar[tuple[EquivalencyEntry, ...]] = ContextVar(
    "active_equivalencies", default=()
)


def spectral() -> EquivalencyList:
    """Equivalencies between wavelength, frequency, wavenumber and energy."""
    c = 299792458.0  # speed of light in vacuum in m/s
    h = 6.62607015e-34  # planck_constant in J*s

    from physure.domain.measurement.dimensions import Dimension

    dim_length = Dimension({"L": 1})
    dim_inv_time = Dimension({"T": -1})
    dim_energy = Dimension({"M": 1, "L": 2, "T": -2})
    dim_inv_length = Dimension({"L": -1})

    return [
        # L <-> T^-1
        (dim_length, dim_inv_time, lambda x: c / x, lambda x: c / x),
        # L <-> E
        (dim_length, dim_energy, lambda x: h * c / x, lambda x: h * c / x),
        # T^-1 <-> E
        (dim_inv_time, dim_energy, lambda x: h * x, lambda x: x / h),
        # L <-> L^-1
        (dim_length, dim_inv_length, lambda x: 1.0 / x, lambda x: 1.0 / x),
        # L^-1 <-> T^-1
        (dim_inv_length, dim_inv_time, lambda x: c * x, lambda x: x / c),
        # L^-1 <-> E
        (
            dim_inv_length,
            dim_energy,
            lambda x: h * c * x,
            lambda x: x / (h * c),
        ),
    ]


def thermodynamic() -> EquivalencyList:
    """Equivalency between temperature and energy (E = k_b * T)."""
    k_b = 1.380649e-23
    from physure.domain.measurement.dimensions import Dimension

    dim_temp = Dimension({"O": 1})
    dim_energy = Dimension({"M": 1, "L": 2, "T": -2})

    return [(dim_temp, dim_energy, lambda x: k_b * x, lambda x: x / k_b)]


@contextmanager
def equivalencies(
    *eq_lists: EquivalencyList | Callable[[], EquivalencyList],
) -> Generator[None]:
    """Context manager activating equivalencies for Quantity.to() calls."""
    flat_eqs: EquivalencyList = []
    for eq_list in eq_lists:
        resolved = eq_list if isinstance(eq_list, list) else eq_list()
        flat_eqs.extend(resolved)

    current = _ACTIVE_EQUIVALENCIES.get()
    token = _ACTIVE_EQUIVALENCIES.set((*current, *flat_eqs))
    try:
        yield
    finally:
        _ACTIVE_EQUIVALENCIES.reset(token)


def find_conversion_path(
    dim_from: Dimension,
    dim_to: Dimension,
    active_eqs: Sequence[EquivalencyEntry],
) -> list[Callable[[Numeric], Numeric]] | None:
    """BFS over active equivalencies; returns the list of magnitude maps.

    Returns None when no path connects the two dimensions.
    """
    graph: dict[
        Dimension, list[tuple[Dimension, Callable[[Numeric], Numeric]]]
    ] = {}
    for d1, d2, to_f, from_f in active_eqs:
        if d1 not in graph:
            graph[d1] = []
        if d2 not in graph:
            graph[d2] = []
        graph[d1].append((d2, to_f))
        graph[d2].append((d1, from_f))

    if dim_from not in graph or dim_to not in graph:
        return None

    queue: list[tuple[Dimension, list[Callable[[Numeric], Numeric]]]] = [
        (dim_from, [])
    ]
    visited = {dim_from}

    while queue:
        current, path = queue.pop(0)
        if current == dim_to:
            return path
        for neighbor, func in graph.get(current, []):
            if neighbor not in visited:
                visited.add(neighbor)
                queue.append((neighbor, [*path, func]))

    return None


def numerical_derivative(
    f: Callable[[Numeric], Numeric], x: Numeric, dx: float = 1e-8
) -> Numeric:
    """Central-difference derivative, used to propagate uncertainty."""
    h = abs(x) * dx or dx  # falls back to dx at exactly zero
    # ponytail: Numeric is a union of mutually-incompatible backend types
    # (Tensor/Array/ndarray/float); pyright can't prove x and h share one
    # concrete member, but callers always pass a single homogeneous backend.
    return (f(x + h) - f(x - h)) / (2 * h)  # pyright: ignore[reportOperatorIssue, reportUnknownArgumentType, reportUnknownVariableType]
