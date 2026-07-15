import pytest

# Skip if dependencies are missing
try:
    import torch
    import torch.nn as nn

    HAS_TORCH = True
except ImportError:
    HAS_TORCH = False


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_torch_null_space_projection():
    from physure.nn.utils import null_space_basis

    # 1. Define a matrix D (Rank 1)
    # x1 + x2 = 0 in null space?
    # D = [[1, 1], [2, 2]] -> Rank 1.
    # Null space is line x1 = -x2. v = [1, -1] normalized.
    d_matrix = torch.tensor([[1.0, 1.0], [2.0, 2.0]])
    null_basis = null_space_basis(d_matrix)

    # Check shape: (2, 1) or (2, 0) if calculation was fragile (fixed now)
    assert null_basis.shape == (2, 1)

    # Check orthogonality: D @ V ~ 0
    res = d_matrix @ null_basis
    assert torch.allclose(res, torch.zeros_like(res), atol=1e-6)

    # Check V is normalized
    norm = torch.linalg.norm(null_basis)
    assert torch.allclose(norm, torch.tensor(1.0), atol=1e-6)


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_pinn_friction_factor_learning():
    """Validates that UnitAwareMLP can learn a dimensionless physical law efficiently."""
    from physure.application.startup import create_system
    from physure.domain.measurement.units import (
        CompoundUnit,
        get_default_system,
        units,
    )
    from physure.nn.networks import TorchUnitAwareMLP

    # Ensure system is loaded
    try:
        sys = get_default_system()
    except Exception:
        # Fallback for testing environment
        sys = create_system("si.conf")

    # 1. Physics Ground Truth (Blasius Correlation)
    # f = 0.316 * Re^(-0.25)
    # Re = rho * v * D / mu

    num_samples = 200
    # Create data with significant variance to challenge unscaled networks
    rho = torch.rand(num_samples) * 10 + 990  # ~1000
    v = torch.rand(num_samples) * 10 + 0.1  # 0.1 - 10
    d_pipe = torch.rand(num_samples) * 0.1 + 0.01  # 0.01 - 0.1
    mu = torch.rand(num_samples) * 0.001 + 0.0008  # Small

    re_num = (rho * v * d_pipe) / mu
    f_true = 0.316 * (re_num**-0.25)

    target = f_true  # (N,)

    # 2. Setup Unit-Aware Model
    # Inputs: rho [M L-3], v [L T-1], D [L], mu [M L-1 T-1]
    input_units = [
        units.kg / units.m**3,  # rho
        units.m / units.s,  # v
        units.m,  # D
        units.kg / (units.m * units.s),  # mu
    ]

    # Output: Dimensionless
    out_units = [CompoundUnit({})]

    model = TorchUnitAwareMLP(
        input_units,
        out_units,
        hidden_dims=[32, 32],
        pi_out_features=1,  # We expect 1 dimensionless group (Re)
        system=sys,
    )

    optim = torch.optim.Adam(model.parameters(), lr=0.01, weight_decay=0.0)

    # 3. Train
    model.train()
    initial_loss = 0.0
    final_loss = 0.0

    inputs = [rho, v, d_pipe, mu]

    for epoch in range(150):
        optim.zero_grad()

        # Pass raw tensors (supported by layer)
        # Output is Quantity
        out_q = model(*inputs)

        # Loss on magnitude
        pred = out_q.magnitude
        loss = nn.MSELoss()(pred, target)
        loss.backward()
        optim.step()

        if epoch == 0:
            initial_loss = loss.item()
        final_loss = loss.item()

    print(f"Initial Loss: {initial_loss:.4f}, Final Loss: {final_loss:.6f}")

    # 4. Assert Convergence
    # Should be extremely low for exact physics
    assert final_loss < 0.005

    # 5. Check Generalization (Extrapolation)
    # Test on data outside range
    v_test = torch.tensor([50.0])  # High velocity
    rho_test = torch.tensor([1000.0])
    d_test = torch.tensor([0.05])
    mu_test = torch.tensor([0.001])

    re_test = (rho_test * v_test * d_test) / mu_test
    expected = 0.316 * (re_test**-0.25)

    model.eval()
    with torch.no_grad():
        out_test_q = model(rho_test, v_test, d_test, mu_test)
        pred_test = out_test_q.magnitude

    err = torch.abs(pred_test - expected)
    print(f"Extrapolation Error: {err.item():.6f}")
    assert (
        err.item() < 0.1
    )  # Should generalize reasonably well due to physics structure


@pytest.mark.skipif(not HAS_TORCH, reason="PyTorch not installed")
def test_dimensional_homogeneity_loss():
    from physure.nn.loss import dimensional_homogeneity_loss

    # We mock soft Dimension, Unit, and Quantity objects with PyTorch parameters
    class SoftDimension:
        def __init__(self, exponents):
            self.exponents = exponents

    class SoftUnit:
        def __init__(self, dimension_obj):
            self.dimension_obj = dimension_obj

        def dimension(self, system=None):
            return self.dimension_obj

    class SoftQuantity:
        def __init__(self, unit, system=None):
            self.unit = unit
            self.system = system

    # Define soft exponents as PyTorch parameters
    # term 1: [L^a]
    # term 2: [L^b]
    # We want a and b to converge to the same value (variance = 0)
    a = torch.tensor(1.0, requires_grad=True)
    b = torch.tensor(2.0, requires_grad=True)

    dim1 = SoftDimension({"L": a})
    dim2 = SoftDimension({"L": b})

    q1 = SoftQuantity(SoftUnit(dim1))
    q2 = SoftQuantity(SoftUnit(dim2))

    # Compute loss
    loss = dimensional_homogeneity_loss([q1, q2])

    # Check loss value (variance between [1.0] and [2.0] is 0.25)
    import math

    assert math.isclose(loss.item(), 0.25, rel_tol=1e-5)

    # Perform backward pass to verify differentiability
    loss.backward()

    # Check gradients
    assert a.grad is not None
    assert b.grad is not None
    # dLoss/da at a=1.0, b=2.0 (mean=1.5, loss = 0.5 * ((a - 1.5)^2 + (b - 1.5)^2) = 0.25 * (a - b)^2)
    # dLoss/da = 0.5 * (a - b) = -0.5
    # dLoss/db = 0.5 * (b - a) = 0.5
    assert math.isclose(a.grad.item(), -0.5, rel_tol=1e-5)
    assert math.isclose(b.grad.item(), 0.5, rel_tol=1e-5)
