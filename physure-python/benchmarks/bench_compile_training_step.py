"""Does the unit bookkeeping cost anything in a compiled training step?

One forward and backward pass of a small physics loss, four ways:

* plain tensors, eager and compiled: what the model costs without units, the floor;
* ``Quantity``, eager and compiled: what the units add, and whether ``torch.compile`` recovers it.

Run from ``physure-python/``: ``uv run --no-sync python benchmarks/bench_compile_training_step.py [--json]``.
``torch.compile`` runs with ``aot_eager`` (graph capture without a C++ compiler) and, when this machine can build
kernels, with ``inductor``. Each row is the median of several timed batches after a warm-up call; the warm-up
(compile) time is reported separately.
"""

from __future__ import annotations

import json
import logging
import statistics
import sys
import time
import warnings

import torch

from physure import Q_

warnings.filterwarnings("ignore")
logging.getLogger("torch").setLevel(logging.CRITICAL)
logging.getLogger("torch._dynamo").setLevel(logging.CRITICAL)
logging.getLogger("torch._inductor").setLevel(logging.CRITICAL)


# Sizes stop at 4,000: with torch tensors `Quantity * Quantity` and `Quantity / Quantity` build dense N x N diagonal
# Jacobians, so a step costs O(N^2) in time and memory (1,000,000 elements would ask for 4 TB).
SIZES = (100, 1_000, 4_000)


def plain_step(mass, d, t, target):
    v = d / t
    e = 0.5 * mass * v * v
    return ((e - target) ** 2).mean()


def quantity_step(mass, d, t, target):
    m, dist, time_ = Q_(mass, "kg"), Q_(d, "m"), Q_(t, "s")
    v = dist / time_
    e = (0.5 * m * v * v).to("J")
    err = e - Q_(target, "J")
    return (err.magnitude**2).mean()


def make_inputs(n: int):
    torch.manual_seed(0)
    mass = torch.rand(n, requires_grad=True) + 1.0
    mass.retain_grad()
    return mass, torch.rand(n) + 1.0, torch.rand(n) + 1.0, torch.rand(n)


def time_step(
    fn, n: int, repeats: int = 7, per_batch: int = 20
) -> tuple[float, float]:
    """(warm-up seconds, median seconds per step) of forward + backward."""
    mass = torch.rand(n, requires_grad=True)
    d, t, target = torch.rand(n) + 1.0, torch.rand(n) + 1.0, torch.rand(n)
    started = time.perf_counter()
    fn(mass + 1.0, d, t, target).backward()
    warmup = time.perf_counter() - started
    batches = []
    for _ in range(repeats):
        started = time.perf_counter()
        for _ in range(per_batch):
            mass.grad = None
            fn(mass + 1.0, d, t, target).backward()
        batches.append((time.perf_counter() - started) / per_batch)
    return warmup, statistics.median(batches)


def inductor_available() -> bool:
    try:
        torch._dynamo.reset()
        torch.compile(lambda x: x * 2 + 1, backend="inductor")(torch.ones(4))
        return True
    except Exception:
        return False
    finally:
        torch._dynamo.reset()


def main() -> None:
    backends = ["aot_eager"] + (["inductor"] if inductor_available() else [])
    rows = []
    for n in SIZES:
        entries = [
            ("plain eager", plain_step),
            ("Quantity eager", quantity_step),
        ]
        for backend in backends:
            entries.append(
                (
                    f"plain compiled ({backend})",
                    torch.compile(plain_step, backend=backend),
                )
            )
            entries.append(
                (
                    f"Quantity compiled ({backend})",
                    torch.compile(quantity_step, backend=backend),
                )
            )
        for name, fn in entries:
            if "compiled" in name:
                torch._dynamo.reset()
            warmup, step = time_step(fn, n)
            rows.append(
                {
                    "n": n,
                    "variant": name,
                    "warmup_s": round(warmup, 3),
                    "step_ms": round(step * 1000, 3),
                }
            )
    if "--json" in sys.argv:
        print(
            json.dumps(
                {
                    "torch": torch.__version__,
                    "python": sys.version.split()[0],
                    "backends": backends,
                    "rows": rows,
                }
            )
        )
        return
    print(
        f"torch {torch.__version__}  python {sys.version.split()[0]}  backends {backends}"
    )
    for n in SIZES:
        print(f"\nn = {n:,}")
        print(f"{'variant':34} {'warm-up s':>10} {'ms/step':>10}")
        base = next(
            r["step_ms"]
            for r in rows
            if r["n"] == n and r["variant"] == "plain eager"
        )
        for r in (r for r in rows if r["n"] == n):
            print(
                f"{r['variant']:34} {r['warmup_s']:10.2f} {r['step_ms']:10.3f}   x{r['step_ms'] / base:5.1f}"
            )


if __name__ == "__main__":
    main()
