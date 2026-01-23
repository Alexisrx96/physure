import logging
import timeit

import jax
import jax.numpy as jnp
import matplotlib.pyplot as plt
import torch

from measurekit import Q_

# --- 1. CONFIG & LOGGING ---
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s"
)
logger = logging.getLogger("MeasureKit-Bench")

# This is the "Zero Smoke" setting.
# It will tell us if Torch is actually compiling or falling back to slow mode.
torch._logging.set_logs(graph_breaks=True, recompiles=True)

results = {
    "Scalar Eager": {},
    "Vectorized (NumPy)": {},
    "Torch Compiled": {},
    "JAX JIT": {},
}

# --- 2. THE BENCHMARKS ---


def bench_scalar():
    logger.info(">>> Benchmarking Scalar Eager (100k ops)...")
    setup = "from measurekit import Q_; a=Q_(1,'m'); b=Q_(2,'m')"
    results["Scalar Eager"]["MeasureKit"] = (
        min(timeit.repeat("a+b", setup, number=100000)) / 100000
    )
    results["Scalar Eager"]["Raw Python"] = (
        min(timeit.repeat("1.0+2.0", number=100000)) / 100000
    )


def bench_vectorized():
    N = 1_000_000
    logger.info(f">>> Benchmarking Vectorized NumPy (N={N})...")
    setup = f"import numpy as np; from measurekit import Q_; a=Q_(np.ones({N}), 'm'); b=Q_(np.ones({N}), 'm')"
    results["Vectorized (NumPy)"]["MeasureKit"] = (
        min(timeit.repeat("a+b", setup, number=50)) / 50
    )
    results["Vectorized (NumPy)"]["Raw NumPy"] = (
        min(
            timeit.repeat(
                "a+b",
                f"import numpy as np; a=np.ones({N}); b=np.ones({N})",
                number=50,
            )
        )
        / 50
    )


def bench_torch_compile():
    N = 1_000_000
    logger.info(">>> Benchmarking Torch Compile (The Zero-Overhead Test)...")
    t_a, t_b = torch.randn(N), torch.randn(N)
    mk_a, mk_b = Q_(t_a, "m"), Q_(t_b, "m")

    @torch.compile(
        fullgraph=True
    )  # fullgraph=True forces an error if zero-overhead isn't possible
    def add(x, y):
        return x + y

    logger.info("  - Warming up Torch compiler (this triggers cl.exe)...")
    add(mk_a, mk_b)

    results["Torch Compiled"]["MeasureKit"] = (
        min(timeit.repeat(lambda: add(mk_a, mk_b), number=100)) / 100
    )

    @torch.compile
    def raw_add(x, y):
        return x + y

    raw_add(t_a, t_b)
    results["Torch Compiled"]["Raw Torch"] = (
        min(timeit.repeat(lambda: raw_add(t_a, t_b), number=100)) / 100
    )


def bench_jax_jit():
    N = 1_000_000
    logger.info(">>> Benchmarking JAX JIT...")
    j_a, j_b = jnp.ones(N), jnp.ones(N)
    mk_a, mk_b = Q_(j_a, "m"), Q_(j_b, "m")

    @jax.jit
    def add(x, y):
        return x + y

    add(mk_a, mk_b).magnitude.block_until_ready()  # Warmup
    results["JAX JIT"]["MeasureKit"] = (
        min(
            timeit.repeat(
                lambda: add(mk_a, mk_b).magnitude.block_until_ready(),
                number=100,
            )
        )
        / 100
    )

    results["JAX JIT"]["Raw JAX"] = (
        min(
            timeit.repeat(
                lambda: add(j_a, j_b).block_until_ready(), number=100
            )
        )
        / 100
    )


# --- 3. PLOTTING ---


def plot():
    logger.info(">>> Generating Plots...")
    fig, axes = plt.subplots(2, 2, figsize=(14, 10))
    axes = axes.flatten()

    for i, (title, data) in enumerate(results.items()):
        ax = axes[i]
        keys = list(data.keys())
        vals = [v * 1000 for v in data.values()]  # ms

        bars = ax.bar(keys, vals, color=["#3498db", "#95a5a6"])
        ax.set_title(title)
        ax.set_ylabel("ms")

        # Add ratio text
        base = vals[1] if len(vals) > 1 else vals[0]
        for bar in bars:
            yval = bar.get_height()
            ax.text(
                bar.get_x() + bar.get_width() / 2,
                yval,
                f"{yval / base:.2f}x",
                ha="center",
                va="bottom",
            )

    plt.tight_layout()
    plt.savefig("full_backend_comparison.png")
    logger.info("SUCCESS: saved as 'full_backend_comparison.png'")


if __name__ == "__main__":
    bench_jax_jit()
    bench_torch_compile()
    bench_scalar()
    bench_vectorized()
    plot()
