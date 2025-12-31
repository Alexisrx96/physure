from __future__ import annotations

import importlib
import math
from collections.abc import Sequence
from typing import Any, Dict, Optional, Union

from measurekit.core.protocols import BackendOps


class PythonBackend(BackendOps):
    """Fallback backend for native Python types using the math module."""

    def is_array(self, obj: Any) -> bool:
        return False

    def asarray(self, obj: Any) -> Any:
        return obj

    def to_device(self, obj: Any, device: str) -> Any:
        return obj

    def add(self, x: Any, y: Any) -> Any:
        return x + y

    def sub(self, x: Any, y: Any) -> Any:
        return x - y

    def mul(self, x: Any, y: Any) -> Any:
        return x * y

    def truediv(self, x: Any, y: Any) -> Any:
        return x / y

    def pow(self, x: Any, y: Any) -> Any:
        return x**y

    def sqrt(self, x: Any) -> Any:
        return math.sqrt(x)

    def exp(self, x: Any) -> Any:
        return math.exp(x)

    def log(self, x: Any) -> Any:
        return math.log(x)

    def sin(self, x: Any) -> Any:
        return math.sin(x)

    def cos(self, x: Any) -> Any:
        return math.cos(x)

    def tan(self, x: Any) -> Any:
        return math.tan(x)

    def dot(self, x: Any, y: Any) -> Any:
        return x * y

    def cross(self, x: Any, y: Any) -> Any:
        raise NotImplementedError("Cross product not supported for scalars")

    def abs(self, x: Any) -> Any:
        return abs(x)

    def sign(self, x: Any) -> Any:
        return math.copysign(1, x)

    def sum(
        self, obj: Any, axis: Union[int, Sequence[int], None] = None
    ) -> Any:
        if isinstance(obj, (list, tuple)):
            return sum(obj)
        return obj

    def mean(
        self, obj: Any, axis: Union[int, Sequence[int], None] = None
    ) -> Any:
        if isinstance(obj, (list, tuple)):
            return sum(obj) / len(obj)
        return obj

    def any(self, obj: Any) -> bool:
        if isinstance(obj, (list, tuple)):
            return any(obj)
        return bool(obj)

    def all(self, obj: Any) -> bool:
        if isinstance(obj, (list, tuple)):
            return all(obj)
        return bool(obj)

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        return math.isclose(a, b, rel_tol=rtol, abs_tol=atol)

    def equal(self, x: Any, y: Any) -> Any:
        return x == y

    def not_equal(self, x: Any, y: Any) -> Any:
        return x != y

    def less(self, x: Any, y: Any) -> Any:
        return x < y

    def less_equal(self, x: Any, y: Any) -> Any:
        return x <= y

    def greater(self, x: Any, y: Any) -> Any:
        return x > y

    def greater_equal(self, x: Any, y: Any) -> Any:
        return x >= y

    def shape(self, obj: Any) -> tuple[int, ...]:
        if hasattr(obj, "__len__"):
            return (len(obj),)
        return ()

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        if shape == ():
            return obj
        if shape == (1,):
            return obj
        raise NotImplementedError(
            "Reshape not fully supported in PythonBackend"
        )

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        result = []
        for arr in arrays:
            if isinstance(arr, list):
                result.extend(arr)
            else:
                result.append(arr)
        return result

    def eye(self, N: int, format: str = "csr") -> Any:
        raise NotImplementedError(
            "Sparse matrices not supported in PythonBackend"
        )

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
        raise NotImplementedError(
            "Sparse matrices not supported in PythonBackend"
        )

    def ones(self, shape: tuple[int, ...]) -> Any:
        raise NotImplementedError(
            "Ones array creation not supported in PythonBackend"
        )


class BackendManager:
    """Manages backend dispatching and lazy loading."""

    _backends: Dict[str, BackendOps] = {}
    _python_backend: Optional[BackendOps] = None

    @classmethod
    def get_backend(cls, data_obj: Any) -> BackendOps:
        """Determines the appropriate backend for the given data object."""
        type_str = str(type(data_obj)).lower()

        if "numpy" in type_str or "ndarray" in type_str:
            return cls._get_or_load_backend("numpy")
        elif "torch" in type_str:
            return cls._get_or_load_backend("torch")
        elif "jax" in type_str:
            return cls._get_or_load_backend("jax")

        if isinstance(data_obj, (int, float, complex, list, tuple)):
            return cls._get_python_backend()

        try:
            return cls._get_or_load_backend("numpy")
        except (ImportError, ModuleNotFoundError):
            return cls._get_python_backend()

    @classmethod
    def _get_or_load_backend(cls, name: str) -> BackendOps:
        if name not in cls._backends:
            if name == "numpy":
                module = importlib.import_module(
                    "measurekit.backends.numpy_backend"
                )
                cls._backends[name] = module.NumpyBackend()
            else:
                raise ValueError(f"Backend '{name}' not supported yet.")
        return cls._backends[name]

    @classmethod
    def _get_python_backend(cls) -> BackendOps:
        if cls._python_backend is None:
            cls._python_backend = PythonBackend()
        return cls._python_backend


def get_backend(data_obj: Any) -> BackendOps:
    """Convenience function to get backend for an object."""
    return BackendManager.get_backend(data_obj)
