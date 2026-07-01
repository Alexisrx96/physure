"""Arithmetic and uncertainty propagation mixin for Quantity."""

from __future__ import annotations

import contextlib
from typing import TYPE_CHECKING, Any, cast

if TYPE_CHECKING:
    from measurekit.domain.measurement.quantity import Quantity

from typing_extensions import Self

from measurekit.domain.exceptions import (
    DimensionError,
    IncompatibleUnitsError,
)
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
)
from measurekit.domain.measurement.uncertainty import Uncertainty
from measurekit.domain.measurement.units import CompoundUnit

_Quantity = None


def _q():
    global _Quantity
    if _Quantity is None:
        from measurekit.domain.measurement.quantity import Quantity

        _Quantity = Quantity
    return _Quantity


class ArithmeticMixin:
    """Arithmetic operators, uncertainty propagation, and transcendentals."""

    # ------------------------------------------------------------------
    # Private helpers shared across arithmetic operators
    # ------------------------------------------------------------------

    @staticmethod
    def _is_zero_uncertainty(u: Any) -> bool:
        """Return True when *u* is absent or a scalar zero."""
        return u is None or (isinstance(u, (int, float)) and not u)

    def _record_op(self, op_name: str, other: Any, res: Any) -> None:
        """Record an arithmetic operation in the active tracer (if any)."""
        from measurekit.application.tracing.context import get_active_tracer

        if (tracer := get_active_tracer()) is not None:
            tracer.record_operation(
                op_name, operands=(self, other), result=res
            )

    def _add_sub_array_path(
        self,
        other: Any,
        new_magnitude: Any,
        j_self: Any,
        j_other: Any,
        op_name: str,
    ) -> Any:
        """Vectorized uncertainty propagation for add/sub (same unit)."""
        new_unc = self._propagate_vectorized(
            other, new_magnitude, j_self, j_other
        )
        res = self._fast_new(
            new_magnitude,
            self.unit,
            new_unc,
            self.system,
            self.dimension,
            self._backend,
        )
        if self._uncertainty_obj is not None:
            res._uncertainty_obj = self._uncertainty_obj.add(
                other._uncertainty_obj,
                jac_self=j_self,
                jac_other=j_other,
                out_magnitude=new_magnitude,
            )
        self._record_op(op_name, other, res)
        return res

    def _add_sub_scalar_path(
        self,
        other: Any,
        new_magnitude: Any,
        u_other: Any,
        jac_self: float,
        jac_other: float,
        op_name: str,
    ) -> Any:
        """Scalar uncertainty propagation for add/sub when both share the same unit."""
        u_obj_other = other._uncertainty_obj
        new_u_obj = None
        if self._uncertainty_obj is not None:
            new_u_obj = self._uncertainty_obj.add(
                u_obj_other,
                jac_self=jac_self,
                jac_other=jac_other,
                out_magnitude=new_magnitude,
            )
            res_unc = new_u_obj.std_dev
        else:
            sum_var = self._backend.add(
                self._backend.pow(self.uncertainty, 2),
                self._backend.pow(u_other, 2),
            )
            res_unc = self._backend.pow(sum_var, 0.5)

        res = self._fast_new(
            new_magnitude,
            self.unit,
            res_unc,
            self.system,
            self.dimension,
            self._backend,
        )
        if new_u_obj is not None:
            res._uncertainty_obj = new_u_obj
        self._record_op(op_name, other, res)
        return res

    def _mul_numeric_path(
        self,
        other: Any,
        new_magnitude: Any,
        new_unit: Any,
        j_self: Any,
        op_name: str,
    ) -> Any:
        """Build and return a result for Quantity * scalar/array-numeric."""
        if self._backend.is_array(new_magnitude):
            new_unc = self._propagate_vectorized(
                None, new_magnitude, j_self, None
            )
            res = self._fast_new(
                new_magnitude,
                new_unit,
                new_unc,
                self.system,
                self.dimension,
                self._backend,
            )
            if self._uncertainty_obj is not None:
                res._uncertainty_obj = self._uncertainty_obj.propagate_mul_div(
                    None,
                    self.magnitude,
                    other,
                    new_magnitude,
                    jac_self=j_self,
                    jac_other=0.0,
                )
            self._record_op(op_name, other, res)
            return res

        # Scalar path
        new_u_obj = None
        if self._uncertainty_obj is not None:
            new_u_obj = self._uncertainty_obj.propagate_mul_div(
                None,
                self.magnitude,
                other,
                new_magnitude,
                jac_self=other,
                jac_other=0.0,
            )
            res_unc = new_u_obj.std_dev
        else:
            res_unc = self._backend.mul(
                self.uncertainty, self._backend.abs(other)
            )

        res = self._fast_new(
            new_magnitude,
            new_unit,
            res_unc,
            self.system,
            self.dimension,
            self._backend,
        )
        if new_u_obj is not None:
            res._uncertainty_obj = new_u_obj
        self._record_op(op_name, other, res)
        return res

    def _mul_qq_path(
        self,
        other: Any,
        new_magnitude: Any,
        new_unit: Any,
        new_dimension: Any,
        j_self: Any,
        j_other: Any,
        op_name: str,
    ) -> Any:
        """Build and return a result for Quantity * Quantity (array or scalar)."""
        if self._backend.is_array(new_magnitude):
            new_unc = self._propagate_vectorized(
                other, new_magnitude, j_self, j_other
            )
            res = self._fast_new(
                new_magnitude,
                new_unit,
                new_unc,
                self.system,
                new_dimension,
                self._backend,
            )
            if self._uncertainty_obj is not None:
                res._uncertainty_obj = self._uncertainty_obj.propagate_mul_div(
                    other._uncertainty_obj,
                    self.magnitude,
                    other.magnitude,
                    new_magnitude,
                    jac_self=j_self,
                    jac_other=j_other,
                )
            self._record_op(op_name, other, res)
            return res

        # Scalar path
        new_u_obj = None
        if self._uncertainty_obj is not None:
            new_u_obj = self._uncertainty_obj.propagate_mul_div(
                other._uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_magnitude,
                jac_self=other.magnitude,
                jac_other=self.magnitude,
            )
            res_unc = new_u_obj.std_dev
        else:
            var_self = self._backend.pow(
                self._backend.mul(self.uncertainty, other.magnitude), 2
            )
            var_other = self._backend.pow(
                self._backend.mul(self.magnitude, other.uncertainty), 2
            )
            res_unc = self._backend.pow(
                self._backend.add(var_self, var_other), 0.5
            )

        res = self._fast_new(
            new_magnitude,
            new_unit,
            res_unc,
            self.system,
            new_dimension,
            self._backend,
        )
        if new_u_obj is not None:
            res._uncertainty_obj = new_u_obj
        self._record_op(op_name, other, res)
        return res

    def _div_numeric_path(
        self,
        other: Any,
        new_magnitude: Any,
        new_unit: Any,
        j_self: Any,
        op_name: str,
    ) -> Any:
        """Build and return a result for Quantity / scalar/array-numeric."""
        if self._backend.is_array(new_magnitude):
            new_unc = self._propagate_vectorized(
                None, new_magnitude, j_self, None
            )
            res = self._fast_new(
                new_magnitude,
                new_unit,
                new_unc,
                self.system,
                self.dimension,
                self._backend,
            )
            if self._uncertainty_obj is not None:
                res._uncertainty_obj = self._uncertainty_obj.propagate_mul_div(
                    None,
                    self.magnitude,
                    other,
                    new_magnitude,
                    jac_self=j_self,
                    jac_other=0.0,
                )
            self._record_op(op_name, other, res)
            return res

        # Scalar path
        new_u_obj = None
        if self._uncertainty_obj is not None:
            new_u_obj = self._uncertainty_obj.propagate_mul_div(
                None,
                self.magnitude,
                other,
                new_magnitude,
                jac_self=j_self,
                jac_other=0.0,
            )
            res_unc = new_u_obj.std_dev
        else:
            res_unc = self._backend.mul(self.uncertainty, abs(j_self))

        res = self._fast_new(
            new_magnitude,
            new_unit,
            res_unc,
            self.system,
            self.dimension,
            self._backend,
        )
        if new_u_obj is not None:
            res._uncertainty_obj = new_u_obj
        self._record_op(op_name, other, res)
        return res

    def _div_qq_path(
        self,
        other: Any,
        new_magnitude: Any,
        new_unit: Any,
        new_dimension: Any,
        j_self: Any,
        j_other: Any,
        op_name: str,
    ) -> Any:
        """Build and return a result for Quantity / Quantity (array or scalar)."""
        if self._backend.is_array(new_magnitude):
            new_unc = self._propagate_vectorized(
                other, new_magnitude, j_self, j_other
            )
            res = self._fast_new(
                new_magnitude,
                new_unit,
                new_unc,
                self.system,
                new_dimension,
                self._backend,
            )
            if self._uncertainty_obj is not None:
                res._uncertainty_obj = self._uncertainty_obj.propagate_mul_div(
                    other._uncertainty_obj,
                    self.magnitude,
                    other.magnitude,
                    new_magnitude,
                    jac_self=j_self,
                    jac_other=j_other,
                )
            self._record_op(op_name, other, res)
            return res

        # Scalar path
        new_u_obj = None
        if self._uncertainty_obj is not None:
            new_u_obj = self._uncertainty_obj.propagate_mul_div(
                other._uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_magnitude,
                jac_self=j_self,
                jac_other=j_other,
            )
            res_unc = new_u_obj.std_dev
        else:
            var_self = self._backend.pow(self.uncertainty / other.magnitude, 2)
            var_other = self._backend.pow(
                self.magnitude * other.uncertainty / (other.magnitude**2), 2
            )
            res_unc = self._backend.pow(
                self._backend.add(var_self, var_other), 0.5
            )

        res = self._fast_new(
            new_magnitude,
            new_unit,
            res_unc,
            self.system,
            new_dimension,
            self._backend,
        )
        if new_u_obj is not None:
            res._uncertainty_obj = new_u_obj
        self._record_op(op_name, other, res)
        return res

    @staticmethod
    def _densify_element(x: Any) -> float:
        """Collapse a sparse/array element to a plain Python float."""
        if hasattr(x, "toarray"):  # Sparse matrix
            return x.toarray().item() if x.size == 1 else x.toarray().sum()
        if hasattr(x, "item") and x.size == 1:  # Scalar-like array
            return x.item()
        if hasattr(x, "sum") and hasattr(x, "shape") and x.shape != ():
            return x.sum()
        return x

    @staticmethod
    def _ensure_float64(arr: Any) -> Any:
        """Convert an object-dtype array to float64 in-place when possible."""
        if not (hasattr(arr, "dtype") and arr.dtype == object):
            return arr
        try:
            return arr.astype("float64")
        except (ValueError, TypeError):
            pass
        try:
            import numpy as np

            flat = [
                float(ArithmeticMixin._densify_element(x)) for x in arr.ravel()
            ]
            return np.array(flat).reshape(arr.shape).astype("float64")
        except Exception:
            return arr

    @staticmethod
    def _fix_matrix_term(res_var: Any, term2: Any) -> Any:
        """Collapse a 2-D square *term2* to its diagonal when *res_var* is 1-D."""
        is_matrix_mismatch = (
            hasattr(res_var, "shape")
            and hasattr(term2, "shape")
            and term2.ndim == 2
            and res_var.ndim == 1
            and term2.shape[0] == term2.shape[1] == res_var.shape[0]
        )
        if not is_matrix_mismatch:
            return term2
        try:
            import numpy as np

            return (
                term2.diagonal()
                if hasattr(term2, "diagonal")
                else np.diag(term2)
            )
        except Exception:
            return term2

    def _propagate_rich_model(
        self,
        other: Any,
        is_other_q: bool,
        u_obj_other: Any,
        jac_self: Any,
        jac_other: Any,
        out_magnitude: Any,
    ) -> Any | None:
        """Return std_dev via rich uncertainty model, or None to fall through."""
        if self._uncertainty_obj is None and u_obj_other is None:
            return None

        if self._uncertainty_obj is not None:
            # Upgrade other if needed
            u_other = u_obj_other
            j_other = jac_other
            if u_other is None:
                if is_other_q:
                    u_other = Uncertainty.from_standard(other.uncertainty)
                else:
                    u_other = Uncertainty.from_standard(0.0)
                    if j_other is None:
                        j_other = 0.0
            new_u_obj = self._uncertainty_obj.add(
                u_other,
                jac_self=jac_self,
                jac_other=j_other,
                out_magnitude=out_magnitude,
            )
            return new_u_obj.std_dev

        # Other has rich model (reverse)
        u_self = Uncertainty.from_standard(self.uncertainty)
        j_self_for_other = jac_other if jac_other is not None else 0.0
        new_u_obj = u_obj_other.add(
            u_self,
            jac_self=j_self_for_other,
            jac_other=jac_self,
            out_magnitude=out_magnitude,
        )
        return new_u_obj.std_dev

    def _propagate_vectorized(
        self,
        other: Any,
        out_magnitude: Any,
        jac_self: Any,
        jac_other: Any = None,
    ) -> Any:
        """Helper to propagate vectorized uncertainty."""
        is_other_q = isinstance(other, _q())
        u_obj_other = other._uncertainty_obj if is_other_q else None

        rich_result = self._propagate_rich_model(
            other, is_other_q, u_obj_other, jac_self, jac_other, out_magnitude
        )
        if rich_result is not None:
            return rich_result

        # Standard Gaussian propagation (uncorrelated fallback)
        u_self = self.uncertainty
        u_other = other.uncertainty if is_other_q else 0.0

        if self._backend.is_array(u_self):
            u_self = self._backend.reshape(u_self, (-1,))
        if self._backend.is_array(u_other):
            u_other = self._backend.reshape(u_other, (-1,))

        var_self = self._backend.asarray(self._backend.pow(u_self, 2))
        if hasattr(var_self, "astype"):
            var_self = var_self.astype("float64", copy=False)

        jac_self_sq = self._backend.pow(jac_self, 2)
        if hasattr(jac_self_sq, "astype"):
            jac_self_sq = jac_self_sq.astype("float64", copy=False)

        res_var = self._ensure_float64(
            self._backend.dot(jac_self_sq, var_self)
        )

        if jac_other is not None:
            if hasattr(u_other, "astype"):
                var_other = self._backend.pow(u_other, 2).astype(
                    "float64", copy=False
                )
            else:
                var_other = self._backend.pow(u_other, 2)

            jac_other_sq = self._backend.pow(jac_other, 2)
            if hasattr(jac_other_sq, "astype"):
                jac_other_sq = jac_other_sq.astype("float64", copy=False)

            term2 = self._ensure_float64(
                self._backend.dot(jac_other_sq, var_other)
            )
            term2 = self._fix_matrix_term(res_var, term2)
            res_var = self._backend.add(res_var, term2)

        if hasattr(self._backend, "sqrt"):
            u_out = self._backend.sqrt(res_var)
        else:
            u_out = self._backend.pow(res_var, 0.5)

        return self._backend.reshape(u_out, self._backend.shape(out_magnitude))

    def diff(self, variable: Quantity | str, order: int = 1) -> Quantity:
        """Calculates the n-th derivative with respect to a variable.

        This method uses SymPy to perform symbolic differentiation of the
        magnitude.

        Args:
            variable (Quantity | str): The variable to differentiate with
                respect to. If a Quantity is provided, its magnitude is used as
                the symbol and its unit affects the resulting unit. If a string
                is provided, it is treated as a dimensionless symbol.
            order (int): The order of differentiation (default: 1).

        Returns:
            Quantity: The derivative.

        Examples:
            >>> import sympy as sp
            >>> from measurekit import Q_
            >>> t = sp.Symbol("t")
            >>> x = Q_(t**2, "m")
            >>> v = x.diff("t")
            >>> print(v)
            2*t m
        """
        if isinstance(variable, _q()):
            d_var = variable.magnitude
            d_unit_exponents = variable.unit.exponents
        else:
            import sympy as sp

            d_var = sp.Symbol(variable)
            d_unit_exponents = {}

        # Differentiate magnitude
        try:
            import sympy as sp

            new_mag = sp.diff(self.magnitude, d_var, order)
        except Exception as e:
            # Fallback for array/tensor backends or non-symbolic magnitudes
            # For Phase 3, we focus on SymPy support.
            msg = f"Differentiation failed or not supported: {e}"
            raise NotImplementedError(msg) from e

        # Update units: u_new = u_old / (u_var)^order
        new_exponents = dict(self.unit.exponents)
        for u, e in d_unit_exponents.items():
            new_exponents[u] = new_exponents.get(u, 0) - (e * order)

        new_unit = CompoundUnit(new_exponents)

        # Check if new unit should be simplified or just return as is?
        # Usually differentiation results in meaningful units.

        return type(self).from_input(
            new_mag, new_unit, self.system, uncertainty=0.0
        )

    def _get_converter_if_simple(self):
        """Returns the converter if the unit is a single simple unit."""
        if len(self.unit.exponents) == 1:
            name, exp = next(iter(self.unit.exponents.items()))
            if exp == 1:
                # Must exclude 'noprefix' check if key is there?
                # CompoundUnit handles noprefix, exponents might have it?
                # Usually exponents dict keys are purely unit names.
                return self.system.get_definition(name).converter
        return None

    @staticmethod
    def _scalar_to_python(val: Any) -> Any:
        """Coerce a 0-d array or array scalar to a Python float, if possible."""
        if hasattr(val, "ndim") and val.ndim == 0:
            with contextlib.suppress(ValueError, TypeError):
                return float(val)
        elif hasattr(val, "item"):
            with contextlib.suppress(ValueError, TypeError):
                return val.item()
        return val

    def _affine_to_base(self, q: Any) -> Any:
        """Convert *q*'s magnitude to the base unit for its dimension."""
        conv = q._get_converter_if_simple()
        if conv:
            return conv.to_base(q.magnitude)
        factor = q.unit._compound_factor(q.system)
        return self._backend.mul(q.magnitude, factor)

    def _affine_result_absolute(
        self, res_base: Any, result_unit: CompoundUnit
    ) -> Any:
        """Convert *res_base* back through *result_unit* and return a Quantity."""
        if not result_unit:
            raise ValueError("Result unit required for absolute result.")
        target_conv = None
        if len(result_unit.exponents) == 1:
            name, exp = next(iter(result_unit.exponents.items()))
            if exp == 1:
                target_conv = self.system.get_definition(name).converter
        if target_conv is None:
            raise NotImplementedError("Complex absolute units not supported.")
        res_mag = self._scalar_to_python(target_conv.from_base(res_base))
        return type(self).from_input(
            res_mag, result_unit, self.system, uncertainty=0.0
        )

    def _affine_result_delta(self, res_base: Any) -> Any:
        """Express *res_base* in the linear base unit for this dimension."""
        base_unit_name = None
        candidates = self.system.UNIT_REGISTRY.get(self.dimension, {})
        for name, u_def in candidates.items():
            is_base_unit = (
                isinstance(u_def.converter, LinearConverter)
                and abs(u_def.converter.scale - 1.0) < 1e-9
            )
            if is_base_unit:
                base_unit_name = name
                break
        if not base_unit_name:
            raise ValueError(
                f"No base linear unit found for dimension {self.dimension}"
            )
        target_unit = self.system.get_unit(base_unit_name)
        res_base = self._scalar_to_python(res_base)
        return type(self).from_input(
            res_base, target_unit, self.system, uncertainty=0.0
        )

    def _affine_add_sub(
        self,
        other: Quantity,
        is_add: bool,
        result_type: str,
        result_unit: CompoundUnit | None,
    ) -> Quantity:
        """Helper for Affine operations (Absolute/Delta)."""
        val_self_base = self._affine_to_base(self)
        val_other_base = self._affine_to_base(other)

        # Perform operation in base domain
        if is_add:
            res_base = self._backend.add(val_self_base, val_other_base)
        else:
            res_base = self._backend.sub(val_self_base, val_other_base)

        if result_type == "absolute":
            return self._affine_result_absolute(res_base, result_unit)
        return self._affine_result_delta(res_base)

    def _apply_transcendental(self, func_name: str) -> Quantity:
        """Applies a dimensionless transcendental function (sin, exp, etc.).

        Pure angles are converted to radians first (the SI dimensionless
        angle); dimensionless compounds (e.g. m/km) are collapsed to their
        scale factor; any other dimension raises DimensionError.
        """
        # 1. Verification: Argument must be dimensionless (or an angle)
        q = self
        if len(self.unit.exponents) > 0:
            dim = self.unit.dimension(self.system)
            if dim == Dimension({"A": 1}):
                q = self.to("rad")
            elif dim == Dimension({}):
                q = self.to(CompoundUnit({}))
            else:
                raise DimensionError(
                    f"Argument of {func_name}() must be dimensionless or "
                    f"an angle, got unit '{self.unit}'."
                )

        # 2. Get backend function
        op = getattr(self._backend, func_name)

        # 3. Propagate with explicit derivatives for accuracy
        if func_name == "sin":
            val = self._backend.sin(q.magnitude)
            der = self._backend.cos(q.magnitude)
        elif func_name == "cos":
            val = self._backend.cos(q.magnitude)
            der = self._backend.mul(-1.0, self._backend.sin(q.magnitude))
        elif func_name == "tan":
            val = self._backend.tan(q.magnitude)
            der = self._backend.add(1.0, self._backend.pow(val, 2))
        elif func_name == "exp":
            val = self._backend.exp(q.magnitude)
            der = val
        elif func_name == "log":
            val = self._backend.log(q.magnitude)
            der = self._backend.truediv(1.0, q.magnitude)
        elif func_name == "tanh":
            val = self._backend.tanh(q.magnitude)
            der = self._backend.sub(1.0, self._backend.pow(val, 2))
        else:
            # Fallback to finite difference if no explicit derivative
            h = 1e-7
            der = self._backend.truediv(
                self._backend.sub(
                    op(self._backend.add(q.magnitude, h)),
                    op(self._backend.sub(q.magnitude, h)),
                ),
                2 * h,
            )
            val = op(q.magnitude)

        unc = self._backend.mul(self._backend.abs(der), q.uncertainty)

        # 4. Return result (Dimensionless)
        return type(self).from_input(
            val, CompoundUnit({}), self.system, uncertainty=unc
        )

    def sin(self) -> Quantity:
        """Computes the sine."""
        return self._apply_transcendental("sin")

    def cos(self) -> Quantity:
        """Computes the cosine."""
        return self._apply_transcendental("cos")

    def tan(self) -> Quantity:
        """Computes the tangent."""
        return self._apply_transcendental("tan")

    def exp(self) -> Quantity:
        """Computes the exponential."""
        return self._apply_transcendental("exp")

    def log(self) -> Quantity:
        """Computes the natural logarithm."""
        return self._apply_transcendental("log")

    def tanh(self) -> Quantity:
        """Computes the hyperbolic tangent."""
        return self._apply_transcendental("tanh")

    def _broadcast_to_size(self, param: Any, size: int) -> Any:
        """Helper to broadcast a parameter to a flat size-vector."""
        if self._backend.is_array(param):
            shape = self._backend.shape(param)
            if shape == () or (
                hasattr(param, "shape") and param.shape == (1,)
            ):
                val = param.item() if hasattr(param, "item") else param
                return self._backend.mul(self._backend.ones(size), val)
            # Else assume it matches in shape or needs reshape to flat
            return self._backend.reshape(param, (size,))
        return self._backend.mul(self._backend.ones(size), param)

    def _affine_check(self, other: Quantity, is_add: bool) -> Quantity | None:
        """Checks and performs affine arithmetic if applicable."""
        kind_self = self.unit.kind(self.system)
        kind_other = other.unit.kind(self.system)

        is_absolute_self = kind_self == "absolute"
        is_absolute_other = kind_other == "absolute"

        if not (is_absolute_self or is_absolute_other):
            return None

        # Check for compatibility
        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)

        if is_add:
            if is_absolute_self and is_absolute_other:
                raise ValueError(
                    "Cannot add two absolute quantities. "
                    "Did you mean to add a difference?"
                )
            if is_absolute_self:
                # P + V -> P (self)
                return self._affine_add_sub(other, True, "absolute", self.unit)
            # V + P -> P (other)
            return other._affine_add_sub(self, True, "absolute", other.unit)

        # Subtraction
        if is_absolute_self and is_absolute_other:
            # P - P -> V
            return self._affine_add_sub(other, False, "delta", None)
        if is_absolute_self:
            # P - V -> P
            return self._affine_add_sub(other, False, "absolute", self.unit)

        # V - P -> Error
        raise ValueError(
            "Cannot subtract an absolute quantity from a difference."
        )

    def _logarithmic_add_sub(
        self, other: Quantity, is_add: bool
    ) -> Quantity | None:
        """Handles Logarithmic arithmetic (dB + dB)."""
        conv_self = self._get_converter_if_simple()
        conv_other = other._get_converter_if_simple()

        is_log_self = isinstance(conv_self, LogarithmicConverter)
        is_log_other = isinstance(conv_other, LogarithmicConverter)

        if is_log_self and is_log_other:
            base_self = conv_self.to_base(self.magnitude)
            base_other = conv_other.to_base(other.magnitude)

            if is_add:
                res_base = self._backend.add(base_self, base_other)
            else:
                res_base = self._backend.sub(base_self, base_other)

            res_mag = conv_self.from_base(res_base)

            if hasattr(res_mag, "ndim") and res_mag.ndim == 0:
                with contextlib.suppress(ValueError, TypeError):
                    res_mag = float(res_mag)

            return type(self).from_input(
                res_mag, self.unit, self.system, uncertainty=0.0
            )
        return None

    def _resolve_compatible_magnitude(self, other: Any, is_other_q: bool) -> Any:
        """Return the numeric value of other, converting units if needed for add/sub."""
        if not is_other_q:
            return other
        if self.unit != other.unit:
            if self.dimension != other.dimension:
                raise IncompatibleUnitsError(self.unit, other.unit)
            return other.to(self.unit).magnitude
        return other.magnitude

    def _mul_numeric_internal(self, other: Any, new_magnitude: Any, new_unit: Any) -> Any:
        """Jacobian + dispatch for numeric multiplication path."""
        if self._backend.is_array(new_magnitude):
            size = self._backend.size(new_magnitude)
            if self._backend.is_array(other):
                _, other_flat = self._backend.broadcast_and_flatten(
                    [self.magnitude, other]
                )
                j_self = self._backend.diagonal_operator(other_flat)
            else:
                j_self = self._backend.mul(
                    self._backend.identity_operator(size, reference=self.magnitude),
                    other,
                )
        else:
            j_self = other
        return self._mul_numeric_path(other, new_magnitude, new_unit, j_self, "mul")

    def _div_numeric_internal(self, other: Any, new_magnitude: Any, new_unit: Any) -> Any:
        """Jacobian + dispatch for numeric division path."""
        if self._backend.is_array(new_magnitude):
            size = self._backend.size(new_magnitude)
            if self._backend.is_array(other):
                _, other_flat = self._backend.broadcast_and_flatten(
                    [self.magnitude, other]
                )
                recip_flat = self._backend.truediv(1.0, other_flat)
                j_self = self._backend.diagonal_operator(recip_flat)
            else:
                j_self = self._backend.mul(
                    self._backend.identity_operator(size, reference=self.magnitude),
                    1.0 / other,
                )
        else:
            j_self = 1.0 / other
        return self._div_numeric_path(other, new_magnitude, new_unit, j_self, "truediv")

    # --- Arithmetic Dunder Methods ---
    def __add__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Handles arithmetic with Affine Support."""
        if isinstance(other, _q()):
            # 1. Affine and Logarithmic Logic (MUST BE ABOVE INITIAL CHECKS)
            res_affine = self._affine_check(other, is_add=True)
            if res_affine is not None:
                return res_affine

            res_log = self._logarithmic_add_sub(other, is_add=True)
            if res_log is not None:
                return res_log

        # 2. Optimized path for no uncertainty
        is_other_q = isinstance(other, _q())
        u_self = self.uncertainty
        u_other = other.uncertainty if is_other_q else 0.0
        no_unc = self._is_zero_uncertainty(u_self) and (
            not is_other_q or self._is_zero_uncertainty(u_other)
        )

        if no_unc:
            other_val = self._resolve_compatible_magnitude(other, is_other_q)
            new_val = self._backend.add(self.magnitude, other_val)
            res = type(self).from_input(new_val, self.unit, self.system)
            self._record_op("add", other, res)
            return res

        # 3. Vectorized / Scalar Path (Same Unit)
        if is_other_q and self.unit is other.unit:
            new_magnitude = self._backend.add(self.magnitude, other.magnitude)

            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)
                # Jacobians are identity matrices for addition
                j_self = self._backend.identity_operator(
                    size, reference=self.magnitude
                )
                j_other = self._backend.identity_operator(
                    size, reference=other.magnitude
                )
                return self._add_sub_array_path(
                    other, new_magnitude, j_self, j_other, "add"
                )

            # 4. Scalar Path (Same Unit)
            return self._add_sub_scalar_path(
                other, new_magnitude, u_other, 1.0, 1.0, "add"
            )

        # 5. Generic Path (Unit Conversion Required)
        if not is_other_q:
            return NotImplemented

        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)

        # Recurse with unit-converted other
        return self + other.to(self.unit)

    def __sub__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Handles subtraction."""
        if isinstance(other, _q()):
            # 1. Affine and Logarithmic Logic
            res_affine = self._affine_check(other, is_add=False)
            if res_affine is not None:
                return res_affine

            res_log = self._logarithmic_add_sub(other, is_add=False)
            if res_log is not None:
                return res_log

        # 2. Optimized path for no uncertainty
        is_other_q = isinstance(other, _q())
        u_self = self.uncertainty
        u_other = other.uncertainty if is_other_q else 0.0
        no_unc = self._is_zero_uncertainty(u_self) and (
            not is_other_q or self._is_zero_uncertainty(u_other)
        )

        if no_unc:
            other_val = self._resolve_compatible_magnitude(other, is_other_q)
            new_val = self._backend.sub(self.magnitude, other_val)
            res = type(self).from_input(new_val, self.unit, self.system)
            self._record_op("sub", other, res)
            return res

        # 3. Vectorized / Scalar Path (Same Unit)
        if is_other_q and self.unit is other.unit:
            new_magnitude = self._backend.sub(self.magnitude, other.magnitude)
            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)
                j_self = self._backend.identity_operator(
                    size, reference=self.magnitude
                )
                j_other = self._backend.mul(
                    self._backend.identity_operator(
                        size, reference=other.magnitude
                    ),
                    -1.0,
                )
                return self._add_sub_array_path(
                    other, new_magnitude, j_self, j_other, "sub"
                )

            # 4. Scalar Path (Same Unit)
            return self._add_sub_scalar_path(
                other, new_magnitude, u_other, 1.0, -1.0, "sub"
            )

        # 5. Generic Path (Unit Conversion Required)
        if not is_other_q:
            return NotImplemented

        if self.dimension != other.dimension:
            raise IncompatibleUnitsError(self.unit, other.unit)

        return self - other.to(self.unit)

    def __rsub__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Right subtraction."""
        return NotImplemented

    def __mul__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Multiplies two quantities."""
        if isinstance(other, CompoundUnit):
            new_unit = self.unit * other
            return type(self).from_input(
                self.magnitude,
                new_unit,
                self.system,
                uncertainty=self.uncertainty,
            )

        is_other_q = isinstance(other, _q())

        # 1. Optimized path for no uncertainty
        u_self = self.uncertainty
        u_other = other.uncertainty if is_other_q else 0.0
        no_unc = self._is_zero_uncertainty(u_self) and (
            not is_other_q or self._is_zero_uncertainty(u_other)
        )

        if no_unc:
            new_val = self._backend.mul(
                self.magnitude,
                other.magnitude if is_other_q else other,
            )
            new_unit = self.unit * (other.unit if is_other_q else other)
            res = type(self).from_input(new_val, new_unit, self.system)
            self._record_op("mul", other, res)
            return res

        # 2. Numeric scalar/array multiplication (Quantity * Numeric)
        is_numeric_other = isinstance(
            other, (int, float, complex)
        ) or self._backend.is_array(other)
        if is_numeric_other:
            new_magnitude = self._backend.mul(self.magnitude, other)
            new_unit = self.unit * other
            return self._mul_numeric_internal(other, new_magnitude, new_unit)

        # 3. Quantity * Quantity multiplication
        if is_other_q:
            new_magnitude = self._backend.mul(self.magnitude, other.magnitude)
            new_unit = self.unit * other.unit
            new_dimension = self.dimension * other.dimension

            if self._backend.is_array(new_magnitude):
                self_flat, other_flat = self._backend.broadcast_and_flatten(
                    [self.magnitude, other.magnitude]
                )
                # Jacobians: dz/dx = y, dz/dy = x
                j_self = self._backend.diagonal_operator(other_flat)
                j_other = self._backend.diagonal_operator(self_flat)
            else:
                j_self = other.magnitude
                j_other = self.magnitude

            return self._mul_qq_path(
                other,
                new_magnitude,
                new_unit,
                new_dimension,
                j_self,
                j_other,
                "mul",
            )

        return NotImplemented

    def __rmul__(self, other: Any) -> Quantity:
        """Handles reverse multiplication."""
        return self.__mul__(other)

    def __radd__(self, other: Any) -> Quantity:
        """Handles reverse addition."""
        return self.__add__(other)

    def __truediv__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Divides two quantities."""
        if isinstance(other, CompoundUnit):
            new_unit = self.unit / other
            return type(self).from_input(
                self.magnitude,
                new_unit,
                self.system,
                uncertainty=self.uncertainty,
            )

        is_other_q = isinstance(other, _q())

        # 2. Optimized path for no uncertainty
        u_self = self.uncertainty
        u_other = other.uncertainty if is_other_q else 0.0
        no_unc = self._is_zero_uncertainty(u_self) and (
            not is_other_q or self._is_zero_uncertainty(u_other)
        )

        if no_unc:
            new_val = self._backend.truediv(
                self.magnitude,
                other.magnitude if is_other_q else other,
            )
            new_unit = self.unit / (other.unit if is_other_q else other)
            res = type(self).from_input(new_val, new_unit, self.system)
            self._record_op("truediv", other, res)
            return res

        # 3. Numeric Path (Quantity / Numeric)
        is_numeric_other = isinstance(
            other, (int, float, complex)
        ) or self._backend.is_array(other)
        if is_numeric_other:
            new_magnitude = self._backend.truediv(self.magnitude, other)
            new_unit = self.unit / other
            return self._div_numeric_internal(other, new_magnitude, new_unit)

        # 4. Quantity / Quantity Path
        if is_other_q:
            new_magnitude = self._backend.truediv(
                self.magnitude, other.magnitude
            )
            new_unit = self.unit / other.unit
            new_dimension = self.dimension / other.dimension

            if self._backend.is_array(new_magnitude):
                _, other_flat = self._backend.broadcast_and_flatten(
                    [self.magnitude, other.magnitude]
                )
                recip_flat = self._backend.truediv(1.0, other_flat)
                j_self = self._backend.diagonal_operator(recip_flat)
                neg_z_over_y = self._backend.mul(
                    self._backend.truediv(new_magnitude.ravel(), other_flat),
                    -1.0,
                )
                j_other = self._backend.diagonal_operator(neg_z_over_y)
            else:
                j_self = 1.0 / other.magnitude
                j_other = -self.magnitude / (other.magnitude**2)

            return self._div_qq_path(
                other,
                new_magnitude,
                new_unit,
                new_dimension,
                j_self,
                j_other,
                "truediv",
            )

        return NotImplemented

    def __pow__(self, exponent: float) -> Quantity[Any, Any, Any]:
        """Raises quantity to power."""
        u_self = self.uncertainty
        if u_self is None or (
            isinstance(u_self, (int, float)) and u_self == 0.0  # NOSONAR
        ):
            new_val = self._backend.pow(self.magnitude, exponent)
            new_unit = self.unit**exponent
            return type(self).from_input(new_val, new_unit, self.system)

        new_value = self._backend.pow(self.magnitude, exponent)
        new_unit = self.unit**exponent
        new_uncertainty = self._backend.mul(
            self._backend.abs(
                self._backend.mul(
                    exponent,
                    self._backend.pow(
                        self.magnitude, self._backend.sub(exponent, 1)
                    ),
                )
            ),
            self.uncertainty,
        )
        res = type(self).from_input(
            new_value, new_unit, self.system, uncertainty=new_uncertainty
        )
        from measurekit.application.tracing.context import get_active_tracer

        if (tracer := get_active_tracer()) is not None:
            tracer.record_operation(
                "pow", operands=(self, exponent), result=res
            )
        return res



    def __rpow__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Right power."""
        return NotImplemented

    def __rtruediv__(self, other: Any) -> Quantity[Any, Any, Any]:
        """Right division."""
        new_magnitude = self._backend.truediv(other, self.magnitude)
        new_unit = 1 / self.unit

        # GUM Rule for y = k/x: u_y = |-k/x^2| * u_x
        # jac_self = -other/x^2 = -y/x
        jac_self = self._backend.mul(
            self._backend.truediv(new_magnitude, self.magnitude), -1.0
        )

        new_u_obj = None
        if self._uncertainty_obj is not None:
            new_u_obj = self._uncertainty_obj.propagate_mul_div(
                None,
                self.magnitude,
                other,
                new_magnitude,
                jac_self=jac_self,
                jac_other=0.0,
            )
            new_uncertainty = new_u_obj.std_dev
        else:
            new_uncertainty = self._backend.mul(
                self._backend.abs(jac_self), self.uncertainty
            )

        res = type(self).from_input(
            new_magnitude, new_unit, self.system, uncertainty=new_uncertainty
        )
        if new_u_obj is not None:
            res._uncertainty_obj = new_u_obj
        return res

    def __neg__(self) -> Self:
        """Returns the negation of the quantity."""
        return cast(
            "Self",
            type(self).from_input(
                self._backend.mul(self.magnitude, -1),
                self.unit,
                self.system,
                uncertainty=self.uncertainty,
            ),
        )

    def __pos__(self) -> Self:
        """Returns the quantity itself."""
        return self

    def __abs__(self) -> Self:
        """Returns the absolute value of the quantity."""
        return cast(
            "Self",
            type(self).from_input(
                self._backend.abs(self.magnitude),
                self.unit,
                self.system,
                uncertainty=self.uncertainty,
            ),
        )
