import time

import physure as mk


def benchmark_lineage(n):
    """Benchmarks lineage growth performance."""
    print(f"Benchmarking lineage growth with {n} additions...")

    with mk.config.propagation_mode("correlated"):
        q = mk.Q_(1.0, "m", uncertainty=0.1)
        start = time.time()
        for _ in range(n):
            q = q + mk.Q_(1.0, "m", uncertainty=0.1)
        # Lineage should have n+1 elements.
        _ = q.uncertainty
        end = time.time()
        print(f"Correlated Time: {end - start:.4f}s")
        print(f"Lineage size: {len(q.uncertainty_obj.lineage)}")

    with mk.config.propagation_mode("uncorrelated"):
        q = mk.Q_(1.0, "m", uncertainty=0.1)
        start = time.time()
        for _ in range(n):
            q = q + mk.Q_(1.0, "m", uncertainty=0.1)
        _ = q.uncertainty
        end = time.time()
        print(f"Uncorrelated Time: {end - start:.4f}s")


if __name__ == "__main__":
    benchmark_lineage(1000)
    benchmark_lineage(5000)
