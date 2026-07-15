"""Unit-Aware Networks."""

from collections.abc import Sequence
from typing import Any

from physure.domain.measurement.quantity import Quantity
from physure.domain.measurement.units import (
    CompoundUnit,
    get_default_system,
)

# --- PyTorch Implementation ---
try:
    import torch
    import torch.nn as nn

    from physure.nn.layers import TorchPiLayer

    class TorchUnitAwareMLP(nn.Module):
        """A Unit-Aware Multilayer Perceptron.

        Pipeline:
            Inputs (Quantities) -> PiLayer (Dimensionless) -> MLP
            -> Rescaling -> Output (Quantities)
        """

        def __init__(
            self,
            in_units: Sequence[CompoundUnit | Quantity],
            out_units: Sequence[CompoundUnit | Quantity],
            hidden_dims: Sequence[int] = (64, 64),
            pi_out_features: int | None = None,
            system: Any = None,
        ):
            super().__init__()
            self.out_units = []
            sys = system or get_default_system()

            # Normalize out_units to CompoundUnit
            for u in out_units:
                if isinstance(u, Quantity):
                    self.out_units.append(u.unit)
                else:
                    self.out_units.append(u)

            self.system = sys

            # 1. Pi Layer
            if pi_out_features is None:
                # Heuristic: match input features or smaller
                pi_out_features = max(1, len(in_units))

            self.pi_layer = TorchPiLayer(in_units, pi_out_features, system=sys)

            # 2. Standard MLP
            layers = []
            last_dim = pi_out_features
            for h in hidden_dims:
                layers.append(nn.Linear(last_dim, h))
                layers.append(nn.Tanh())  # Physics usually likes Tanh or SiLU
                last_dim = h

            self.mlp = nn.Sequential(*layers)

            # 3. Output Head
            # We map last hidden layer to Number of Outputs
            self.head = nn.Linear(last_dim, len(self.out_units))

            # 4. Learnable Scales
            # We initialize scales to 1.0 (vector of size N_out)
            self.scale = nn.Parameter(torch.ones(len(self.out_units)))

        def forward(self, *inputs: Quantity):
            """Forward pass returning dimensional Quantities."""
            # 1. Normalization (Pi Layer)
            # Returns (Batch, Pi_Features) tensor
            pi_out = self.pi_layer(*inputs)

            # 2. Transformation (MLP)
            features = self.mlp(pi_out)
            raw_out = self.head(features)  # (Batch, N_out)

            # 3. Re-dimensionalization
            # Expand scale for batching: (1, N_out) * (Batch, N_out)
            scaled_out = raw_out * self.scale.unsqueeze(0)

            # 4. Wrap results
            results = []
            for i, unit in enumerate(self.out_units):
                # Extract column
                mag = scaled_out[:, i]
                # Create Quantity
                # Note: We must explicitly construct.
                # Assuming simple Construction from backend tensor
                q = Quantity.from_input(mag, unit, self.system)
                results.append(q)

            if len(results) == 1:
                return results[0]
            return tuple(results)

except ImportError:
    pass
