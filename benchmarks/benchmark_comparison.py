import os
import sys
import time

import matplotlib.pyplot as plt
import numpy as np

# Ensure measurekit is in path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

import pint

from measurekit import Quantity, u

# Setup Pint for comparison
ureg = pint.UnitRegistry()


# --- Benchmark Helpers ---
def setup_pint_task(n: int, ureg):
    """Sets up the Pint benchmark task."""
    q_pint = np.ones(n) * ureg.meter

    def pint_task():
        return q_pint.sum().to("kilometer")

    return pint_task


def setup_mk_numpy_task(n: int):
    """Sets up the MeasureKit (NumPy) benchmark task."""
    q_mk = Quantity(np.ones(n), u.m)

    def mk_numpy_task():
        sum_mag = q_mk.magnitude.sum()
        return Quantity(sum_mag, u.m).to(u.km)

    return mk_numpy_task


def setup_pure_numpy_task(n: int):
    """Sets up the Pure NumPy benchmark task."""
    arr_np = np.ones(n)

    def numpy_task():
        return arr_np.sum() / 1000.0

    return numpy_task


def setup_numba_task(n: int):
    """Sets up the MeasureKit (Numba) benchmark task."""
    try:
        from numba import NumbaError, TypingError, njit

        import measurekit.ext.numba_support  # noqa: F401

        q_mk = Quantity(np.ones(n), u.m)

        @njit
        def numba_sum_op(q):
            mag = q.magnitude
            acc = 0.0
            for i in range(len(mag)):
                acc += mag[i]
            return acc / 1000.0

        numba_sum_op(q_mk)  # Warmup
        return lambda: numba_sum_op(q_mk)
    except (ImportError, RecursionError, NumbaError) as e:
        print(f"Skipping Numba benchmark: {e}")
        return None


def setup_jax_task(n: int):
    """Sets up the MeasureKit (JAX) benchmark task."""
    try:
        import jax.numpy as jnp
        from jax import jit

        q_jax = Quantity(jnp.ones(n), u.m)

        @jit
        def jax_sum_op(q):
            return q.magnitude.sum() / 1000.0

        jax_sum_op(q_jax).block_until_ready()  # Warmup
        return lambda: jax_sum_op(q_jax).block_until_ready()
    except (ImportError, Exception):
        print("Skipping JAX benchmark.")
        return None


def execute_tasks(tasks, iterations):
    """Executes tasks and returns results."""
    results = {}
    for name, task in tasks.items():
        print(f"   Executing {name}...")
        try:
            task()
            start = time.perf_counter()
            for _ in range(iterations):
                task()
            avg_time = (time.perf_counter() - start) / iterations
            results[name] = avg_time
            print(f"      Mean: {avg_time * 1000:.4f} ms")
        except Exception as e:
            print(f"      Failed: {e}")
    return results


def plot_results(results):
    """Plots benchmark results."""
    if not results:
        print("No results to plot.")
        return

    names = sorted(results.keys(), key=lambda x: results[x], reverse=True)
    times_ms = [results[n] * 1000 for n in names]

    plt.figure(figsize=(10, 6), dpi=100)
    colors = [
        "#e74c3c"
        if "Pint" in n
        else "#f1c40f"
        if "Numba" in n
        else "#9b59b6"
        if "JAX" in n
        else "#2ecc71"
        if "MeasureKit" in n
        else "#95a5a6"
        for n in names
    ]

    bars = plt.bar(names, times_ms, color=colors, edgecolor="black", alpha=0.8)
    plt.yscale("log")
    plt.ylabel("Execution Time (ms) - Log Scale")
    plt.title(
        "Performance Benchmark: Sum & Convert (1M Elements)", fontsize=14
    )
    plt.grid(True, which="both", ls="-", alpha=0.1)

    for bar in bars:
        yval = bar.get_height()
        plt.text(
            bar.get_x() + bar.get_width() / 2,
            yval * 1.1,
            f"{yval:.2f}",
            va="bottom",
            ha="center",
            fontsize=9,
        )

    plt.tight_layout()
    plt.savefig("benchmark_results.png")
    print("\nBenchmark complete. Saved to 'benchmark_results.png'.")


def run_benchmarks():
    """Run performance benchmarks comparing MeasureKit with competitors."""
    n = 1_000_000
    iterations = 10
    print(f"Benchmarking with N = {n}, {iterations} iterations...")

    tasks = {
        "Pint": setup_pint_task(n, ureg),
        "MeasureKit (NumPy)": setup_mk_numpy_task(n),
        "Pure NumPy": setup_pure_numpy_task(n),
    }

    if mk_numba := setup_numba_task(n):
        tasks["MeasureKit (Numba)"] = mk_numba
    if mk_jax := setup_jax_task(n):
        tasks["MeasureKit (JAX)"] = mk_jax

    results = execute_tasks(tasks, iterations)
    plot_results(results)


if __name__ == "__main__":
    run_benchmarks()
