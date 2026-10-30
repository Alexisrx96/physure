"""Uncertain torch arrays propagate by the law of propagation of uncertainty (JCGM 100:2008, GUM 5.1).

The expected values are written out from the GUM formulas for uncorrelated inputs, not taken from the code under
test, and the numpy backend (the Rust covariance store) is checked against the same numbers: torch has to agree
with both. A quantity combined with itself is fully correlated, which is what ``a - a == 0`` and ``a * a == a ** 2``
say.
"""

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from physure import Q_  # noqa: E402

SHAPES = [(4,), (2, 3)]
BACKENDS = ["numpy", "torch"]


def inputs(shape, lib, dtype=np.float64):
    rng = np.random.default_rng(3)
    a, b = rng.random(shape) + 1.0, rng.random(shape) + 1.0
    ua, ub = 0.05 * a, 0.03 * b
    a, b, ua, ub = (x.astype(dtype) for x in (a, b, ua, ub))
    wrap = torch.as_tensor if lib == "torch" else (lambda x: x)
    return (a, b, ua, ub), (Q_(wrap(a), "m", wrap(ua)), Q_(wrap(b), "m", wrap(ub)), Q_(wrap(b), "s", wrap(ub)))


def as_numpy(x):
    return x.detach().numpy() if hasattr(x, "detach") else np.asarray(x)


# name -> (operation on the quantities, expected standard uncertainty from the GUM, with a, b, ua, ub)
CASES = {
    "a + b": (lambda a, b, s: a + b, lambda a, b, ua, ub: np.sqrt(ua**2 + ub**2)),
    "a - b": (lambda a, b, s: a - b, lambda a, b, ua, ub: np.sqrt(ua**2 + ub**2)),
    "a * 2.0": (lambda a, b, s: a * 2.0, lambda a, b, ua, ub: 2.0 * ua),
    "a / 2.0": (lambda a, b, s: a / 2.0, lambda a, b, ua, ub: ua / 2.0),
    "a * s": (lambda a, b, s: a * s, lambda a, b, ua, ub: np.sqrt((b * ua) ** 2 + (a * ub) ** 2)),
    "a / s": (lambda a, b, s: a / s, lambda a, b, ua, ub: np.sqrt((ua / b) ** 2 + (a * ub / b**2) ** 2)),
    "a ** 2": (lambda a, b, s: a**2, lambda a, b, ua, ub: 2.0 * a * ua),
    "a - a": (lambda a, b, s: a - a, lambda a, b, ua, ub: np.zeros_like(a)),
    "a * a": (lambda a, b, s: a * a, lambda a, b, ua, ub: 2.0 * a * ua),
    "a / a": (lambda a, b, s: a / a, lambda a, b, ua, ub: np.zeros_like(a)),
    "(a + b) * a": (lambda a, b, s: (a + b) * a, lambda a, b, ua, ub: np.sqrt(((2 * a + b) * ua) ** 2 + (a * ub) ** 2)),
}


@pytest.mark.parametrize("lib", BACKENDS)
@pytest.mark.parametrize("shape", SHAPES, ids=str)
@pytest.mark.parametrize("name", list(CASES))
def test_standard_uncertainty_follows_the_gum(name, shape, lib):
    operation, expected = CASES[name]
    (a, b, ua, ub), quantities = inputs(shape, lib)
    result = operation(*quantities)
    got = as_numpy(result.uncertainty).reshape(shape)
    np.testing.assert_allclose(got, expected(a, b, ua, ub), rtol=1e-9, atol=1e-12)


@pytest.mark.parametrize("name", ["a + b", "a * s", "a / s", "a * a"])
def test_float32_torch_agrees_with_the_gum_to_single_precision(name):
    operation, expected = CASES[name]
    (a, b, ua, ub), quantities = inputs((5,), "torch", dtype=np.float32)
    got = as_numpy(operation(*quantities).uncertainty)
    np.testing.assert_allclose(got, expected(*(x.astype(np.float64) for x in (a, b, ua, ub))), rtol=2e-5, atol=1e-7)


def test_the_magnitude_keeps_its_gradient_when_the_inputs_carry_an_uncertainty():
    a = (torch.rand(6, dtype=torch.float64) + 1.0).requires_grad_(True)
    b = (torch.rand(6, dtype=torch.float64) + 1.0).requires_grad_(True)
    product = Q_(a, "m", 0.05 * a.detach()) * Q_(b, "s", 0.03 * b.detach())
    product.magnitude.sum().backward()
    np.testing.assert_allclose(a.grad.numpy(), b.detach().numpy())
    np.testing.assert_allclose(b.grad.numpy(), a.detach().numpy())
