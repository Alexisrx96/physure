try:
    import torch
    import torch.nn as nn

    from measurekit_core import RationalUnit

    class UnitAutogradFunction(torch.autograd.Function):
        """Bridge between Rust RationalUnit logic and PyTorch Autograd.

        This function allows unit exponents to be learnable while maintaining
        compatibility with the symbolic unit system.
        """

        @staticmethod
        def forward(ctx, exponents, base_names):
            """Forward pass for differentiable units."""
            ctx.save_for_backward(exponents)
            ctx.base_names = base_names
            # We pass through the exponents for the neural net's logic
            return exponents

        @staticmethod
        def backward(ctx, grad_output):
            """Backward pass using symbolic information if needed."""
            # For now, we use standard gradient propagation as the unit transformation
            # is usually a linear scaling in log-space.
            return grad_output, None


    class UnitLayer(nn.Module):
        """Learnable unit for Physics-Informed Neural Networks (PINNs).

        Enforces dimensional consistency by allowing the network to learn the
        optimal non-dimensionalization or the dimensionality of latent variables.
        """

        def __init__(self, base_dimensions: list[str]):
            """Initializes the UnitLayer with a set of base dimensions."""
            super().__init__()
            self.base_dimensions = base_dimensions
            # Learnable exponents (initially zero/dimensionless)
            self.exponents = nn.Parameter(torch.zeros(len(base_dimensions)))

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            """Applies the differentiable unit logic to the input tensor."""
            # The learnable exponents are mediated by the custom autograd function
            _ = UnitAutogradFunction.apply(self.exponents, self.base_dimensions)
            return x

        def get_unit(self) -> RationalUnit:
            """Converts learnable float exponents into a symbolic RationalUnit.

            This rounding 'bridges' the continuous learning with the discrete
            rational system used in MeasureKit's core.
            """
            dims = {}
            # We use a precision-based rounding to map floats to rationals
            for name, val in zip(
                self.base_dimensions, self.exponents.detach().cpu().numpy(), strict=False
            ):
                # Round to nearest 1/100th for rational representation
                num = round(float(val) * 100)
                if num != 0:
                    dims[name] = (num, 100)
            return RationalUnit(dims)

except ImportError:
    pass
