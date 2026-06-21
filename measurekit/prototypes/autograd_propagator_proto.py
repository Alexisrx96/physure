import os
import sys

# Ensure measurekit is in path
sys.path.append(os.getcwd())

import math

try:
    import torch
    import torch.func
except ImportError:
    torch = None

try:
    import jax
    import jax.numpy as jnp
except ImportError:
    jax = None


def prototype_torch():
    if torch is None:
        print("Skipping Torch prototype (not installed)")
        return

    print("\n--- Torch Autograd Prototype ---")

    # Dummy inputs
    val = torch.tensor([2.0], requires_grad=True)
    covariance = torch.tensor([[0.01]])

    def func_to_differentiate(x):
        return torch.sin(x)

    # Use torch.func.jacrev to get Jacobian
    # jacrev computes vjp equivalent for full jacobian
    jacobian_fn = torch.func.jacrev(func_to_differentiate)

    J = jacobian_fn(val)
    print(f"Jacobian (cos(2) ~ -0.416): {J}")

    # Propagate covariance: J @ Sigma @ J.T
    # If scalars/1D:
    # J shape: (output_dim, input_dim) -> (1, 1)
    new_cov = J @ covariance @ J.T
    print(f"New Covariance: {new_cov}")

    expected_var = (math.cos(2.0) ** 2) * 0.01
    print(f"Expected Variance (linear approx): {expected_var}")

    # Test with multi-input function
    # z = x * y
    # x = 2, y = 3
    val_x = torch.tensor([2.0])
    val_y = torch.tensor([3.0])

    # Input vector [x, y]
    inputs = torch.cat([val_x, val_y])
    cov_inputs = torch.diag(torch.tensor([0.01, 0.02]))  # uncorrelared

    def func_multi(inputs):
        x, y = inputs[0], inputs[1]
        return torch.stack([x * y, x + y])

    j_multi = torch.func.jacrev(func_multi)(inputs)
    print(f"Multi-dim Jacobian:\n{j_multi}")

    new_cov_multi = j_multi @ cov_inputs @ j_multi.T
    print(f"Propagated Covariance:\n{new_cov_multi}")


def prototype_jax():
    if jax is None:
        print("Skipping JAX prototype")
        return

    print("\n--- JAX Autograd Prototype ---")

    val = jnp.array([2.0])
    covariance = jnp.array([[0.01]])

    def func_to_diff(x):
        return jnp.sin(x)

    j_fn = jax.jacfwd(func_to_diff)
    J = j_fn(val)
    print(f"Jacobian: {J}")

    new_cov = J @ covariance @ J.T
    print(f"New Covariance: {new_cov}")


if __name__ == "__main__":
    prototype_torch()
    prototype_jax()
