"""A backend announces itself when it loads; the model subscribes to be told.

The model registers its own torch and jax pytrees. A backend module used to
import `Quantity` to do it, which put the model underneath the backends.
"""

import pytest

from physure.core.dispatcher import BackendManager


@pytest.fixture
def clean_hooks(monkeypatch):
    monkeypatch.setattr(BackendManager, "_load_hooks", {})
    monkeypatch.setattr(BackendManager, "_announced", set())


def test_a_hook_runs_when_its_backend_announces(clean_hooks):
    calls = []
    BackendManager.on_load("demo", lambda: calls.append("demo"))
    assert calls == []
    BackendManager.announce("demo")
    assert calls == ["demo"]


def test_a_hook_registered_after_the_announcement_runs_at_once(clean_hooks):
    BackendManager.announce("demo")
    calls = []
    BackendManager.on_load("demo", lambda: calls.append("late"))
    assert calls == ["late"]


def test_a_hook_is_registered_once(clean_hooks):
    calls = []

    def hook():
        calls.append(1)

    BackendManager.on_load("demo", hook)
    BackendManager.on_load("demo", hook)
    BackendManager.announce("demo")
    assert calls == [1]


def test_a_hook_only_hears_its_own_backend(clean_hooks):
    calls = []
    BackendManager.on_load("demo", lambda: calls.append("demo"))
    BackendManager.announce("other")
    assert calls == []


def test_clearing_the_backends_keeps_the_registrations(clean_hooks):
    calls = []
    BackendManager.on_load("demo", lambda: calls.append(1))
    BackendManager.clear_backends()
    BackendManager.announce("demo")
    assert calls == [1]


def test_torch_flattens_a_quantity():
    torch = pytest.importorskip("torch")
    from torch.utils import _pytree

    from physure import Q_

    q = Q_(torch.tensor([1.0, 2.0], dtype=torch.float64), "m")
    leaves, spec = _pytree.tree_flatten(q)
    rebuilt = _pytree.tree_unflatten(leaves, spec)
    assert type(rebuilt) is type(q)
    assert rebuilt.unit == q.unit
    assert torch.equal(rebuilt.magnitude, q.magnitude)


def test_jax_flattens_a_quantity_and_the_uncertainty_models():
    jax = pytest.importorskip("jax")
    jnp = pytest.importorskip("jax.numpy")
    from physure import Q_
    from physure.backends.jax_backend import register_jax_behavior
    from physure.domain.measurement.uncertainty import (
        CovarianceModel,
        VarianceModel,
    )

    register_jax_behavior()
    q = Q_(jnp.array([1.0, 2.0]), "m")
    leaves, spec = jax.tree_util.tree_flatten(q)
    rebuilt = jax.tree_util.tree_unflatten(spec, leaves)
    assert type(rebuilt) is type(q)
    assert rebuilt.unit == q.unit

    variance = VarianceModel(variance=jnp.array([0.04, 0.09]))
    v_leaves, v_spec = jax.tree_util.tree_flatten(variance)
    v_rebuilt = jax.tree_util.tree_unflatten(v_spec, v_leaves)
    assert isinstance(v_rebuilt, VarianceModel)

    covariance = CovarianceModel(
        std_dev_internal=jnp.array([0.2, 0.3]), lineage={}
    )
    c_leaves, c_spec = jax.tree_util.tree_flatten(covariance)
    c_rebuilt = jax.tree_util.tree_unflatten(c_spec, c_leaves)
    assert isinstance(c_rebuilt, CovarianceModel)
