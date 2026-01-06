"""Physics-Aware Layers for MeasureKit."""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any

from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.nn.utils import extract_dimension_matrix, null_space_basis

# --- PyTorch Implementation ---
try:
    import torch
    import torch.nn as nn
    from torch import Tensor

    class TorchPiLayer(nn.Module):
        """A physics-aware layer that learns dimensionless groups.

        This layer enforces the Buckingham Pi Theorem by constraining weights
        to be in the null space of the dimensional matrix D.

        Args:
            units: A list of Units or Quantities defining the input features.
            out_features: Number of dimensionless groups to discover.
            system: UnitSystem to resolve dimensions.
        """

        def __init__(
            self,
            units: Sequence[CompoundUnit | Quantity],
            out_features: int,
            system: Any = None,
        ):
            super().__init__()
            self.in_features = len(units)
            self.out_features = out_features

            # 1. Compute Dimensional Matrix D (N_dims x N_in)
            D, _ = extract_dimension_matrix(units, system)

            # Convert to torch tensor for null space computation
            D_tensor = torch.as_tensor(D, dtype=torch.float32)

            # 2. Compute Null Space Basis V (N_in x N_null)
            # V columns form the basis.
            V = null_space_basis(D_tensor)

            if V.shape[1] == 0:
                # No null space means no dimensionless group can be formed!
                # This usually implies inputs are dimensionally independent.
                # Use a dummy parameter or raise warning?
                # We'll allow it but output will be constant 1.0 (log=0).
                pass

            # Register V as a buffer (not a scalable parameter)
            self.register_buffer("V_null", V)

            # 3. Learnable Parameters Theta (N_null x N_out)
            # We map from the null space coordinates to the outputs.
            n_null = V.shape[1]
            self.theta = nn.Parameter(torch.randn(n_null, out_features) * 0.1)

        def forward(self, *inputs: Quantity | Tensor) -> Tensor:
            """Forward pass.

            Args:
                *inputs: Sequence of input quantities or tensors.
                         Must match the order and length of 'units' provided in __init__.

            Returns:
                A Tensor of shape (Batch, OutFeatures) representing the dimensionless groups.
                (Values are dimensionless, so we return raw Tensor, not Quantity, for efficiency).
            """
            # 1. Stack Magnitudes
            # We expect inputs to be Quantities or Tensors.
            mags = []
            for x in inputs:
                if isinstance(x, Quantity):
                    mags.append(x.magnitude)
                else:
                    mags.append(x)

            # Stack along last dim -> (Batch, N_in)
            # If inputs are 1D (Batch,), reshape to (Batch, 1) first?
            # Assuming standard (Batch, 1) or (Batch,)
            processed_mags = []
            for m in mags:
                if isinstance(m, torch.Tensor) and m.ndim == 1:
                    processed_mags.append(m.unsqueeze(-1))
                elif isinstance(m, (int, float)):
                    processed_mags.append(
                        torch.tensor([m], device=self.V_null.device).unsqueeze(
                            -1
                        )
                    )
                else:
                    processed_mags.append(m)

            X = torch.cat(processed_mags, dim=-1)

            # 2. Log Transform
            # Use abs to handle negative inputs safely, assuming physical magnitudes
            # Add epsilon for numerical stability
            log_X = torch.log(torch.abs(X) + 1e-9)

            # 3. Constrained Linear Layer
            # W = V * theta
            # W shape: (N_in, N_null) @ (N_null, N_out) -> (N_in, N_out)
            W = self.V_null @ self.theta

            # Y = log_X @ W
            Y = log_X @ W

            # 4. Exponential
            out = torch.exp(Y)

            return out

except ImportError:
    pass


# --- JAX / Equinox Implementation ---
try:
    import equinox as eqx
    import jax
    import jax.numpy as jnp

    class JaxPiLayer(eqx.Module):
        """JAX/Equinox implementation of the PiLayer."""

        V_null: jnp.ndarray
        theta: jnp.ndarray
        in_features: int = eqx.field(static=True)
        out_features: int = eqx.field(static=True)

        def __init__(
            self,
            units: Sequence[CompoundUnit | Quantity],
            out_features: int,
            key: jax.random.PRNGKey,
            system: Any = None,
        ):
            self.in_features = len(units)
            self.out_features = out_features

            # 1. Compute Dimensional Matrix D
            D, _ = extract_dimension_matrix(units, system)
            D_array = jnp.array(D)

            # 2. Compute Null Space Basis
            self.V_null = null_space_basis(D_array)

            # 3. Initialize Theta
            n_null = self.V_null.shape[1]
            self.theta = jax.random.normal(key, (n_null, out_features)) * 0.1

        def __call__(self, *inputs: Quantity | Any) -> jnp.ndarray:
            """Forward pass."""
            # 1. Stack Magnitudes
            mags = []
            for x in inputs:
                if isinstance(x, Quantity):
                    mags.append(x.magnitude)
                else:
                    mags.append(x)

            # Handle list of scalars or arrays
            # We use jnp.stack or similar.
            # Assuming inputs are compatible arrays.
            # We need to ensure they are at least 2D (Batch, 1) or handle scalar inputs.

            processed_mags = []
            for m in mags:
                m_arr = jnp.asarray(m)
                if m_arr.ndim == 1:
                    m_arr = m_arr[:, None]  # Add feature dim
                processed_mags.append(m_arr)

            X = jnp.concatenate(processed_mags, axis=-1)

            # 2. Log Transform
            log_X = jnp.log(jnp.abs(X) + 1e-9)

            # 3. Constrained Weights
            W = self.V_null @ self.theta

            # 4. Compute
            Y = log_X @ W
            return jnp.exp(Y)

except ImportError:
    pass
