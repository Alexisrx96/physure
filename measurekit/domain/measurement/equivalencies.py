"""Dimension-crossing equivalencies (astropy-style) for Quantity.to().

An equivalency is a tuple ``(dim_a, dim_b, a_to_b, b_to_a)`` where the
callables map magnitudes expressed in the *base units* of each dimension.
Activate them with the :func:`equivalencies` context manager or pass them
directly to ``Quantity.to(..., equivalencies=...)``.
"""

from contextlib import contextmanager
from contextvars import ContextVar

_ACTIVE_EQUIVALENCIES: ContextVar[tuple] = ContextVar(
    "active_equivalencies", default=()
)


def spectral():
    """Equivalencies between wavelength, frequency, wavenumber and energy."""
    c = 299792458.0  # speed of light in vacuum in m/s
    h = 6.62607015e-34  # planck_constant in J*s

    from measurekit.domain.measurement.dimensions import Dimension

    dim_L = Dimension({"L": 1})
    dim_T_inv = Dimension({"T": -1})
    dim_E = Dimension({"M": 1, "L": 2, "T": -2})
    dim_L_inv = Dimension({"L": -1})

    return [
        # L <-> T^-1
        (dim_L, dim_T_inv, lambda x: c / x, lambda x: c / x),
        # L <-> E
        (dim_L, dim_E, lambda x: h * c / x, lambda x: h * c / x),
        # T^-1 <-> E
        (dim_T_inv, dim_E, lambda x: h * x, lambda x: x / h),
        # L <-> L^-1
        (dim_L, dim_L_inv, lambda x: 1.0 / x, lambda x: 1.0 / x),
        # L^-1 <-> T^-1
        (dim_L_inv, dim_T_inv, lambda x: c * x, lambda x: x / c),
        # L^-1 <-> E
        (dim_L_inv, dim_E, lambda x: h * c * x, lambda x: x / (h * c)),
    ]


def thermodynamic():
    """Equivalency between temperature and energy (E = k_B * T)."""
    k_B = 1.380649e-23
    from measurekit.domain.measurement.dimensions import Dimension

    dim_temp = Dimension({"O": 1})
    dim_E = Dimension({"M": 1, "L": 2, "T": -2})

    return [(dim_temp, dim_E, lambda x: k_B * x, lambda x: x / k_B)]


@contextmanager
def equivalencies(*eq_lists):
    """Context manager activating equivalencies for Quantity.to() calls."""
    flat_eqs = []
    for eq_list in eq_lists:
        if callable(eq_list):
            eq_list = eq_list()
        flat_eqs.extend(eq_list)

    current = _ACTIVE_EQUIVALENCIES.get()
    token = _ACTIVE_EQUIVALENCIES.set((*current, *flat_eqs))
    try:
        yield
    finally:
        _ACTIVE_EQUIVALENCIES.reset(token)


def find_conversion_path(dim_from, dim_to, active_eqs):
    """BFS over active equivalencies; returns the list of magnitude maps.

    Returns None when no path connects the two dimensions.
    """
    graph = {}
    for d1, d2, to_f, from_f in active_eqs:
        if d1 not in graph:
            graph[d1] = []
        if d2 not in graph:
            graph[d2] = []
        graph[d1].append((d2, to_f))
        graph[d2].append((d1, from_f))

    if dim_from not in graph or dim_to not in graph:
        return None

    queue = [(dim_from, [])]
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


def numerical_derivative(f, x, dx=1e-8):
    """Central-difference derivative, used to propagate uncertainty."""
    if x == 0.0:
        return (f(dx) - f(-dx)) / (2 * dx)
    h = x * dx
    return (f(x + h) - f(x - h)) / (2 * h)
