"""The torch backend's element-wise Jacobians are sparse: N entries, not N x N."""

import pytest

torch = pytest.importorskip("torch")

from physure import Q_
from physure.backends.torch_backend import TorchBackend


def test_diagonal_and_identity_operators_are_sparse_and_equal_the_dense_ones():
    backend = TorchBackend()
    values = torch.rand(7, dtype=torch.float64) + 1.0
    diagonal = backend.diagonal_operator(values)
    assert diagonal.is_sparse
    assert torch.equal(diagonal.to_dense(), torch.diag(values))
    identity = backend.identity_operator(7, reference=values)
    assert identity.is_sparse
    assert torch.equal(identity.to_dense(), torch.eye(7, dtype=torch.float64))


def test_a_million_element_operator_needs_no_n_squared_memory():
    # dense, this is 4 TB and raises; sparse, it is a million entries
    backend = TorchBackend()
    assert backend.identity_operator(
        1_000_000, reference=torch.zeros(1)
    ).is_sparse
    assert backend.diagonal_operator(torch.ones(1_000_000)).is_sparse


def test_exact_quantity_arithmetic_on_tensors_matches_plain_tensors_and_keeps_gradients():
    a = (torch.rand(40, dtype=torch.float64) + 1.0).requires_grad_(True)
    b = (torch.rand(40, dtype=torch.float64) + 1.0).requires_grad_(True)
    product, quotient = Q_(a, "m") * Q_(b, "s"), Q_(a, "m") / Q_(b, "s")
    assert torch.allclose(product.magnitude, a * b)
    assert torch.allclose(quotient.magnitude, a / b)
    (product.magnitude.sum() + quotient.magnitude.sum()).backward()
    expected_a = (b + 1.0 / b).detach()
    expected_b = (a - a / b**2).detach()
    assert torch.allclose(a.grad, expected_a)
    assert torch.allclose(b.grad, expected_b)


def test_operators_stay_dense_while_torch_compile_traces(monkeypatch):
    # Dynamo cannot trace sparse tensors (it would add graph breaks, which
    # tests/ext/test_graph_breaks.py holds down), so tracing keeps the dense operator.
    monkeypatch.setattr(torch.compiler, "is_compiling", lambda: True)
    backend = TorchBackend()
    values = torch.rand(5, dtype=torch.float64) + 1.0
    assert not backend.diagonal_operator(values).is_sparse
    assert not backend.identity_operator(5, reference=values).is_sparse
