import os
import time

import numpy as np
import psutil

import physure as mk


def get_memory_usage():
    """Returns current process memory usage in MB."""
    process = psutil.Process(os.getpid())
    return process.memory_info().rss / 1024 / 1024  # MB


def benchmark_propagation(size, mode):
    """Benchmarks uncertainty propagation performance."""
    print(f"\nBenchmarking {mode} mode with size {size}...")

    # Force garbage collection
    import gc

    gc.collect()

    mem_start = get_memory_usage()

    with mk.propagation_mode(mode):
        # Create quantities
        rng = np.random.default_rng(42)  # Fixed seed for reproducibility
        m1 = rng.random(size)
        u1 = np.ones(size) * 0.01

        q1 = mk.Q_(m1, "m", uncertainty=u1)
        q2 = mk.Q_(m1, "m", uncertainty=u1)

        start_time = time.time()

        # Operation: a * b
        res = q1 * q2

        # Access uncertainty to force any lazy calculations
        std = res.uncertainty
        _ = np.mean(std)

        end_time = time.time()
        mem_end = get_memory_usage()

    print(f"Time: {end_time - start_time:.4f}s")
    print(f"Memory Increment: {mem_end - mem_start:.2f} MB")
    return end_time - start_time, mem_end - mem_start


def run():
    """Runs benchmarks for different sizes."""
    sizes = [100, 1000, 5000]

    for size in sizes:
        _, _ = benchmark_propagation(size, "correlated")
        _, _ = benchmark_propagation(size, "uncorrelated")

        if size > 1000:
            # Memory should be significantly smaller for uncorrelated
            # Correlated storage for size N is roughly (N^2 + offsets) sparse.
            # But here N is 5000, so N^2 is 25M elements.
            # Even sparse matrices take memory for indices.
            pass


if __name__ == "__main__":
    run()
