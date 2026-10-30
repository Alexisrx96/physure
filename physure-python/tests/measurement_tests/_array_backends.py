"""The array backends the uncertainty tests run on, and how to put a numpy array into each of them."""

import contextlib
import importlib.util

#: numpy always; torch and JAX when they are installed
AVAILABLE = ["numpy"] + [
    name for name in ("torch", "jax") if importlib.util.find_spec(name)
]


def wrapper(lib):
    """A function that turns a numpy array into the array type of `lib`."""
    if lib == "torch":
        import torch

        return torch.as_tensor
    if lib == "jax":
        import jax.numpy as jnp

        return jnp.asarray
    return lambda x: x


@contextlib.contextmanager
def x64_for(lib):
    """JAX computes in float32 unless told otherwise; the GUM comparisons are float64."""
    if lib != "jax":
        yield
        return
    import jax

    # `jax.enable_x64` in current JAX, `jax.experimental.enable_x64` before it moved
    enable = getattr(jax, "enable_x64", None)
    if enable is None:
        from jax.experimental import enable_x64 as enable
    with enable(True):
        yield
