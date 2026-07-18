"""NumPy and PyTorch integration mixin for Quantity."""

from __future__ import annotations

import operator
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from typing_extensions import Self

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


class BackendMixin:
    """NumPy ufunc/function and PyTorch function dispatch methods."""

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
        """Handle np.add.reduce (e.g. np.sum); return NotImplemented for others."""
        import numpy as np

        if ufunc != np.add:
            return NotImplemented
        inp = inputs[0]
        if not isinstance(inp, _q()):
            return NotImplemented
        res_mag = self._backend.sum(inp.magnitude, axis=kwargs.get("axis"))
        return type(self).from_input(res_mag, inp.unit, self.system)

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

        if func == np.mean:
            q = args[0]
            if isinstance(q, _q()):
                # ponytail: same mixin-blindness as _numpy_concatenate above.
                return type(self)(
                    np.mean(q.magnitude, **kwargs),  # pyright: ignore[reportCallIssue]  # ty: ignore[too-many-positional-arguments]
                    q.unit,
                    system=q.system,  # ty: ignore[unknown-argument]
                )

        return NotImplemented

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
        """Delegate torch arithmetic to Python operators."""
        import torch

        if func == torch.add:
            return operator.add(args[0], args[1])
        if func == torch.sub:
            return operator.sub(args[0], args[1])
        if func == torch.mul:
            return operator.mul(args[0], args[1])
        if func in (torch.div, torch.true_divide):
            return operator.truediv(args[0], args[1])
        if func == torch.pow:
            return operator.pow(args[0], args[1])
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
