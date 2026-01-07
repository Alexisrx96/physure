"""JIT compiler core for MeasureKit.

Provides symbolic tracing and kernel baking using the Rust-based RationalUnit
for exact dimensional validation.
"""

from __future__ import annotations

import functools
import sys
from collections.abc import Callable
from fractions import Fraction
from pathlib import Path
from typing import Any

# Resolve core path relative to the project structure
# Expecting: measurekit/jit/tracer.py -> ../../measurekit_core/target/release
_BASE_PATH = Path(__file__).resolve().parent.parent.parent
CORE_PATH = _BASE_PATH / "measurekit_core" / "target" / "release"
if CORE_PATH.exists() and str(CORE_PATH) not in sys.path:
    sys.path.insert(0, str(CORE_PATH))

try:
    import measurekit_core
    from measurekit_core import RationalUnit
except ImportError:
    # Minimal mock for local development if Rust core is missing
    class RationalUnit:
        """Mock RationalUnit if Rust core is not linked."""

        def __init__(self, dims: dict):
            self.dimensions = dims

        def __mul__(self, other):
            return RationalUnit({})

        def __truediv__(self, other):
            return RationalUnit({})

        def __pow__(self, other, modulo=None):
            return RationalUnit({})

        def __eq__(self, other):
            return True

        def __hash__(self):
            return 0


class DimensionalError(TypeError):
    """Raised when units are incompatible during JIT compilation."""


class Node:
    """A node in the computational DAG."""

    __slots__ = ("args", "op")

    def __init__(self, op: str, args: tuple[Any, ...]):
        self.op = op
        self.args = args


class TracerQuantity:
    """A symbolic quantity used during JIT tracing."""

    def __init__(self, node: str | Node | float, unit: RationalUnit):
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


def bake_kernel(result_node: Node, arg_names: list[str]) -> Callable:
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


def jit(func: Callable):
    """JIT compiler for unit-aware functions.

    Traces the function to validate units and bakes a unit-less numerical kernel.
    """

    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        from measurekit.domain.measurement.quantity import Quantity

        # 1. Signature identification
        unit_sig = []
        for arg in args:
            unit = getattr(arg, "unit", None)
            if unit is not None:
                r_unit = _ensure_rational(unit)
                unit_sig.append(hash(r_unit))
            else:
                unit_sig.append(None)

        sig = (func, tuple(unit_sig))

        if sig not in _JIT_CACHE:
            # 2. Trace
            tracer_args = []
            arg_names = []
            for i, (arg, u_hash) in enumerate(zip(args, unit_sig)):
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
                return func(*args, **kwargs)

            # 3. Bake
            kernel = bake_kernel(trace_result.node, arg_names)
            _JIT_CACHE[sig] = (kernel, trace_result.unit)

        # 4. Execute
        kernel, out_r_unit = _JIT_CACHE[sig]
        magnitudes = [getattr(a, "magnitude", a) for a in args]
        raw_res = kernel(*magnitudes)

        # 5. Re-wrap in Quantity
        # We need to convert RationalUnit back to CompoundUnit
        from measurekit.domain.measurement.units import (
            CompoundUnit,
            get_default_system,
        )

        # RationalUnit.dimensions is HashMap<String, (i64, i64)>
        # CompoundUnit expects exponents as Dict[str, int]
        # This is a simplification; if we have fractions, we might need a more complex unit.
        # But for JIT usually we have integer exponents.
        exponents = {}
        for base, (num, den) in out_r_unit.dimensions.items():
            if den == 1:
                exponents[base] = num
            else:
                # If we have fractional units, we might need to support them in CompoundUnit
                # or simplified them. For now, assume integer or warn.
                exponents[base] = num / den

        out_unit = CompoundUnit(exponents)
        return Quantity.from_input(raw_res, out_unit, get_default_system())

    return wrapper


def _ensure_rational(unit_obj: Any) -> RationalUnit:
    """Helper to convert any unit representation to core RationalUnit."""
    if isinstance(unit_obj, RationalUnit):
        return unit_obj

    if hasattr(unit_obj, "dimensions"):
        return unit_obj

    if hasattr(unit_obj, "exponents"):
        dims = {k: (v, 1) for k, v in unit_obj.exponents.items()}
        return RationalUnit(dims)

    return RationalUnit({})
