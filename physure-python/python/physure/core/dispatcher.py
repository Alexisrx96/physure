from __future__ import annotations

import importlib
import importlib.util
import os
from typing import TYPE_CHECKING, Any, ClassVar

if TYPE_CHECKING:
    from physure.core.protocols import BackendOps


def enforce_tensor_contract(func: Any) -> Any:
    """Decorator to enforce jaxtyping contracts at runtime."""
    if os.environ.get("MEASUREKIT_DEBUG") == "1":
        try:
            from beartype import beartype

            return beartype(func)
        except ImportError:
            return func
    return func


import functools  # noqa: E402


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

        if "physure._core.Quantity" in str(type(data_obj)):
            return cls._get_or_load_backend("core")

        cls_name = getattr(data_obj.__class__, "__name__", "").lower()

        # SciPy Detection (for sparse matrices)
        if (
            module.startswith("scipy")
            or cls_name == "csr_matrix"
            or cls_name == "csc_matrix"
            or cls_name == "coo_matrix"
        ) and importlib.util.find_spec("scipy.sparse") is not None:
            return cls._get_or_load_backend("numpy")
        # If scipy is not installed (or class name didn't match),
        # fall through to python backend

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
                    "physure.backends.numpy_backend"
                )
                cls._backends[name] = module.NumpyBackend()
            elif name == "torch":
                module = importlib.import_module(
                    "physure.backends.torch_backend"
                )
                cls._backends[name] = module.TorchBackend()
            elif name == "jax":
                module = importlib.import_module(
                    "physure.backends.jax_backend"
                )
                cls._backends[name] = module.JaxBackend()
                # Trigger JAX Pytree registration
                if hasattr(module, "register_jax_behavior"):
                    module.register_jax_behavior()
            elif name == "core":
                module = importlib.import_module(
                    "physure.backends.core_backend"
                )
                cls._backends[name] = module.CoreBackend()
            else:
                raise ValueError(f"Backend '{name}' not supported yet.")
        return cls._backends[name]

    @classmethod
    def _get_python_backend(cls) -> BackendOps:
        if cls._python_backend is None:
            module = importlib.import_module(
                "physure.backends.python_backend"
            )
            cls._python_backend = module.PythonBackend()
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
