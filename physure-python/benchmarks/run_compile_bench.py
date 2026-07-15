import logging
import timeit
import traceback

import torch

from physure import Q_

# Setup Logging
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s"
)
logger = logging.getLogger("Benchmark")


def bench_torch_compile():
    logger.info(">>> Benchmarking Torch Compile (The Zero-Overhead Test)...")
    N = 1_000_000

    device = "cuda" if torch.cuda.is_available() else "cpu"
    logger.info(f"  - Using device: {device}")

    t_a, t_b = torch.randn(N, device=device), torch.randn(N, device=device)
    mk_a, mk_b = Q_(t_a, "m"), Q_(t_b, "m")

    # We remove fullgraph=True because the current code
    # HAS graph breaks. This allows it to run, but slower.
    # explicit backend to avoid platform issues
    @torch.compile(backend="aot_eager", fullgraph=True)
    def add(x, y):
        return x + y

    logger.info("  - Warming up Torch (Expected Graph Breaks in logs)...")
    try:
        add(mk_a, mk_b)
    except Exception as e:
        logger.error(f"  - Compile failed: {e}")
        traceback.print_exc()
        return

    logger.info("  - Measuring...")
    # Repeat a few times
    results = timeit.repeat(lambda: add(mk_a, mk_b), number=100, repeat=5)
    best_time = min(results) / 100

    logger.info(f"  - Best Average Time: {best_time * 1000:.4f} ms")


if __name__ == "__main__":
    bench_torch_compile()
