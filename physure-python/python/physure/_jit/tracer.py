"""JIT compiler core for Physure.

Provides symbolic tracing and kernel baking using the Rust-based RationalUnit
for exact dimensional validation.
"""

from __future__ import annotations

import functools
from fractions import Fraction
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Callable

try:
    # ponytail: the Rust RationalUnit and the pure-Python fallback below
    # are structurally different types by design (Rust core is always
    # optional); callers only ever see one or the other.
    from physure._core import (
        RationalUnit,  # pyright: ignore[reportAssignmentType]
    )
except ImportError:

    class RationalUnit:
        """Python fallback for RationalUnit when extension is missing."""

        def __init__(self, dimensions: dict[str, tuple[int, int]]) -> None:
            self.dimensions = dimensions

        def __repr__(self) -> str:
            return f"RationalUnit({self.dimensions})"

        def __hash__(self) -> int:
            return hash(tuple(sorted(self.dimensions.items())))

        def __eq__(self, other: object) -> bool:
            if isinstance(other, RationalUnit):
                return self.dimensions == other.dimensions
            return False

        def __mul__(self, other: object) -> RationalUnit:
            if not isinstance(other, RationalUnit):
                return NotImplemented
            new_dims = self.dimensions.copy()
            for k, v in other.dimensions.items():
                f1 = Fraction(*new_dims.get(k, (0, 1)))
                f2 = Fraction(*v)
                res = f1 + f2
                if res == 0:
                    if k in new_dims:
                        del new_dims[k]
                else:
                    new_dims[k] = (res.numerator, res.denominator)
            return RationalUnit(new_dims)

        def __truediv__(self, other: object) -> RationalUnit:
            if not isinstance(other, RationalUnit):
                return NotImplemented
            new_dims = self.dimensions.copy()
            for k, v in other.dimensions.items():
                f1 = Fraction(*new_dims.get(k, (0, 1)))
                f2 = Fraction(*v)
                res = f1 - f2
                if res == 0:
                    if k in new_dims:
                        del new_dims[k]
                else:
                    new_dims[k] = (res.numerator, res.denominator)
            return RationalUnit(new_dims)

        def __pow__(self, power: int | tuple[int, int]) -> RationalUnit:
            if isinstance(power, int):
                p = Fraction(power, 1)
            # ponytail: this dunder is reachable from `**` with arbitrary
            # runtime arguments regardless of the static annotation, so the
            # tuple-length guard is a real runtime check, not dead code.
            elif (
                isinstance(power, tuple)  # pyright: ignore[reportUnnecessaryIsInstance]
                and len(power) == 2
            ):
                p = Fraction(*power)
            else:
                return NotImplemented  # pyright: ignore[reportUnreachable]

            new_dims: dict[str, tuple[int, int]] = {}
            for k, v in self.dimensions.items():
                f = Fraction(*v) * p
                if f != 0:
                    new_dims[k] = (f.numerator, f.denominator)
            return RationalUnit(new_dims)


class DimensionalError(TypeError):
    """Raised when units are incompatible during JIT compilation."""


class Node:
    """A node in the computational DAG."""

    __slots__ = ("args", "op")

    def __init__(self, op: str, args: tuple[Any, ...]) -> None:
        self.op = op
        self.args = args


class TracerQuantity:
    """A symbolic quantity used during JIT tracing."""

    def __init__(self, node: str | Node | float, unit: RationalUnit) -> None:
        self.node = node
        self.unit = unit

    def __add__(self, other: Any) -> TracerQuantity:
        other_node, other_unit = self._to_tracer(other)
        if self.unit != other_unit:
            raise DimensionalError(
                f"Incompatible units: {self.unit} + {other_unit}"
            )
        return TracerQuantity(Node("add", (self.node, other_node)), self.unit)

    def __sub__(self, other: Any) -> TracerQuantity:
        other_node, other_unit = self._to_tracer(other)
        if self.unit != other_unit:
            raise DimensionalError(
                f"Incompatible units: {self.unit} - {other_unit}"
            )
        return TracerQuantity(Node("sub", (self.node, other_node)), self.unit)

    def __mul__(self, other: Any) -> TracerQuantity:
        other_node, other_unit = self._to_tracer(other)
        new_unit = self.unit * other_unit
        return TracerQuantity(Node("mul", (self.node, other_node)), new_unit)

    def __rmul__(self, other: Any) -> TracerQuantity:
        other_node, other_unit = self._to_tracer(other)
        new_unit = other_unit * self.unit
        return TracerQuantity(Node("mul", (other_node, self.node)), new_unit)

    def __truediv__(self, other: Any) -> TracerQuantity:
        other_node, other_unit = self._to_tracer(other)
        new_unit = self.unit / other_unit
        return TracerQuantity(Node("div", (self.node, other_node)), new_unit)

    def __rtruediv__(self, other: Any) -> TracerQuantity:
        other_node, other_unit = self._to_tracer(other)
        new_unit = other_unit / self.unit
        return TracerQuantity(Node("div", (other_node, self.node)), new_unit)

    def __pow__(self, exponent: Any) -> TracerQuantity:
        # Exponent must be a literal number for static unit validation
        if isinstance(exponent, (int, float)):
            if float(exponent).is_integer():
                new_unit = self.unit ** int(exponent)
            else:
                f = Fraction(exponent).limit_denominator()
                new_unit = self.unit ** (f.numerator, f.denominator)
            return TracerQuantity(Node("pow", (self.node, exponent)), new_unit)
        raise DimensionalError("Only numeric exponents supported during JIT")

    def _to_tracer(self, other: Any) -> tuple[Any, RationalUnit]:
        if isinstance(other, TracerQuantity):
            return other.node, other.unit
        return other, RationalUnit({})


def bake_kernel(
    result_node: Node | str | float, arg_names: list[str]
) -> Callable:
    """Compiles a DAG into a Python function for maximum execution speed."""
    code_lines = []
    memo = {}

    def _visit(n) -> str:
        if id(n) in memo:
            return memo[id(n)]

        if not isinstance(n, Node):
            return str(n) if not isinstance(n, str) else n

        arg_exprs = [_visit(a) for a in n.args]
        var_name = f"v{len(memo)}"

        ops = {"add": "+", "sub": "-", "mul": "*", "div": "/", "pow": "**"}
        op_sym = ops[n.op]

        code_lines.append(
            f"    {var_name} = {arg_exprs[0]} {op_sym} {arg_exprs[1]}"
        )
        memo[id(n)] = var_name
        return var_name

    final_var = _visit(result_node)
    func_def = f"def compiled_kernel({', '.join(arg_names)}):\n"
    func_def += "\n".join(code_lines) if code_lines else "    pass"
    func_def += f"\n    return {final_var}"

    namespace = {}
    exec(func_def, {}, namespace)
    return namespace["compiled_kernel"]


# Cache: (func, tuple of input unit hashes) -> (kernel, output_unit)
_JIT_CACHE: dict[
    tuple[Callable, tuple[int | None, ...]], tuple[Callable, RationalUnit]
] = {}


def _build_unit_sig(args: tuple) -> list[int | None]:
    """Builds the unit signature list from a tuple of arguments."""
    unit_sig = []
    for arg in args:
        unit = getattr(arg, "unit", None)
        if unit is not None:
            unit_sig.append(hash(_ensure_rational(unit)))
        else:
            unit_sig.append(None)
    return unit_sig


def _trace_and_bake(
    func: Callable, args: tuple, unit_sig: list, kwargs: dict
) -> tuple[Callable, RationalUnit] | None:
    """Traces func and bakes a kernel. Returns (kernel, out_unit) or None."""
    tracer_args = []
    arg_names = []
    for i, (arg, u_hash) in enumerate(zip(args, unit_sig, strict=False)):
        name = f"a{i}"
        arg_names.append(name)
        if u_hash is not None:
            tracer_args.append(
                TracerQuantity(name, _ensure_rational(arg.unit))
            )
        else:
            tracer_args.append(arg)

    trace_result = func(*tracer_args, **kwargs)
    if not isinstance(trace_result, TracerQuantity):
        return None

    kernel = bake_kernel(trace_result.node, arg_names)
    return kernel, trace_result.unit


def _rational_unit_to_exponents(
    r_unit: RationalUnit,
) -> dict[str, int | float]:
    """Converts a RationalUnit's dimensions to a CompoundUnit exponents dict."""
    exponents: dict[str, int | float] = {}
    for base, (num, den) in r_unit.dimensions.items():
        exponents[base] = num if den == 1 else num / den
    return exponents


def jit(func: Callable) -> Callable:
    """JIT compiler for unit-aware functions.

    Traces the function to validate units and bakes a unit-less numerical kernel.
    """

    @functools.wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        from physure.application.context import get_uncertainty_mode
        from physure.domain.measurement.quantity import Quantity

        unit_sig = _build_unit_sig(args)
        sig = (func, tuple(unit_sig))

        mode, _ = get_uncertainty_mode()
        if mode != "python":
            return func(*args, **kwargs)

        if sig not in _JIT_CACHE:
            result = _trace_and_bake(func, args, unit_sig, kwargs)
            if result is None:
                return func(*args, **kwargs)
            _JIT_CACHE[sig] = result

        kernel, out_r_unit = _JIT_CACHE[sig]
        magnitudes = [getattr(a, "magnitude", a) for a in args]
        raw_res = kernel(*magnitudes)

        mode, _ = get_uncertainty_mode()
        if mode != "python":
            return func(*args, **kwargs)

        from physure.domain.measurement.units import (
            CompoundUnit,
            get_default_system,
        )

        exponents = _rational_unit_to_exponents(out_r_unit)
        out_unit = CompoundUnit(exponents)
        return Quantity.from_input(raw_res, out_unit, get_default_system())

    return wrapper


def _ensure_rational(unit_obj: Any) -> RationalUnit:
    """Helper to convert any unit representation to core RationalUnit."""
    if isinstance(unit_obj, RationalUnit):
        ret = unit_obj
    elif hasattr(unit_obj, "exponents"):
        # We handle both integer and fractional exponents
        dims = {}
        for k, v in unit_obj.exponents.items():
            if isinstance(v, tuple):  # Already (num, den)
                dims[k] = v
            elif isinstance(v, float) and not v.is_integer():
                from fractions import Fraction

                f = Fraction(v).limit_denominator()
                dims[k] = (f.numerator, f.denominator)
            else:
                dims[k] = (int(v), 1)
        ret = RationalUnit(dims)
    else:
        ret = RationalUnit({})
    return ret
