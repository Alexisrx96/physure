from __future__ import annotations

import importlib
import math
import os
from typing import TYPE_CHECKING, Any, ClassVar

from measurekit.core.protocols import BackendOps, Boolean, Numeric

if TYPE_CHECKING:
    from collections.abc import Sequence

    try:
        from jaxtyping import Array, Bool, Float, typecheck
    except ImportError:
        Array = Any
        Bool = Any
        Float = Any

        def typecheck(func):
            return func

    try:
        from beartype import beartype
    except ImportError:
        beartype = typecheck
else:
    Array = Any
    Bool = Any
    Float = Any


def enforce_tensor_contract(func):
    """Decorator to enforce jaxtyping contracts at runtime."""
    if os.environ.get("MEASUREKIT_DEBUG") == "1":
        try:
            from beartype import beartype

            return beartype(func)
        except ImportError:
            return func
    return func


import functools


def ensure_backend_compatible(func):
    """Ensures that inputs are converted to backend-compatible arrays."""

    @functools.wraps(func)
    def wrapper(self, *args, **kwargs):
        # 'self' is the backend
        converted_args = [
            self.asarray(arg) if isinstance(arg, (list, tuple)) else arg
            for arg in args
        ]
        return func(self, *converted_args, **kwargs)

    return wrapper


class PythonBackend(BackendOps):
    """Fallback backend for native Python types using the math module."""

    SPARSE_NOT_SUPPORTED = "Sparse matrices not supported in PythonBackend"

    def is_array(self, obj: Any) -> bool:
        """Checks if the object is an array."""
        return False

    def is_tracing(self, obj: Any) -> bool:
        """Checks if the object is being traced (always False here)."""
        return False

    def asarray(self, obj: Any) -> Array:
        """Converts to a form suitable for this backend (identity)."""
        return obj

    def to_device(self, obj: Any, device: str) -> Array:
        """Moves the object to the specified device (identity)."""
        return obj

    def get_device(self, obj: Any) -> str | None:
        """Returns the device of the object (always 'cpu')."""
        return "cpu"

    @enforce_tensor_contract
    def add(self, x: Numeric, y: Numeric) -> Numeric:
        """Adds two values (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a + b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a + y for a in x]
        if isinstance(y, (list, tuple)):
            return [x + b for b in y]
        return x + y

    @enforce_tensor_contract
    def sub(self, x: Numeric, y: Numeric) -> Numeric:
        """Subtracts two values (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a - b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a - y for a in x]
        if isinstance(y, (list, tuple)):
            return [x - b for b in y]
        return x - y

    @enforce_tensor_contract
    def mul(self, x: Numeric, y: Numeric) -> Numeric:
        """Multiplies two values (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a * b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a * y for a in x]
        if isinstance(y, (list, tuple)):
            return [x * b for b in y]
        return x * y

    @enforce_tensor_contract
    def truediv(self, x: Numeric, y: Numeric) -> Numeric:
        """Divides two values (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a / b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a / y for a in x]
        if isinstance(y, (list, tuple)):
            return [x / b for b in y]
        return x / y

    @enforce_tensor_contract
    def pow(self, x: Numeric, y: Numeric) -> Numeric:
        """Raises x to the power of y (or lists element-wise)."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a**b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a**y for a in x]
        if isinstance(y, (list, tuple)):
            return [x**b for b in y]
        return x**y

    @enforce_tensor_contract
    def sqrt(self, x: Numeric) -> Numeric:
        """Returns the square root of x."""
        if isinstance(x, (list, tuple)):
            return [math.sqrt(val) for val in x]
        return math.sqrt(x)

    @enforce_tensor_contract
    def exp(self, x: Numeric) -> Numeric:
        """Returns the exponential of x."""
        if isinstance(x, (list, tuple)):
            return [math.exp(val) for val in x]
        return math.exp(x)

    @enforce_tensor_contract
    def log(self, x: Numeric) -> Numeric:
        """Returns the natural logarithm of x."""
        if isinstance(x, (list, tuple)):
            return [math.log(val) for val in x]
        return math.log(x)

    @enforce_tensor_contract
    def sin(self, x: Numeric) -> Numeric:
        """Returns the sine of x."""
        if isinstance(x, (list, tuple)):
            return [math.sin(val) for val in x]
        return math.sin(x)

    @enforce_tensor_contract
    def cos(self, x: Numeric) -> Numeric:
        """Returns the cosine of x."""
        if isinstance(x, (list, tuple)):
            return [math.cos(val) for val in x]
        return math.cos(x)

    @enforce_tensor_contract
    def tan(self, x: Numeric) -> Numeric:
        """Returns the tangent of x."""
        if isinstance(x, (list, tuple)):
            return [math.tan(val) for val in x]
        return math.tan(x)

    @enforce_tensor_contract
    def dot(self, x: Numeric, y: Numeric) -> Numeric:
        """Computes the dot product of two values."""
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            # Assuming dot product for 1D lists
            if len(x) != len(y):
                raise ValueError("Lists must have same length for dot product")
            return sum(a * b for a, b in zip(x, y, strict=False))
        return x * y

    @enforce_tensor_contract
    def cross(self, x: Numeric, y: Numeric) -> Numeric:
        """Computes the cross product of two values (not supported)."""
        raise NotImplementedError("Cross product not supported for scalars")

    @enforce_tensor_contract
    def abs(self, x: Numeric) -> Numeric:
        """Returns the absolute value of x."""
        if isinstance(x, (list, tuple)):
            return [abs(val) for val in x]
        return abs(x)

    @enforce_tensor_contract
    def sign(self, x: Numeric) -> Numeric:
        """Returns the sign of x (-1, 0, or 1)."""
        if isinstance(x, (list, tuple)):
            return [math.copysign(1, val) for val in x]
        return math.copysign(1, x)

    @enforce_tensor_contract
    def sum(
        self, obj: Any, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Computes the sum of elements."""
        if isinstance(obj, (list, tuple)):
            return sum(obj)
        return obj

    @enforce_tensor_contract
    def mean(
        self, obj: Numeric, axis: int | Sequence[int] | None = None
    ) -> Numeric:
        """Computes the arithmetic mean."""
        if isinstance(obj, (list, tuple)):
            return sum(obj) / len(obj)
        return obj

    @enforce_tensor_contract
    def any(self, obj: Boolean) -> bool:
        """Returns True if any element is True."""
        if isinstance(obj, (list, tuple)):
            return any(obj)
        return bool(obj)

    @enforce_tensor_contract
    def all(self, obj: Boolean) -> bool:
        """Returns True if all elements are True."""
        if isinstance(obj, (list, tuple)):
            return all(obj)
        return bool(obj)

    @enforce_tensor_contract
    def allclose(
        self, a: Numeric, b: Numeric, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        """Returns True if arrays are equal within a tolerance."""
        try:
            return math.isclose(a, b, rel_tol=rtol, abs_tol=atol)
        except TypeError:
            return a == b

    @enforce_tensor_contract
    def equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x equals y."""
        return x == y

    @enforce_tensor_contract
    def not_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x does not equal y."""
        return x != y

    @enforce_tensor_contract
    def less(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is less than y."""
        return x < y

    @enforce_tensor_contract
    def less_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is less than or equal to y."""
        return x <= y

    @enforce_tensor_contract
    def greater(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is greater than y."""
        return x > y

    @enforce_tensor_contract
    def greater_equal(self, x: Numeric, y: Numeric) -> Boolean:
        """Returns True if x is greater than or equal to y."""
        return x >= y

    def shape(self, obj: Any) -> tuple[int, ...]:
        """Returns the shape of the object."""
        if hasattr(obj, "__len__"):
            return (len(obj),)
        return ()

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        """Reshapes the object to the specified shape."""
        if shape == () or shape == (1,):
            if isinstance(obj, (list, tuple)):
                if len(obj) == 1:
                    return obj[0]
                if len(obj) == 0:
                    return 0.0
            return obj

        if len(shape) == 1:
            return self._reshape_1d(obj, shape[0])

        if shape == (1, 1) and not isinstance(obj, (list, tuple)):
            return [[obj]]

        raise NotImplementedError(
            f"Reshape {shape} not fully supported in PythonBackend."
        )

    def _reshape_1d(self, obj: Any, total: int) -> Any:
        """Helper for 1D reshaping/flattening."""
        if isinstance(obj, (list, tuple)):
            # If nested, flatten
            if len(obj) > 0 and isinstance(obj[0], (list, tuple)):
                flat = [item for sublist in obj for item in sublist]
            else:
                flat = list(obj)

            if total == -1 or len(flat) == total:
                return flat

        # Scalar to vector
        if (total == 1 or total == -1) and not isinstance(obj, (list, tuple)):
            return [obj]
        return obj

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
        """Concatenates arrays along the specified axis."""
        result = []
        for arr in arrays:
            if isinstance(arr, list):
                result.extend(arr)
            else:
                result.append(arr)
        return result

    def size(self, obj: Any) -> int:
        """Returns the total number of elements in the object."""
        if hasattr(obj, "__len__"):
            return len(obj)
        return 1

    def broadcast_and_flatten(self, inputs: Sequence[Any]) -> Sequence[Any]:
        """Broadcasts inputs to a common shape and flattens them."""
        # Basic scalar/list broadcasting simulation
        max_len = 0
        for x in inputs:
            if isinstance(x, (list, tuple)):
                max_len = max(max_len, len(x))
            else:
                max_len = max(max_len, 1)

        results = []
        for x in inputs:
            if isinstance(x, (list, tuple)):
                if len(x) == max_len:
                    results.append(list(x))
                elif len(x) == 1:
                    results.append(list(x) * max_len)
                else:
                    raise ValueError(f"Shape mismatch: {len(x)} vs {max_len}")
            else:
                results.append([x] * max_len)
        return results

    def identity_operator(self, size: int, reference: Any = None) -> Any:
        """Returns an identity matrix of the specified size."""
        # Python backend doesn't have devices/sparse operators really,
        # but we return an identity-like structure
        return [
            [1.0 if i == j else 0.0 for j in range(size)] for i in range(size)
        ]

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator from the given values."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Constructs a sparse matrix from COO data."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix of other matrices."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Matrix multiplication where at least one operand is sparse."""
        return a @ b

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        raise NotImplementedError(self.SPARSE_NOT_SUPPORTED)

    def eye(self, n: int, format: str = "csr", reference: Any = None) -> Any:
        """Returns a sparse identity matrix (not supported)."""
        return self.identity_operator(n)

    def sparse_eye(self, n: int, reference: Any = None) -> Any:
        """Returns a sparse identity matrix (not supported)."""
        return self.identity_operator(n)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
        """Constructs a sparse matrix from diagonals (not supported)."""
        # Unused arguments: offsets, format
        return self.diagonal_operator(diagonals[0])

    def ones(self, shape: tuple[int, ...], reference: Any = None) -> Any:
        """Returns an array of ones with the specified shape."""
        if len(shape) == 0:
            return 1.0
        if len(shape) == 1:
            return [1.0] * shape[0]
        return [[1.0] * shape[-1] for _ in range(shape[-2])] if len(shape) == 2 else [1.0]


class CoreBackend(BackendOps):
    """Backend for the Rust-based QuantityInner core."""

    def is_array(self, obj: Any) -> bool:
        return False

    def is_tracing(self, obj: Any) -> bool:
        return False

    def add(self, x: Any, y: Any) -> Any:
        """Adds two core quantities or a core quantity and a scalar."""
        return x + y

    def sub(self, x: Any, y: Any) -> Any:
        """Subtracts two core quantities or a core quantity and a scalar."""
        return x - y

    def mul(self, x: Any, y: Any) -> Any:
        """Multiplies two core quantities or a core quantity and a scalar."""
        return x * y

    def truediv(self, x: Any, y: Any) -> Any:
        """Divides two core quantities or a core quantity and a scalar."""
        return x / y

    def pow(self, x: Any, y: Any) -> Any:
        """Computes power of a core quantity."""
        return x**y

    def sqrt(self, x: Any) -> Any:
        """Computes square root of a core quantity."""
        return x**0.5

    def exp(self, x: Any) -> Any:
        """Computes exponential of a core quantity."""
        return x.propagate_function("exp")

    def log(self, x: Any) -> Any:
        """Computes natural logarithm of a core quantity."""
        return x.propagate_function("log")

    def sin(self, x: Any) -> Any:
        """Computes sine of a core quantity."""
        return x.propagate_function("sin")

    def cos(self, x: Any) -> Any:
        """Computes cosine of a core quantity."""
        return x.propagate_function("cos")


class BackendManager:
    """Manages backend dispatching and lazy loading."""

    _backends: ClassVar[dict[str, BackendOps]] = {}
    _python_backend: ClassVar[BackendOps | None] = None

    @classmethod
    def get_backend(cls, data_obj: Any) -> BackendOps:
        """Determines the appropriate backend for the given data object."""
        # Use module-based detection to avoid eager imports
        module = getattr(data_obj.__class__, "__module__", "")

        if isinstance(data_obj, (int, float, complex, list, tuple)):
            return cls._get_python_backend()

        if "measurekit_core.Quantity" in str(type(data_obj)):
            return cls._get_or_load_backend("core")

        cls_name = getattr(data_obj.__class__, "__name__", "").lower()

        # SciPy Detection (for sparse matrices)
        if (
            module.startswith("scipy")
            or cls_name == "csr_matrix"
            or cls_name == "csc_matrix"
            or cls_name == "coo_matrix"
        ):
            try:
                import scipy.sparse  # noqa: F401

                return cls._get_or_load_backend("numpy")
            except ImportError:
                # If scipy is not installed, fallback to python
                pass

        # JAX Detection (Prioritize to catch complex Tracers)
        if (
            module.startswith("jax")
            or module.startswith("jaxlib")
            or "jax" in module
            or "jax" in cls_name
            or "tracer" in cls_name
            or hasattr(data_obj, "aval")  # JAX 'aval'
        ):
            return cls._get_or_load_backend("jax")

        if module.startswith("torch"):
            return cls._get_or_load_backend("torch")

        if (
            module.startswith("numpy")
            or "numpy" in str(type(data_obj)).lower()
            or "ndarray" in str(type(data_obj)).lower()
        ):
            return cls._get_or_load_backend("numpy")

        try:
            return cls._get_or_load_backend("numpy")
        except ImportError:
            return cls._get_python_backend()

    @classmethod
    def _get_or_load_backend(cls, name: str) -> BackendOps:
        if name not in cls._backends:
            if name == "numpy":
                module = importlib.import_module(
                    "measurekit.backends.numpy_backend"
                )
                cls._backends[name] = module.NumpyBackend()
            elif name == "torch":
                module = importlib.import_module(
                    "measurekit.backends.torch_backend"
                )
                cls._backends[name] = module.TorchBackend()
            elif name == "jax":
                module = importlib.import_module(
                    "measurekit.backends.jax_backend"
                )
                cls._backends[name] = module.JaxBackend()
                # Trigger JAX Pytree registration
                if hasattr(module, "register_jax_behavior"):
                    module.register_jax_behavior()
            elif name == "core":
                cls._backends[name] = CoreBackend()
            else:
                raise ValueError(f"Backend '{name}' not supported yet.")
        return cls._backends[name]

    @classmethod
    def _get_python_backend(cls) -> BackendOps:
        if cls._python_backend is None:
            cls._python_backend = PythonBackend()
        return cls._python_backend

    @classmethod
    def clear_backends(cls) -> None:
        """Clears all cached backends. Useful for test isolation."""
        cls._backends.clear()
        cls._python_backend = None


@enforce_tensor_contract
def get_backend(data_obj: Any) -> BackendOps:
    """Convenience function to get backend for an object."""
    return BackendManager.get_backend(data_obj)
