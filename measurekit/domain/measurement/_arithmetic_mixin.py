"""Arithmetic and uncertainty propagation mixin for Quantity."""
from __future__ import annotations

import contextlib
from typing import Any, cast

from typing_extensions import Self

from measurekit.domain.exceptions import IncompatibleUnitsError
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
        from measurekit.domain.measurement.quantity import Quantity as _Q
        _Quantity = _Q
    return _Quantity


class ArithmeticMixin:
    """Arithmetic operators, uncertainty propagation, and transcendental functions."""

    def _propagate_vectorized(
        self,
        other: Any,
        out_magnitude: Any,
        jac_self: Any,
        jac_other: Any = None,
    ) -> Any:
        """Helper to propagate vectorized uncertainty."""
        # Check for rich model first
        is_other_q = isinstance(other, _q())
        u_obj_other = other._uncertainty_obj if is_other_q else None

        if self._uncertainty_obj is not None or u_obj_other is not None:
            # For Phase 1, we assume they are compatible or the lineage handles it.

            # Case 1: Self has rich model
            if self._uncertainty_obj is not None:
                # Upgrade other if needed
                u_other = u_obj_other
                j_other = jac_other

                if u_other is None:
                    if is_other_q:
                        u_other = Uncertainty.from_standard(other.uncertainty)
                    else:
                        u_other = Uncertainty.from_standard(0.0)
                        # If we synthesize uncertainty for scalar, jac_other might be None
                        if j_other is None:
                            j_other = 0.0

                new_u_obj = self._uncertainty_obj.add(
                    u_other,
                    jac_self=jac_self,
                    jac_other=j_other,
                    out_magnitude=out_magnitude,
                )
                return new_u_obj.std_dev

            # Case 2: Other has rich model (reverse add)
            elif u_obj_other is not None:
                # Upgrade self
                u_self = Uncertainty.from_standard(self.uncertainty)

                j_self_for_other = jac_other
                if j_self_for_other is None:
                    j_self_for_other = 0.0

                # Swap Jacobians because we call other.add(self)
                new_u_obj = u_obj_other.add(
                    u_self,
                    jac_self=j_self_for_other,
                    jac_other=jac_self,
                    out_magnitude=out_magnitude,
                )
                return new_u_obj.std_dev

        # Standard Gaussian propagation (uncorrelated fallback)
        u_self = self.uncertainty
        u_other = other.uncertainty if is_other_q else 0.0

        if self._backend.is_array(u_self):
            # Flatten to vector
            u_self = self._backend.reshape(u_self, (-1,))
        if self._backend.is_array(u_other):
            u_other = self._backend.reshape(u_other, (-1,))

        var_self = self._backend.pow(u_self, 2)
        # Force numeric array once and for all to avoid dtype('O') issues
        var_self = self._backend.asarray(var_self)
        if hasattr(var_self, "astype"):
            var_self = var_self.astype("float64", copy=False)

        jac_self_sq = self._backend.pow(jac_self, 2)
        if hasattr(jac_self_sq, "astype"):
            jac_self_sq = jac_self_sq.astype("float64", copy=False)

        res_var = self._backend.dot(jac_self_sq, var_self)

        # Fix for potential object array result from sparse dot product
        # Scipy sparse dot can return object arrays of sparse matrices/arrays
        # when broadcasting with certain shapes. We must densify them.
        def _densify(x):
            if hasattr(x, "toarray"):  # Sparse matrix
                return x.toarray().item() if x.size == 1 else x.toarray().sum()
            if hasattr(x, "item") and x.size == 1:  # Scalar-like array
                return x.item()
            if (
                hasattr(x, "sum") and hasattr(x, "shape") and x.shape != ()
            ):  # Array
                return x.sum()
            return x

        if hasattr(res_var, "dtype") and res_var.dtype == object:
            try:
                res_var = res_var.astype("float64")
            except (ValueError, TypeError):
                # Fallback: Densify elements
                try:
                    import numpy as np

                    # Use ravel() to iterate over objects, not into them
                    # Force conversion to python float to avoid nested array creation
                    flat_data = [float(_densify(x)) for x in res_var.ravel()]

                    res_var = (
                        np.array(flat_data)
                        .reshape(res_var.shape)
                        .astype("float64")
                    )
                except Exception:
                    pass

        if jac_other is not None:
            # Ensure var_other is strictly numeric (float64) to prevent object array pollution
            if hasattr(u_other, "astype"):
                var_other = self._backend.pow(u_other, 2).astype(
                    "float64", copy=False
                )
            else:
                var_other = self._backend.pow(u_other, 2)

            jac_other_sq = self._backend.pow(jac_other, 2)
            # Ensure macobian is numeric if it came from identity_operator
            if hasattr(jac_other_sq, "astype"):
                jac_other_sq = jac_other_sq.astype("float64", copy=False)

            term2 = self._backend.dot(jac_other_sq, var_other)
            if hasattr(term2, "dtype") and term2.dtype == object:
                try:
                    term2 = term2.astype("float64")
                except (ValueError, TypeError):
                    # Reuse densify logic
                    try:
                        import numpy as np

                        # Explicit iteration
                        r2 = term2.reshape(-1)
                        flat_data = []
                        for i in range(r2.size):
                            val = r2[i]
                            flat_data.append(float(_densify(val)))

                        term2 = (
                            np.array(flat_data)
                            .reshape(term2.shape)
                            .astype("float64")
                        )
                    except Exception:
                        pass

            # Fix broadcasting issue where dot produced matrix instead of vector
            # due to scalar variance being treated as scaling factor
            if hasattr(res_var, "shape") and hasattr(term2, "shape"):
                if (
                    term2.ndim == 2
                    and res_var.ndim == 1
                    and term2.shape[0] == term2.shape[1] == res_var.shape[0]
                ):
                    try:
                        import numpy as np

                        if hasattr(term2, "diagonal"):
                            term2 = term2.diagonal()
                        else:
                            term2 = np.diag(term2)
                    except Exception:
                        pass

            res_var = self._backend.add(res_var, term2)

        if hasattr(self._backend, "sqrt"):
            u_out = self._backend.sqrt(res_var)
        else:
            u_out = self._backend.pow(res_var, 0.5)

        # Reshape to match magnitude
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

    def _affine_add_sub(
        self,
        other: Quantity,
        is_add: bool,
        result_type: str,
        result_unit: CompoundUnit | None,
    ) -> Quantity:
        """Helper for Affine operations (Absolute/Delta)."""
        conv_self = self._get_converter_if_simple()
        conv_other = other._get_converter_if_simple()

        # Convert to Base Values
        # Note: to_base takes backend types.
        if conv_self:
            val_self_base = conv_self.to_base(self.magnitude)
        else:
            # Assume Delta if compound/complex (Vector)
            # Delta to Base is scaling.
            # We use conversion_factor_to to get scale?
            # But conversion_factor_to needs a target.
            # We assume target is Base Unit.
            # Construct a temporary unit representing the base?
            # Or use self.unit._compound_factor(system).
            factor = self.unit._compound_factor(self.system)
            val_self_base = self._backend.mul(self.magnitude, factor)

        if conv_other:
            val_other_base = conv_other.to_base(other.magnitude)
        else:
            factor = other.unit._compound_factor(self.system)
            val_other_base = self._backend.mul(other.magnitude, factor)

        # Perform Operation in Base Domain
        if is_add:
            res_base = self._backend.add(val_self_base, val_other_base)
        else:
            res_base = self._backend.sub(val_self_base, val_other_base)

        # Convert to Result Unit
        if result_type == "absolute":
            # Return in result_unit (Must be provided and Simple/Absolute)
            if not result_unit:
                raise ValueError("Result unit required for absolute result.")

            # Retrieve converter for the result unit
            # (assumed simple for Absolute)
            target_conv = None
            if len(result_unit.exponents) == 1:
                name, exp = next(iter(result_unit.exponents.items()))
                if exp == 1:
                    target_conv = self.system.get_definition(name).converter

            if target_conv is None:
                # Should not happen for Absolute units (usually simple)
                raise NotImplementedError(
                    "Complex absolute units not supported."
                )

            res_mag = target_conv.from_base(res_base)

            # Helper cast for numpy scalars to avoid BackendManager confusion
            if hasattr(res_mag, "ndim") and res_mag.ndim == 0:
                with contextlib.suppress(ValueError, TypeError):
                    res_mag = float(res_mag)
            elif hasattr(res_mag, "item"):
                with contextlib.suppress(ValueError, TypeError):
                    res_mag = res_mag.item()

            # Simplified uncertainty (assuming 1.0 correlation/scale prop)
            return type(self).from_input(
                res_mag, result_unit, self.system, uncertainty=0.0
            )

        # result_type == "delta"
        # Return in Base Unit (Linear)
        # We need to find the base unit for this dimension.
        # Helper: Find linear unit with scale=1.0 for this dimension.
        base_unit_name = None
        candidates = self.system.UNIT_REGISTRY.get(self.dimension, {})
        for name, u_def in candidates.items():
            if (
                isinstance(u_def.converter, LinearConverter)
                and abs(u_def.converter.scale - 1.0) < 1e-9
            ):
                base_unit_name = name
                break

        if not base_unit_name:
            # Fallback: Just return numbers? No, return Quantity.
            # Use self.unit if linear?
            # If we are here, we likely have kelvin/meter/etc.
            raise ValueError(
                f"No base linear unit found for dimension {self.dimension}"
            )

        target_unit = self.system.get_unit(base_unit_name)

        if hasattr(res_base, "ndim") and res_base.ndim == 0:
            with contextlib.suppress(ValueError, TypeError):
                res_base = float(res_base)
        elif hasattr(res_base, "item"):
            with contextlib.suppress(ValueError, TypeError):
                res_base = res_base.item()

        return type(self).from_input(
            res_base, target_unit, self.system, uncertainty=0.0
        )

    def _apply_transcendental(self, func_name: str) -> Quantity:
        """Applies a dimensionless transcendental function (sin, exp, etc.)."""
        # 1. Verification: Argument must be dimensionless
        if len(self.unit.exponents) > 0:
            # For now, we allow it but in strict mode we should raise.
            pass

        # 2. Get backend function
        op = getattr(self._backend, func_name)

        # 3. Propagate with explicit derivatives for accuracy
        if func_name == "sin":
            val = self._backend.sin(self.magnitude)
            der = self._backend.cos(self.magnitude)
        elif func_name == "cos":
            val = self._backend.cos(self.magnitude)
            der = self._backend.mul(-1.0, self._backend.sin(self.magnitude))
        elif func_name == "tan":
            val = self._backend.tan(self.magnitude)
            der = self._backend.add(1.0, self._backend.pow(val, 2))
        elif func_name == "exp":
            val = self._backend.exp(self.magnitude)
            der = val
        elif func_name == "log":
            val = self._backend.log(self.magnitude)
            der = self._backend.truediv(1.0, self.magnitude)
        elif func_name == "tanh":
            val = self._backend.tanh(self.magnitude)
            der = self._backend.sub(1.0, self._backend.pow(val, 2))
        else:
            # Fallback to finite difference if no explicit derivative
            h = 1e-7
            der = self._backend.truediv(
                self._backend.sub(
                    op(self._backend.add(self.magnitude, h)),
                    op(self._backend.sub(self.magnitude, h)),
                ),
                2 * h,
            )
            val = op(self.magnitude)

        unc = self._backend.mul(self._backend.abs(der), self.uncertainty)

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

        if (
            u_self is None
            or (isinstance(u_self, (int, float)) and u_self == 0.0)  # NOSONAR
        ) and (
            not is_other_q
            or (
                u_other is None
                or (isinstance(u_other, (int, float)) and u_other == 0.0)  # NOSONAR
            )
        ):
            if is_other_q:
                if self.unit != other.unit:
                    if self.dimension != other.dimension:
                        raise IncompatibleUnitsError(self.unit, other.unit)
                    other_val = other.to(self.unit).magnitude
                else:
                    other_val = other.magnitude
            else:
                other_val = other

            new_val = self._backend.add(self.magnitude, other_val)
            res = type(self).from_input(new_val, self.unit, self.system)

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "add", operands=(self, other), result=res
                )
            return res

        # 3. Vectorized Path (Same Unit)
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
                # Re-attach rich model if created by _propagate_vectorized
                if self._uncertainty_obj is not None:
                    u_obj_other = other._uncertainty_obj
                    res._uncertainty_obj = self._uncertainty_obj.add(
                        u_obj_other,
                        jac_self=j_self,
                        jac_other=j_other,
                        out_magnitude=new_magnitude,
                    )

                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "add", operands=(self, other), result=res
                    )
                return res

            # 4. Scalar Path (Same Unit)
            u_obj_other = other._uncertainty_obj
            new_u_obj = None
            if self._uncertainty_obj is not None:
                new_u_obj = self._uncertainty_obj.add(
                    u_obj_other,
                    jac_self=1.0,
                    jac_other=1.0,
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
            return res

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

        if (
            u_self is None
            or (isinstance(u_self, (int, float)) and u_self == 0.0)  # NOSONAR
        ) and (
            not is_other_q
            or (
                u_other is None
                or (isinstance(u_other, (int, float)) and u_other == 0.0)  # NOSONAR
            )
        ):
            if is_other_q:
                if self.unit != other.unit:
                    if self.dimension != other.dimension:
                        raise IncompatibleUnitsError(self.unit, other.unit)
                    other_val = other.to(self.unit).magnitude
                else:
                    other_val = other.magnitude
            else:
                other_val = other

            new_val = self._backend.sub(self.magnitude, other_val)
            res = type(self).from_input(new_val, self.unit, self.system)

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "sub", operands=(self, other), result=res
                )
            return res

        # 3. Vectorized Path (Same Unit)
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
                # Re-attach rich model
                if self._uncertainty_obj is not None:
                    res._uncertainty_obj = self._uncertainty_obj.add(
                        other._uncertainty_obj,
                        jac_self=j_self,
                        jac_other=j_other,
                        out_magnitude=new_magnitude,
                    )

                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "sub", operands=(self, other), result=res
                    )
                return res

            # 4. Scalar Path (Same Unit)
            u_obj_other = other._uncertainty_obj
            new_u_obj = None
            if self._uncertainty_obj is not None:
                new_u_obj = self._uncertainty_obj.add(
                    u_obj_other,
                    jac_self=1.0,
                    jac_other=-1.0,
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

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "sub", operands=(self, other), result=res
                )
            return res

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

        if (
            u_self is None
            or (isinstance(u_self, (int, float)) and u_self == 0.0)  # NOSONAR
        ) and (
            not is_other_q
            or (
                u_other is None
                or (isinstance(u_other, (int, float)) and u_other == 0.0)  # NOSONAR
            )
        ):
            new_val = self._backend.mul(
                self.magnitude,
                other.magnitude if is_other_q else other,
            )
            new_unit = self.unit * (other.unit if is_other_q else other)
            res = type(self).from_input(new_val, new_unit, self.system)

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "mul", operands=(self, other), result=res
                )
            return res

        # 2. Numeric scalar/array multiplication (Quantity * Numeric)
        if isinstance(other, (int, float, complex)) or self._backend.is_array(
            other
        ):
            new_magnitude = self._backend.mul(self.magnitude, other)
            new_unit = self.unit * other  # CompoundUnit handles this

            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)
                if self._backend.is_array(other):
                    # Use diagonal_operator for element-wise scaling
                    # broadcast_and_flatten ensures shapes match
                    _, other_flat = self._backend.broadcast_and_flatten(
                        [self.magnitude, other]
                    )
                    j_self = self._backend.diagonal_operator(other_flat)
                else:
                    j_self = self._backend.mul(
                        self._backend.identity_operator(
                            size, reference=self.magnitude
                        ),
                        other,
                    )

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
                    res._uncertainty_obj = (
                        self._uncertainty_obj.propagate_mul_div(
                            None,
                            self.magnitude,
                            other,
                            new_magnitude,
                            jac_self=j_self,
                            jac_other=0.0,
                        )
                    )

                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "mul", operands=(self, other), result=res
                    )
                return res

            # Scalar Path for Quantity * Numeric
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

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "mul", operands=(self, other), result=res
                )
            return res

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
                    res._uncertainty_obj = (
                        self._uncertainty_obj.propagate_mul_div(
                            other._uncertainty_obj,
                            self.magnitude,
                            other.magnitude,
                            new_magnitude,
                            jac_self=j_self,
                            jac_other=j_other,
                        )
                    )

                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "mul", operands=(self, other), result=res
                    )
                return res

            # Scalar Path for Quantity * Quantity
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
                # GUM Fallback: u_z = sqrt((u_x*y)^2 + (x*u_y)^2)
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

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "mul", operands=(self, other), result=res
                )
            return res

        if isinstance(other, CompoundUnit):
            return type(self).from_input(
                self.magnitude,
                self.unit * other,
                self.system,
                uncertainty=self.uncertainty,
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

        # 1. Unit/Dimension Path
        if isinstance(other, CompoundUnit):
            return type(self).from_input(
                self.magnitude,
                self.unit / other,
                self.system,
                uncertainty=self.uncertainty,
            )

        # 2. Optimized path for no uncertainty
        u_self = self.uncertainty
        u_other = other.uncertainty if is_other_q else 0.0

        if (
            u_self is None
            or (isinstance(u_self, (int, float)) and u_self == 0.0)  # NOSONAR
        ) and (
            not is_other_q
            or (
                u_other is None
                or (isinstance(u_other, (int, float)) and u_other == 0.0)  # NOSONAR
            )
        ):
            new_val = self._backend.truediv(
                self.magnitude,
                other.magnitude if is_other_q else other,
            )
            new_unit = self.unit / (other.unit if is_other_q else other)
            res = type(self).from_input(new_val, new_unit, self.system)

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "truediv", operands=(self, other), result=res
                )
            return res

        # 3. Numeric Path (Quantity / Numeric)
        if isinstance(other, (int, float, complex)) or self._backend.is_array(
            other
        ):
            new_magnitude = self._backend.truediv(self.magnitude, other)
            new_unit = self.unit / other

            if self._backend.is_array(new_magnitude):
                size = self._backend.size(new_magnitude)
                # dz/dx = 1/y
                if self._backend.is_array(other):
                    _, other_flat = self._backend.broadcast_and_flatten(
                        [self.magnitude, other]
                    )
                    recip_flat = self._backend.truediv(1.0, other_flat)
                    j_self = self._backend.diagonal_operator(recip_flat)
                else:
                    j_self = self._backend.mul(
                        self._backend.identity_operator(
                            size, reference=self.magnitude
                        ),
                        1.0 / other,
                    )

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
                    res._uncertainty_obj = (
                        self._uncertainty_obj.propagate_mul_div(
                            None,
                            self.magnitude,
                            other,
                            new_magnitude,
                            jac_self=j_self,
                            jac_other=0.0,
                        )
                    )

                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "truediv", operands=(self, other), result=res
                    )
                return res

            # Scalar Path for Quantity / Numeric
            new_u_obj = None
            j_self = 1.0 / other
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

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "truediv", operands=(self, other), result=res
                )
            return res

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

                # dz/dx = 1/y
                recip_flat = self._backend.truediv(1.0, other_flat)
                j_self = self._backend.diagonal_operator(recip_flat)

                # dz/dy = -x/y^2 = -z/y
                neg_z_over_y = self._backend.mul(
                    self._backend.truediv(new_magnitude.ravel(), other_flat),
                    -1.0,
                )
                j_other = self._backend.diagonal_operator(neg_z_over_y)

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
                    res._uncertainty_obj = (
                        self._uncertainty_obj.propagate_mul_div(
                            other._uncertainty_obj,
                            self.magnitude,
                            other.magnitude,
                            new_magnitude,
                            jac_self=j_self,
                            jac_other=j_other,
                        )
                    )

                from measurekit.application.tracing.context import (
                    get_active_tracer,
                )

                if (tracer := get_active_tracer()) is not None:
                    tracer.record_operation(
                        "truediv", operands=(self, other), result=res
                    )
                return res

            # Scalar Path for Quantity / Quantity
            new_u_obj = None
            j_self = 1.0 / other.magnitude
            j_other = -self.magnitude / (other.magnitude**2)
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
                # GUM Fallback: u_z = sqrt((u_x/y)^2 + (x*u_y/y^2)^2)
                var_self = self._backend.pow(
                    self.uncertainty / other.magnitude, 2
                )
                var_other = self._backend.pow(
                    self.magnitude * other.uncertainty / (other.magnitude**2),
                    2,
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

            from measurekit.application.tracing.context import (
                get_active_tracer,
            )

            if (tracer := get_active_tracer()) is not None:
                tracer.record_operation(
                    "truediv", operands=(self, other), result=res
                )
            return res

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

    __radd__ = __add__
    __rmul__ = __mul__

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

