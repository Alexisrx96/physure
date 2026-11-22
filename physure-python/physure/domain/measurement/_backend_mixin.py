"""NumPy and PyTorch integration mixin for Quantity."""

from __future__ import annotations

import operator
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from typing import Self

    from physure.core.protocols import Numeric

from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.uncertainty import Uncertainty
from physure.domain.measurement.units import CompoundUnit

_Quantity = None


def _q():
    global _Quantity
    if _Quantity is None:
        from physure.domain.measurement.quantity import Quantity

        _Quantity = Quantity
    return _Quantity


_TOLERANCE_ARGS = ("a", "b", "rtol", "atol", "equal_nan")


def _coherent_operand(np: Any, operand: Any) -> tuple[Any, Any, Any]:
    """`(dimension, coherent unit, float array)` of a quantity, of an array or list of them.

    Plain numbers (no quantity among them) are dimensionless: dimension `None`.
    """
    if isinstance(operand, _q()):
        unit, value = operand._coherent_value()
        return operand.dimension, unit, np.asarray(value, dtype=float)
    items = np.asarray(operand, dtype=object)
    flat = items.reshape(-1)
    quantities = [item for item in flat if isinstance(item, _q())]
    if not quantities:
        return None, None, np.asarray(operand, dtype=float)
    first = quantities[0]
    first_unit = first._coherent_value()[0]
    values = np.empty(flat.shape, dtype=float)
    for position, item in enumerate(flat):
        if not isinstance(item, _q()):
            raise IncompatibleUnitsError(first.unit, CompoundUnit({}))
        unit, value = item._coherent_value()
        if item.dimension != first.dimension or unit != first_unit:
            raise IncompatibleUnitsError(first.unit, item.unit)
        values[position] = value
    return first.dimension, first_unit, values.reshape(items.shape)


def _isclose(
    np: Any, all_of_them: bool, args: tuple[Any, ...], kwargs: dict[str, Any]
) -> Any:
    """`np.isclose` and `np.allclose` of quantities, compared in their coherent unit.

    Each side (a quantity, an array-backed one, an array or a list of them) is read as
    its value in the coherent unit, so `1 m` is close to `100 cm`; another dimension
    raises. `rtol` is a plain number. `atol` is a quantity of the same dimension, or
    `0`: NumPy's default `1e-8` has no unit, so for a dimensional comparison the
    default `atol` is `0`, and a bare non-zero `atol` raises `TypeError`.
    """
    bound = dict(zip(_TOLERANCE_ARGS, args, strict=False))
    bound.update(kwargs)
    a, b = bound["a"], bound["b"]
    if (
        isinstance(a, _q())
        and isinstance(b, _q())
        and a._point_against_interval(b)
    ):
        a._refuse_point_against_interval(b)
    left_dimension, left_unit, left = _coherent_operand(np, a)
    right_dimension, right_unit, right = _coherent_operand(np, b)
    if left_dimension is not None:
        dimension, unit = left_dimension, left_unit
    else:
        dimension, unit = right_dimension, right_unit
    dimensional = dimension is not None and not dimension.is_dimensionless
    if left_dimension is not None and right_dimension is not None:
        if left_dimension != right_dimension or left_unit != right_unit:
            raise IncompatibleUnitsError(left_unit, right_unit)
    elif dimensional:
        # a plain number against a length has no unit to be compared in
        raise IncompatibleUnitsError(unit, CompoundUnit({}))
    atol = bound.get("atol", 0.0 if dimensional else 1e-8)
    if isinstance(atol, _q()):
        if atol.dimension != dimension:
            raise IncompatibleUnitsError(atol.unit, unit)
        atol = float(np.asarray(atol._coherent_value()[1], dtype=float))
    elif dimensional and atol != 0:
        message = (
            f"atol={atol!r} has no unit and the quantities compared are in '{unit}': "
            f"give atol as a quantity (atol=Q_(1e-9, '{unit}')) or 0"
        )
        raise TypeError(message)
    close = np.isclose(
        left,
        right,
        rtol=bound.get("rtol", 1e-05),
        atol=atol,
        equal_nan=bound.get("equal_nan", False),
    )
    return bool(np.all(close)) if all_of_them else close


class BackendMixin:
    """NumPy ufunc/function and PyTorch function dispatch methods."""

    # ponytail: what the host class (Quantity) provides, declared for the type
    # checkers that read the mixin on its own, as ArithmeticMixin does.
    if TYPE_CHECKING:
        unit: CompoundUnit
        system: Any
        magnitude: Any
        _backend: Any
        _uncertainty_obj: Any

        @property
        def _numeric_std_dev(self) -> Any: ...

        @property
        def _has_uncertainty(self) -> bool: ...

        def _is_level(self) -> bool: ...

        def _is_point(self) -> bool: ...

        @classmethod
        def from_input(cls, *args: Any, **kwargs: Any) -> Any: ...

    # --- NumPy Integration (Soft Dependency) ---

    _NUMPY_TRIG_NAMES = frozenset(
        (
            "sin",
            "cos",
            "tan",
            "exp",
            "log",
            "log10",
            "arcsin",
            "arccos",
            "arctan",
            "tanh",
            "sinh",
            "cosh",
        )
    )

    def _numpy_ufunc_reduce(
        self, ufunc: Any, inputs: tuple[Any, ...], kwargs: dict[str, Any]
    ) -> Any:
        """`np.add.reduce`, the sum `np.sum` is; NotImplemented for the other reductions.

        It used to sum the numbers alone and drop the uncertainty.
        """
        import numpy as np

        if ufunc != np.add:
            return NotImplemented
        inp = inputs[0]
        if not isinstance(inp, _q()):
            return NotImplemented
        return inp._numpy_sum_or_mean(np, False, {"axis": 0, **kwargs})

    def _numpy_ufunc_check_compatible(self, val: Any) -> None:
        """Raise IncompatibleUnitsError on dimension mismatch."""
        if (
            isinstance(val, _q())
            and self.unit != val.unit
            and self.dimension != val.dimension
        ):
            raise IncompatibleUnitsError(self.unit, val.unit)

    def _numpy_ufunc_arithmetic(
        self, ufunc: Any, inputs: tuple[Any, ...]
    ) -> Any:
        """Dispatch numpy arithmetic ufuncs (add/sub/mul/div/pow)."""
        import numpy as np

        if ufunc in (np.add, np.subtract, np.multiply):
            other = inputs[1] if inputs[0] is self else inputs[0]
            if ufunc == np.add:
                self._numpy_ufunc_check_compatible(other)
                return self.__add__(other)
            if ufunc == np.multiply:
                return self.__mul__(other)
            self._numpy_ufunc_check_compatible(other)
            if inputs[0] is self:
                return self.__sub__(other)
            return self.__rsub__(other)
        if ufunc == np.true_divide:
            if inputs[0] is self:
                return self.__truediv__(inputs[1])
            return self.__rtruediv__(inputs[0])
        if ufunc == np.power and inputs[0] is self:
            return self.__pow__(inputs[1])
        return NotImplemented

    def _numpy_ufunc_trig_with_uncertainty(
        self,
        ufunc: Any,
        inp: Any,
        res_mag: Numeric,
        u_inp: Numeric,
        **kwargs: Any,
    ) -> Any:
        """Propagate uncertainty through a trig ufunc; returns a Quantity."""
        # ponytail: kwargs is part of the call signature forwarded from
        # __array_ufunc__ (numpy ufunc kwargs like `where=`) but the
        # finite-difference derivative below never needs them.
        del kwargs
        h = 1e-7
        m_plus = inp._backend.add(inp.magnitude, h)
        m_minus = inp._backend.sub(inp.magnitude, h)
        try:
            der = inp._backend.truediv(
                inp._backend.sub(ufunc(m_plus), ufunc(m_minus)),
                2 * h,
            )
            res_unc = inp._backend.mul(inp._backend.abs(der), u_inp)
            return type(self).from_input(
                res_mag, CompoundUnit({}), self.system, uncertainty=res_unc
            )
        except Exception:
            # Conservative fallback for complex backends
            return type(self).from_input(
                res_mag, CompoundUnit({}), self.system, uncertainty=u_inp
            )

    def _numpy_ufunc_trig(
        self, ufunc: Any, inputs: tuple[Any, ...], kwargs: dict[str, Any]
    ) -> Any:
        """Handle dimensionless trig ufuncs; return NotImplemented if ufunc not recognised."""
        if ufunc.__name__ not in self._NUMPY_TRIG_NAMES:
            return NotImplemented
        inp = inputs[0]
        if not isinstance(inp, _q()):
            return NotImplemented
        if not inp.dimension.is_dimensionless:
            raise IncompatibleUnitsError(inp.unit, CompoundUnit({}))
        res_mag = ufunc(inp.magnitude, **kwargs)
        if inp._has_uncertainty:
            return self._numpy_ufunc_trig_with_uncertainty(
                ufunc, inp, res_mag, inp._numeric_std_dev, **kwargs
            )
        return type(self).from_input(res_mag, CompoundUnit({}), self.system)

    def __array_ufunc__(
        self,
        ufunc: Any,
        method: str,
        *inputs: Any,
        **kwargs: Any,
    ) -> Any:
        """Handles NumPy ufuncs by delegating to the backend."""
        try:
            import numpy as np
        except ImportError:
            return NotImplemented

        if method == "reduce":
            return self._numpy_ufunc_reduce(ufunc, inputs, kwargs)

        if method != "__call__":
            return NotImplemented

        # ponytail: BackendMixin is only ever mixed into Quantity, which
        # defines __pow__/__abs__; pyright analyzes the mixin in isolation
        # and can't see those operators on Self@BackendMixin.
        # Unary math that changes unit
        if ufunc == np.sqrt:
            return self**0.5  # pyright: ignore[reportOperatorIssue]
        if ufunc == np.square:
            return self**2  # pyright: ignore[reportOperatorIssue]
        # Unary math that preserves unit
        if ufunc == np.absolute:
            return abs(self)  # pyright: ignore[reportArgumentType]

        arith = self._numpy_ufunc_arithmetic(ufunc, inputs)
        if arith is not NotImplemented:
            return arith

        return self._numpy_ufunc_trig(ufunc, inputs, kwargs)

    def _numpy_concatenate(
        self, np: Any, args: tuple[Any, ...], kwargs: dict[str, Any]
    ) -> Any:
        """Collect magnitudes from a same-unit sequence and concatenate them."""
        mags = []
        unit = None
        for arg in args[0]:
            if not isinstance(arg, _q()):
                return NotImplemented  # All must be Quantity for now
            if unit is None:
                unit = arg.unit
            elif arg.unit != unit:
                return NotImplemented  # Strict unit check
            mags.append(arg.magnitude)
        res_mag = np.concatenate(mags, **kwargs)
        # ponytail: BackendMixin is only ever mixed into Quantity, whose
        # real __init__ takes (magnitude, unit, system=...); pyright/ty see
        # only object.__init__ from the mixin's own MRO.
        return type(self)(
            res_mag,  # pyright: ignore[reportCallIssue]  # ty: ignore[too-many-positional-arguments]
            unit,
            system=self.system,  # ty: ignore[unknown-argument]
        )

    def __array_function__(
        self,
        func: Any,
        types: tuple[type, ...],
        args: tuple[Any, ...],
        kwargs: dict[str, Any],
    ) -> Any:
        """Handles NumPy functions like np.concatenate, np.mean."""
        try:
            import numpy as np
        except ImportError:
            return NotImplemented

        if func == np.concatenate:
            return self._numpy_concatenate(np, args, kwargs)

        if func in (np.isclose, np.allclose):
            return _isclose(np, func is np.allclose, args, kwargs)

        if func in (np.sum, np.mean):
            q = args[0]
            if not isinstance(q, _q()):
                return NotImplemented
            # np.sum(a, axis, dtype, out, keepdims): positional ones named
            names = ("axis", "dtype", "out", "keepdims")
            named = dict(zip(names[: len(args) - 1], args[1:], strict=True))
            return q._numpy_sum_or_mean(
                np, func is np.mean, {**named, **kwargs}
            )

        return NotImplemented

    def _numpy_sum_or_mean(
        self, np: Any, mean: bool, kwargs: dict[str, Any]
    ) -> Any:
        """`np.sum` or `np.mean` of this quantity, its unit and uncertainty kept.

        An exact array reduces its numbers (vectorised, any NumPy keyword). With an
        uncertainty, the whole array is reduced: the value is the sum of the numbers and
        the standard deviation that of `J x` with `J` a row of ones, which the core's
        covariance store propagates (`J Σ Jᵀ`), so elements that share an error
        (`x * [1, 1, 2]`) count it once; independent elements give
        `sqrt(sum(u_i^2))`, the uncertainty of `q[0] + q[1] + ...`. The mean is the sum
        divided by `n`. The result is a new scalar measurement, as `q[i]` is.
        """
        name = "np.mean" if mean else "np.sum"
        if self._is_level():  # pyright: ignore[reportAttributeAccessIssue]
            raise TypeError(
                f"{name} of levels ('{self.unit}'): a level adds as the quantity it "
                "stands for, not by its numbers; convert to its plain unit first"
            )
        if not mean and self._is_point():  # pyright: ignore[reportAttributeAccessIssue]
            raise ValueError(
                f"{name} of temperature points ('{self.unit}'): cannot add absolute "
                "quantities; np.mean of points, or a sum of differences, has a meaning"
            )
        magnitude: Any = self.magnitude
        backend = self._backend
        shape = (
            tuple(backend.shape(magnitude))
            if backend.is_array(magnitude)
            else ()
        )
        if not shape:
            return self  # one number: its own sum and mean
        if len(shape) == 1 and kwargs.get("axis") in (0, -1, (0,), (-1,)):
            kwargs = {**kwargs, "axis": None}
        reduce = np.mean if mean else np.sum
        if self._is_exact_array(np):
            # ponytail: same mixin-blindness as _numpy_concatenate above.
            return type(self)(
                reduce(magnitude, **kwargs),  # pyright: ignore[reportCallIssue]  # ty: ignore[too-many-positional-arguments]
                self.unit,
                system=self.system,  # ty: ignore[unknown-argument]
            )
        refused = sorted(k for k, v in kwargs.items() if v is not None)
        if refused:
            raise TypeError(
                f"{name} of an array-backed quantity with an uncertainty reduces the "
                f"whole array; {', '.join(refused)} (along an axis, a dtype, an out "
                "array) cannot propagate the uncertainty yet: index the quantity first, "
                "or take the magnitude on purpose (q.magnitude)"
            )
        size = backend.size(magnitude)
        total = backend.sum(magnitude)
        sigma = self._sum_standard_deviation(np, size, name)
        if mean:
            total, sigma = total / size, sigma / size
        return type(self).from_input(  # pyright: ignore[reportAttributeAccessIssue]
            total, self.unit, self.system, uncertainty=sigma
        )

    def _is_exact_array(self, np: Any) -> bool:
        """No uncertainty on any element; one pass (`max`) for a NumPy array of them.

        `_has_uncertainty` builds a boolean array first, which costs more than the
        reduction itself. A standard deviation is never negative, and a NaN is not zero.
        """
        known = self.__dict__.get("_exact_array")
        if known is not None:
            # the uncertainty is fixed when the quantity is made (`q[i] = x` sets
            # the magnitude only), so the answer is kept
            return known
        unc = self._numeric_std_dev  # pyright: ignore[reportAttributeAccessIssue]
        if isinstance(unc, np.ndarray) and unc.size:
            exact = bool(unc.max() == 0)
        else:
            exact = not self._has_uncertainty  # pyright: ignore[reportAttributeAccessIssue]
        object.__setattr__(self, "_exact_array", exact)
        return exact

    def _sum_standard_deviation(self, np: Any, size: int, name: str) -> float:
        """The standard deviation of the sum of the elements, by the core's covariance store."""
        from physure.domain.measurement.uncertainty import (
            CovarianceModel,
            VarianceModel,
        )
        from physure.domain.measurement.vectorized_uncertainty import (
            _dense_host,
            ensure_store,
        )

        model = self._uncertainty_obj  # pyright: ignore[reportAttributeAccessIssue]
        backend = self._backend
        store = ensure_store(backend)
        if isinstance(model, CovarianceModel):
            elements = model.ensure_vector_slice(backend)
        elif isinstance(model, VarianceModel):
            # an array of variances only comes out of arithmetic that already dropped
            # how its elements are correlated (an array with a scalar of the
            # uncorrelated mode shares that scalar's error in every element): summed
            # as independent, it would give a confident uncertainty that is too small
            raise TypeError(
                f"{name} cannot propagate the uncertainty of these elements: their "
                "correlation was lost by the arithmetic that made them (variances "
                "only, as in the uncorrelated mode), so a sum of them as independent "
                "errors could be too small; build the array in the default mode, or "
                "sum the magnitudes on purpose (q.magnitude)"
            )
        else:
            raise TypeError(
                f"{name} cannot propagate an uncertainty held as "
                f"{type(model).__name__}: only a covariance or variance model of the "
                "array says how its elements are correlated"
            )
        out = store.allocate(1)
        store.update_from_propagation(out, [elements], [np.ones((1, size))])
        block = store.get_covariance_block(out, out)
        variance = np.asarray(
            _dense_host(backend.sparse_diagonal(block)), dtype=float
        ).reshape(-1)[0]
        return float(np.sqrt(variance))

    @classmethod
    def _torch_unwrap(cls, obj: Any) -> Any:
        """Recursively unwrap Quantity objects to their raw magnitudes."""
        if isinstance(obj, _q()):
            return obj.magnitude
        if isinstance(obj, (list, tuple)):
            return type(obj)(cls._torch_unwrap(x) for x in obj)
        return obj

    @classmethod
    def _torch_arithmetic(cls, func: Any, args: tuple[Any, ...]) -> Any:
        """Delegate torch arithmetic to the Quantity operators.

        `tensor + quantity` reaches here as `Tensor.add` (the method, not `torch.add`),
        with the tensor first. The operator has to be the quantity's reflected one:
        calling `operator.add(tensor, quantity)` would come straight back into this
        hook, and falling through to `_torch_fallback` would stamp the quantity's unit
        on the tensor (a bare tensor added to seconds is seconds, and `tensor / seconds`
        is seconds).
        """
        import torch

        forward = {
            "add": operator.add,
            "sub": operator.sub,
            "mul": operator.mul,
            "div": operator.truediv,
            "true_divide": operator.truediv,
            "pow": operator.pow,
        }
        reflected = {
            "add": "__radd__",
            "sub": "__rsub__",
            "mul": "__rmul__",
            "div": "__rtruediv__",
            "true_divide": "__rtruediv__",
        }
        name = next(
            (
                n
                for n in forward
                if func in (getattr(torch, n), getattr(torch.Tensor, n))
            ),
            None,
        )
        if name is None or len(args) < 2:
            return NotImplemented
        left, right = args[0], args[1]
        if isinstance(left, _q()):
            return forward[name](left, right)
        if isinstance(right, _q()) and name in reflected:
            return getattr(right, reflected[name])(left)
        return NotImplemented

    @classmethod
    def _torch_unary_math(
        cls, func: Any, args: tuple[Any, ...], kwargs: dict[str, Any]
    ) -> Any:
        """Handle torch unary math (sqrt, abs, trig)."""
        import torch

        trig_map = {
            torch.sin,
            torch.cos,
            torch.tan,
            torch.exp,
            torch.log,
            torch.log10,
            torch.abs,
            torch.sqrt,
        }
        if func not in trig_map:
            return NotImplemented
        q = args[0]
        if not isinstance(q, _q()):
            return NotImplemented
        if func == torch.sqrt:
            return q**0.5
        if func == torch.abs:
            return abs(q)
        # Others require dimensionless
        if not q.dimension.is_dimensionless:
            raise IncompatibleUnitsError(q.unit, CompoundUnit({}))
        res_mag = func(q.magnitude, **kwargs)
        return cls.from_input(res_mag, CompoundUnit({}), q.system)

    @classmethod
    def _torch_fallback(
        cls, func: Any, args: tuple[Any, ...], kwargs: dict[str, Any]
    ) -> Any:
        """Unwrap args, call func, and re-wrap the result with the first Quantity's unit."""
        import torch

        unwrapped_args = tuple(cls._torch_unwrap(arg) for arg in args)
        unwrapped_kwargs = {k: cls._torch_unwrap(v) for k, v in kwargs.items()}
        result = func(*unwrapped_args, **unwrapped_kwargs)
        source_q = next((arg for arg in args if isinstance(arg, _q())), None)
        if source_q is not None and isinstance(result, torch.Tensor):
            return cls.from_input(result, source_q.unit, source_q.system)
        return result

    @classmethod
    def __torch_function__(
        cls,
        func: Any,
        types: tuple[type, ...],
        args: tuple[Any, ...] = (),
        kwargs: dict[str, Any] | None = None,
    ) -> Any:
        """Handles Torch functions like torch.mean, torch.relu."""
        if kwargs is None:
            kwargs = {}

        arith = cls._torch_arithmetic(func, args)
        if arith is not NotImplemented:
            return arith

        unary = cls._torch_unary_math(func, args, kwargs)
        if unary is not NotImplemented:
            return unary

        return cls._torch_fallback(func, args, kwargs)

    def to_device(self, device: str) -> Self:
        """Moves the quantity and its uncertainty to the specified device."""
        new_mag = self._backend.to_device(self.magnitude, device)
        new_unc_val = self._backend.to_device(self.uncertainty, device)
        new_unc = Uncertainty.from_standard(new_unc_val)

        return self._fast_new(
            new_mag,
            self.unit,
            new_unc,
            self.system,
            self.dimension,
            self._backend,
        )

    def backward(self, *args: Any, **kwargs: Any) -> None:
        """Delegates autograd backward call to the underlying magnitude."""
        if hasattr(self.magnitude, "backward"):
            self.magnitude.backward(*args, **kwargs)
        else:
            raise TypeError(
                f"Backend magnitude {type(self.magnitude)} no backward()"
            )

    # --- Representation ---
