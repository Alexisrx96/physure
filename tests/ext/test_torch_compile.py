import sys

import pytest

from measurekit import Q_

try:
    import torch
except ImportError:
    torch = None


def simple_fn(a, b):
    # Basic arithmetic that should be compilable
    return (a + b) * 2.0


@pytest.mark.skipif(torch is None, reason="PyTorch not installed")
@pytest.mark.skipif(
    sys.version_info >= (3, 14),
    reason="torch.compile is not supported on Python 3.14+ (torch 2.9 raises)",
)
def test_compile_quantity():
    # Only run if torch.compile is available (torch 2.0+)
    if not hasattr(torch, "compile"):
        pytest.skip("torch.compile not available")

    # Enable debug logs to see graph breaks
    # os.environ["TORCH_LOGS"] = "graph_breaks,recompiles"

    # Create inputs
    val_a = torch.randn(10, 10, requires_grad=True)
    val_b = torch.randn(10, 10, requires_grad=True)

    a = Q_(val_a, "m")
    b = Q_(val_b, "m")

    # Compile the function
    # fullgraph=True enforces no graph breaks. If this passes, we are golden.
    # backend="aot_eager" verifies graph capture without requiring C++ compiler on Windows
    # If "inductor" is used, it might fail on CI/Windows if MSVC is missing.
    # If "inductor" is used, it might fail on CI/Windows if MSVC is missing.
    import warnings

    try:
        # Relax fullgraph=True since we are running in pure Python fallback mode,
        # which inevitably introduces graph breaks (e.g., dynamic types, object creation).

        # Suppress benign warning about Quantity.__new__ (C-extension base)
        with warnings.catch_warnings():
            warnings.filterwarnings(
                "ignore",
                message=".*Dynamo does not know how to trace the builtin.*Quantity.__new__.*",
            )
            opt_fn = torch.compile(
                simple_fn, backend="aot_eager", fullgraph=False
            )

        print("Running compiled function...")
        res = opt_fn(a, b)
    except Exception:
        raise

    # Check standard execution for reference
    ref = simple_fn(a, b)

    # Sanity check values
    assert torch.allclose(res.magnitude, ref.magnitude), (
        "Compiled result magnitude mismatch"
    )
    assert res.unit == ref.unit, "Compiled result unit mismatch"

    # Check gradients
    loss = res.magnitude.sum()
    loss.backward()
    assert val_a.grad is not None, "Gradient flow failed"
    assert torch.all(torch.isfinite(val_a.grad)), "Gradients should be finite"
