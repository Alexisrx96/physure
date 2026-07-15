import pytest

try:
    import torch
except ImportError:
    torch = None

if torch is not None:
    from measurekit.backends.torch.autograd_store import (
        AutogradCovarianceStore,
    )
    from measurekit.backends.torch_backend import TorchBackend
    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.domain.measurement.units import get_default_system
    from measurekit.domain.measurement.vectorized_uncertainty import (
        CovarianceStore,
        _current_store,
    )


@pytest.mark.skipif(torch is None, reason="Torch not installed")
def test_autograd_propagation():
    # 1. Setup Context with Autograd Store
    backend = TorchBackend()

    # Manually construct the store facade but inject Autograd implementation
    # We create a dummy CovarianceStore then swap _core
    store_facade = CovarianceStore(backend=backend)

    # Swap out the Rust core for Python Autograd core
    autograd_core = AutogradCovarianceStore(backend)
    store_facade._core = autograd_core

    token = _current_store.set(store_facade)

    try:
        system = get_default_system()
        meter = system.get_unit("meter")

        # 2. Define Inputs with Gradients
        # We want to optimize the *input uncertainty* to minimize output uncertainty

        val_x = torch.tensor([10.0], dtype=torch.float64)
        # Initial uncertainty 0.1. We want gradients w.r.t this.
        unc_x = torch.tensor([0.1], dtype=torch.float64, requires_grad=True)

        val_y = torch.tensor([20.0], dtype=torch.float64)
        unc_y = torch.tensor([0.2], dtype=torch.float64, requires_grad=True)

        # 3. Create Quantities
        # Note: Quantity constructor registers the uncertainty in the store
        q_x = Quantity.from_input(val_x, meter, system, uncertainty=unc_x)
        print(f"DEBUG: q_x.uncertainty dtype={q_x.uncertainty.dtype}")
        q_y = Quantity.from_input(val_y, meter, system, uncertainty=unc_y)
        print(f"DEBUG: q_y.uncertainty dtype={q_y.uncertainty.dtype}")

        # 4. Perform Operation
        # z = 2*x + y
        q_z = 2.0 * q_x + q_y

        # 5. Access Output Uncertainty
        # This triggers `get_covariance_block` which should return a Tensor connected to unc_x, unc_y
        unc_z = q_z.uncertainty

        # Check values (Sanity)
        # Var(z) = 4*Var(x) + Var(y) = 4*(0.01) + 0.04 = 0.08
        # Std(z) = sqrt(0.08) ~ 0.2828
        print(f"DEBUG: unc_z value={unc_z.item()}")
        assert isinstance(unc_z, torch.Tensor)
        assert torch.isclose(
            unc_z, torch.tensor([0.2828427], dtype=torch.float64), atol=1e-5
        )

        # 6. Backpropagation
        # Minimize output uncertainty
        loss = unc_z.sum()
        loss.backward()

        # 7. Check Gradients
        # d(Std)/d(Std_x)?
        # Var = 4 * sx^2 + sy^2
        # Std = sqrt(Var)
        # dStd/dsx = (1/2Std) * dVar/dsx = (1/2Std) * 8 * sx = 4 * sx / Std
        #          = 4 * 0.1 / 0.2828 = 0.4 / 0.2828 ~ 1.414

        assert unc_x.grad is not None
        assert unc_y.grad is not None

        expected_grad_x = 4.0 * 0.1 / 0.2828427
        assert torch.isclose(
            unc_x.grad,
            torch.tensor([expected_grad_x], dtype=torch.float64),
            rtol=1e-3,
        )

        print(f"Success! Gradient X: {unc_x.grad}")

    finally:
        _current_store.reset(token)


if __name__ == "__main__":
    test_autograd_propagation()
