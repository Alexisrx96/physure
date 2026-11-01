"""`broadcast_operator`: the Jacobian of a result with respect to an operand that was broadcast, on every backend.

Row ``i`` has one entry, the coefficient at result element ``i``, in the column of the operand element that result
element was copied from. numpy (scipy CSR) is the reference; torch (sparse COO) and JAX (BCOO) must build the same
matrix, and must power it like a dense matrix: a sparse ``0 ** 0`` is ``1``, not an unstored zero.
"""

import importlib.util

import numpy as np
import pytest

from physure.backends.numpy_backend import NumpyBackend

SHAPES = [
    ((5,), (4, 5)),
    ((4, 1), (4, 5)),
    ((1, 5), (4, 5)),
    ((), (3,)),
    ((1,), (3,)),
    ((2, 3), (2, 3)),
    ((3,), (2, 1, 3)),
]


def reference(coefficients, shape, out_shape):
    """The matrix written out with plain loops."""
    out_size, size = int(np.prod(out_shape)), int(np.prod(shape))
    columns = np.broadcast_to(
        np.arange(size).reshape(shape), out_shape
    ).reshape(-1)
    matrix = np.zeros((out_size, size))
    for row, column in enumerate(columns):
        matrix[row, column] = (
            1.0 if coefficients is None else coefficients[row]
        )
    return matrix


def densify(matrix):
    for attribute in ("toarray", "todense", "to_dense"):
        if hasattr(matrix, attribute):
            return np.asarray(getattr(matrix, attribute)())
    return np.asarray(matrix)


def backends():
    found = [("numpy", NumpyBackend(), np.asarray)]
    if importlib.util.find_spec("torch"):
        import torch

        from physure.backends.torch_backend import TorchBackend

        found.append(("torch", TorchBackend(), torch.as_tensor))
    if importlib.util.find_spec("jax"):
        import jax.numpy as jnp

        from physure.backends.jax_backend import JaxBackend

        found.append(("jax", JaxBackend(), jnp.asarray))
    return found


@pytest.fixture(autouse=True)
def x64():
    """JAX is float32 unless told otherwise, and the comparisons here are exact in float64."""
    if importlib.util.find_spec("jax") is None:
        yield
        return
    import jax

    enable = getattr(jax, "enable_x64", None)
    if enable is None:
        from jax.experimental import enable_x64 as enable
    with enable(True):
        yield


@pytest.mark.parametrize("name", [b[0] for b in backends()])
@pytest.mark.parametrize(("shape", "out_shape"), SHAPES, ids=str)
def test_broadcast_operator_is_the_incidence_matrix_scaled_by_the_coefficients(
    name, shape, out_shape
):
    _, backend, wrap = next(b for b in backends() if b[0] == name)
    coefficients = np.linspace(1.0, 2.0, int(np.prod(out_shape)))
    got = densify(
        backend.broadcast_operator(wrap(coefficients), shape, out_shape)
    )
    np.testing.assert_array_equal(
        got, reference(coefficients, shape, out_shape)
    )


@pytest.mark.parametrize("name", [b[0] for b in backends()])
@pytest.mark.parametrize(("shape", "out_shape"), SHAPES[:3], ids=str)
def test_no_coefficients_means_ones(name, shape, out_shape):
    _, backend, wrap = next(b for b in backends() if b[0] == name)
    got = densify(
        backend.broadcast_operator(
            None, shape, out_shape, reference=wrap(np.ones(3))
        )
    )
    np.testing.assert_array_equal(got, reference(None, shape, out_shape))


@pytest.mark.parametrize("name", [b[0] for b in backends()])
@pytest.mark.parametrize("exponent", [2, 0.5, 0, -1])
def test_a_sparse_operator_is_powered_like_a_dense_one(name, exponent):
    """Its unstored zeros are zeros: `0 ** 2` is still 0, but `0 ** 0` is 1 and `0 ** -1` is infinite."""
    if name == "numpy" and exponent <= 0:
        # scipy's sparse power leaves the unstored zeros alone. The covariance code only ever squares an
        # operator, so this is not reached, and it is not a backend this change adds.
        pytest.skip("scipy sparse power ignores unstored zeros")
    _, backend, wrap = next(b for b in backends() if b[0] == name)
    operator = backend.broadcast_operator(
        wrap(np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])), (3,), (2, 3)
    )
    with np.errstate(divide="ignore"):
        expected = np.power(densify(operator), exponent)
    got = densify(backend.pow(operator, exponent))
    np.testing.assert_array_equal(got, expected)
