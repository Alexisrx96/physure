import time

import torch

from physure import Quantity
from physure.infrastructure.config import units


def benchmark_comparison():
    print("Running Zero-Overhead Benchmark...")

    # Setup
    N = 1000000
    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"Device: {device}")

    val1 = torch.randn(N, device=device)
    val2 = torch.randn(N, device=device)

    # 1. Pure PyTorch Baseline
    start = time.perf_counter()
    for _ in range(100):
        res = val1 + val2
        torch.cuda.synchronize() if device == "cuda" else None
    end = time.perf_counter()
    torch_time = (end - start) / 100
    print(f"Pure PyTorch Time: {torch_time * 1000:.4f} ms")

    # 2. Physure Eager Mode
    q1 = Quantity(val1, units.meter)
    q2 = Quantity(val2, units.meter)

    start = time.perf_counter()
    for _ in range(100):
        res = q1 + q2
        # res.magnitude is tensor
        # Check if computation happened
        _ = res.magnitude
        torch.cuda.synchronize() if device == "cuda" else None
    end = time.perf_counter()
    mk_eager_time = (end - start) / 100
    print(
        f"Physure Eager Time: {mk_eager_time * 1000:.4f} ms (Ratio: {mk_eager_time / torch_time:.2f}x)"
    )

    # 3. Physure Compiled Mode (Zero Overhead)
    @torch.compile(fullgraph=True)
    def add_quantities(a, b):
        return a + b

    # Warmup
    _ = add_quantities(q1, q2)

    start = time.perf_counter()
    for _ in range(100):
        res = add_quantities(q1, q2)
        # res is likely a tensor because __torch_dispatch__ returns tensor for operations?
        # Or does compiled graph output the tensor result?
        # If __torch_dispatch__ unwrap->op->wrap, it might return Quantity if wrap returns Quantity.
        # But my implementation returned x (Tensor).
        # So expectation is Tensor result or Wrapped result.

        # Force sync
        if isinstance(res, torch.Tensor):
            pass  # computation is in graph
        torch.cuda.synchronize() if device == "cuda" else None
    end = time.perf_counter()
    mk_compiled_time = (end - start) / 100
    print(
        f"Physure Compiled Time: {mk_compiled_time * 1000:.4f} ms (Ratio: {mk_compiled_time / torch_time:.2f}x)"
    )

    if mk_compiled_time / torch_time < 1.1:
        print("SUCCESS: Zero-Overhead Achieved (< 1.1x)")
    else:
        print("WARNING: Overhead detected.")


if __name__ == "__main__":
    benchmark_comparison()
