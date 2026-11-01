"""Broadcast shapes through the differentiable torch covariance store (`python_lineage()`).

The Rust store takes numpy values; with torch tensors that carry an autograd graph the uncertainty has to be
computed, and differentiated, by the Python lineage store. The reference is the GUM formula written in plain torch:
the uncertainty must equal it, and so must its gradient with respect to the input uncertainties.
"""

import numpy as np
import pytest

torch = pytest.importorskip("torch")

import physure
from physure import Q_
from physure.backends.torch.autograd_store import AutogradCovarianceStore
from physure.backends.torch_backend import TorchBackend
from physure.domain.measurement.vectorized_uncertainty import (
    CovarianceStore,
    _current_store,
)

SHAPES = [
    ((5,), (5,)),
    ((2, 3), (2, 3)),
    ((4, 5), (5,)),
    ((5,), (4, 5)),
    ((4, 1), (1, 5)),
    ((3,), (1,)),
]

FORMULAS = {
    "add": lambda a, b, ua, ub: torch.sqrt(ua**2 + ub**2),
    "sub": lambda a, b, ua, ub: torch.sqrt(ua**2 + ub**2),
    "mul": lambda a, b, ua, ub: torch.sqrt((b * ua) ** 2 + (a * ub) ** 2),
    "div": lambda a, b, ua, ub: torch.sqrt(
        (ua / b) ** 2 + (a * ub / b**2) ** 2
    ),
}


@pytest.fixture
def autograd_store():
    backend = TorchBackend()
    facade = CovarianceStore(backend=backend)
    facade._core = AutogradCovarianceStore(backend)
    token = _current_store.set(facade)
    try:
        yield facade
    finally:
        _current_store.reset(token)


@pytest.mark.parametrize("op", list(FORMULAS))
@pytest.mark.parametrize("shapes", SHAPES, ids=str)
def test_value_and_gradient_follow_the_gum(autograd_store, shapes, op):
    rng = np.random.default_rng(7)
    a = torch.as_tensor(rng.random(shapes[0]) + 1.0)
    b = torch.as_tensor(rng.random(shapes[1]) + 1.0)
    ua = (0.05 * a).clone().requires_grad_(True)
    ub = (0.03 * b).clone().requires_grad_(True)
    unit_b = "m" if op in ("add", "sub") else "s"

    with physure.python_lineage():
        qa, qb = Q_(a, "m", ua), Q_(b, unit_b, ub)
        result = {
            "add": lambda: qa + qb,
            "sub": lambda: qa - qb,
            "mul": lambda: qa * qb,
            "div": lambda: qa / qb,
        }[op]()
        got = result.uncertainty

    ua_ref = ua.detach().clone().requires_grad_(True)
    ub_ref = ub.detach().clone().requires_grad_(True)
    expected = FORMULAS[op](*torch.broadcast_tensors(a, b, ua_ref, ub_ref))
    expected.sum().backward()

    assert torch.allclose(
        got.reshape(expected.shape).detach(), expected.detach(), rtol=1e-6
    )
    got.sum().backward()
    assert torch.allclose(ua.grad, ua_ref.grad, rtol=1e-6)
    assert torch.allclose(ub.grad, ub_ref.grad, rtol=1e-6)
