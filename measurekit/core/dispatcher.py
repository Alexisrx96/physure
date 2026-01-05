from __future__ import annotations

import importlib
import math
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Sequence

try:
    from jaxtyping import Array, Bool, Float
except (ImportError, ModuleNotFoundError):
    # Fallback if jaxtyping or its dependencies (jax) are missing
    from typing import Any

    Array = Any
    Bool = Any
    Float = Any


from measurekit.core.protocols import BackendOps


class PythonBackend(BackendOps):
    """Fallback backend for native Python types using the math module."""

    def is_array(self, obj: Any) -> bool:
        return False

    def is_tracing(self, obj: Any) -> bool:
        return False

    def asarray(self, obj: Any) -> Array:
        return obj

    def to_device(self, obj: Any, device: str) -> Array:
        return obj

    def get_device(self, obj: Any) -> str | None:
        return "cpu"

    def add(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a + b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a + y for a in x]
        if isinstance(y, (list, tuple)):
            return [x + b for b in y]
        return x + y

    def sub(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a - b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a - y for a in x]
        if isinstance(y, (list, tuple)):
            return [x - b for b in y]
        return x - y

    def mul(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a * b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a * y for a in x]
        if isinstance(y, (list, tuple)):
            return [x * b for b in y]
        return x * y

    def truediv(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a / b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a / y for a in x]
        if isinstance(y, (list, tuple)):
            return [x / b for b in y]
        return x / y

    def pow(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            return [a**b for a, b in zip(x, y, strict=False)]
        if isinstance(x, (list, tuple)):
            return [a**y for a in x]
        if isinstance(y, (list, tuple)):
            return [x**b for b in y]
        return x**y

    def sqrt(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [math.sqrt(val) for val in x]
        return math.sqrt(x)

    def exp(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [math.exp(val) for val in x]
        return math.exp(x)

    def log(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [math.log(val) for val in x]
        return math.log(x)

    def sin(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [math.sin(val) for val in x]
        return math.sin(x)

    def cos(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [math.cos(val) for val in x]
        return math.cos(x)

    def tan(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [math.tan(val) for val in x]
        return math.tan(x)

    def dot(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
            # Assuming dot product for 1D lists
            if len(x) != len(y):
                raise ValueError("Lists must have same length for dot product")
            return sum(a * b for a, b in zip(x, y, strict=False))
        return x * y

    def cross(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Float[Array, ...]:
        raise NotImplementedError("Cross product not supported for scalars")

    def abs(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [abs(val) for val in x]
        return abs(x)

    def sign(self, x: Float[Array, ...]) -> Float[Array, ...]:
        if isinstance(x, (list, tuple)):
            return [math.copysign(1, val) for val in x]
        return math.copysign(1, x)

    def sum(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        if isinstance(obj, (list, tuple)):
            return sum(obj)
        return obj

    def mean(
        self, obj: Float[Array, ...], axis: int | Sequence[int] | None = None
    ) -> Float[Array, ...]:
        if isinstance(obj, (list, tuple)):
            return sum(obj) / len(obj)
        return obj

    def any(self, obj: Bool[Array, ...]) -> bool:
        if isinstance(obj, (list, tuple)):
            return any(obj)
        return bool(obj)

    def all(self, obj: Bool[Array, ...]) -> bool:
        if isinstance(obj, (list, tuple)):
            return all(obj)
        return bool(obj)

    def allclose(
        self, a: Any, b: Any, rtol: float = 1e-5, atol: float = 1e-8
    ) -> bool:
        return math.isclose(a, b, rel_tol=rtol, abs_tol=atol)

    def equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        return x == y

    def not_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        return x != y

    def less(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        return x < y

    def less_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        return x <= y

    def greater(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        return x > y

    def greater_equal(
        self, x: Float[Array, ...], y: Float[Array, ...]
    ) -> Bool[Array, ...]:
        return x >= y

    def shape(self, obj: Any) -> tuple[int, ...]:
        if hasattr(obj, "__len__"):
            return (len(obj),)
        return ()

    def reshape(self, obj: Any, shape: tuple[int, ...]) -> Any:
        if shape == ():
            if isinstance(obj, (list, tuple)):
                if len(obj) == 1:
                    return obj[0]
                if len(obj) == 0:
                    return 0.0  # obscure case
            return obj

        # Flatten logic for (N,)
        if len(shape) == 1:
            total = shape[0]
            if isinstance(obj, (list, tuple)):
                # If nested, flatten
                if len(obj) > 0 and isinstance(obj[0], (list, tuple)):
                    flat = [item for sublist in obj for item in sublist]
                    if len(flat) == total:
                        return flat
                if len(obj) == total:
                    return obj
            # Scalar to vector
            if total == 1 and not isinstance(obj, (list, tuple)):
                return [obj]

        raise NotImplementedError(
            f"Reshape {shape} not fully supported in PythonBackend for obj {obj}"
        )

    def concatenate(self, arrays: Sequence[Any], axis: int = 0) -> Any:
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
        """Broadcasts inputs to a common shape and returns them as flattened 1D lists."""
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
        # Python backend doesn't have devices/sparse operators really,
        # but we return an identity-like structure
        return [
            [1.0 if i == j else 0.0 for j in range(size)] for i in range(size)
        ]

    def diagonal_operator(self, diagonal: Any) -> Any:
        """Returns a diagonal operator (matrix) from the given diagonal values."""
        raise NotImplementedError(
            "Sparse matrices not supported in PythonBackend"
        )

    def sparse_matrix(
        self,
        data: Any,
        indices: tuple[Any, Any],
        shape: tuple[int, int],
    ) -> Any:
        """Constructs a sparse matrix from COO data."""
        raise NotImplementedError(
            "Sparse matrices not supported in PythonBackend"
        )

    def sparse_diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        shape: tuple[int, int] | None = None,
    ) -> Any:
        """Constructs a sparse matrix from diagonals."""
        raise NotImplementedError(
            "Sparse matrices not supported in PythonBackend"
        )

    def sparse_bmat(
        self,
        blocks: Sequence[Sequence[Any | None]],
    ) -> Any:
        """Constructs a sparse matrix from a block matrix of other matrices."""
        raise NotImplementedError(
            "Sparse matrices not supported in PythonBackend"
        )

    def sparse_matmul(self, a: Any, b: Any) -> Any:
        """Performs matrix multiplication where at least one operand may be sparse."""
        return a @ b

    def sparse_diagonal(self, a: Any) -> Any:
        """Returns the diagonal elements of a (potentially sparse) matrix."""
        raise NotImplementedError(
            "Sparse matrices not supported in PythonBackend"
        )

    def eye(self, n: int, format: str = "csr", reference: Any = None) -> Any:
        return self.identity_operator(n)

    def diags(
        self,
        diagonals: Sequence[Any],
        offsets: Sequence[int],
        format: str = "csr",
    ) -> Any:
        return self.diagonal_operator(diagonals[0])

    def ones(self, shape: tuple[int, ...], reference: Any = None) -> Any:
        if len(shape) == 0:
            return 1.0
        # ... simplified recursive or fixed ones
        return 1.0


class BackendManager:
    """Manages backend dispatching and lazy loading."""

    _backends: dict[str, BackendOps] = {}
    _python_backend: BackendOps | None = None

    @classmethod
    def get_backend(cls, data_obj: Any) -> BackendOps:
        """Determines the appropriate backend for the given data object."""
        # Use module-based detection to avoid eager imports
        module = getattr(data_obj.__class__, "__module__", "")

        if isinstance(data_obj, (int, float, complex, list, tuple)):
            return cls._get_python_backend()

        cls_name = getattr(data_obj.__class__, "__name__", "").lower()

        # JAX Detection (Prioritize to catch Tracers which might mimic other things)
        if (
            module.startswith("jax")
            or module.startswith("jaxlib")
            or "jax" in module
            or "jax" in cls_name
            or "tracer" in cls_name
            or hasattr(data_obj, "aval")  # JAX Tracers often have 'aval'
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
