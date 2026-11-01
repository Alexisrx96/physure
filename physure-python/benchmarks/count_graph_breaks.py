"""Counts the TorchDynamo graph breaks of unit operations on ``Quantity`` (docs/torch_compile_integration.md).

Run from ``physure-python/``: ``uv run --no-sync python benchmarks/count_graph_breaks.py [--json] [-v]``.
``-v`` prints each distinct reason with the physure source line behind it. The numbers depend on the torch
version; ``tests/ext/test_graph_breaks.py`` holds the baseline per version and fails when a count goes up.
"""

from __future__ import annotations

import json
import logging
import sys
import warnings

import torch
import torch._dynamo as dynamo

from physure import Q_

warnings.filterwarnings("ignore")
logging.getLogger("torch").setLevel(logging.CRITICAL)
logging.getLogger("torch._dynamo").setLevel(logging.CRITICAL)

CASES = {
    "(a + b) * 2.0": lambda a, b, s, t: (a + b) * 2.0,
    "a * b": lambda a, b, s, t: a * b,
    "a / b": lambda a, b, s, t: a / b,
    "a.to('km')": lambda a, b, s, t: a.to("km"),
    "s.to('km/h')": lambda a, b, s, t: s.to("km/h"),
    "t.to('K')": lambda a, b, s, t: t.to("K"),
}


def count_breaks(verbose: bool = False) -> dict[str, tuple[int, int]]:
    """Maps each expression to ``(graph_breaks, graphs)`` as ``torch._dynamo.explain`` reports them."""
    torch.manual_seed(0)
    results: dict[str, tuple[int, int]] = {}
    for name, fn in CASES.items():
        dynamo.reset()
        a, b = Q_(torch.randn(4, 4), "m"), Q_(torch.randn(4, 4), "m")
        s, t = Q_(torch.randn(4, 4), "m/s"), Q_(torch.randn(4, 4), "degC")
        explanation = dynamo.explain(fn)(a, b, s, t)
        results[name] = (
            explanation.graph_break_count,
            explanation.graph_count,
        )
        if verbose:
            seen = set()
            for reason in explanation.break_reasons:
                frames = [
                    f
                    for f in (reason.user_stack or [])
                    if "physure" in f.filename.replace("\\", "/")
                    and ".venv" not in f.filename
                ]
                where = (
                    f"{frames[-1].filename.replace(chr(92), '/').split('physure/')[-1]}:{frames[-1].lineno} `{(frames[-1].line or '').strip()[:60]}`"
                    if frames
                    else "?"
                )
                key = (str(reason.reason).strip().splitlines()[0][:100], where)
                if key not in seen:
                    seen.add(key)
                    print(f"    {name}: {key[0]}  [{key[1]}]", file=sys.stderr)
    return results


if __name__ == "__main__":
    counts = count_breaks(verbose="-v" in sys.argv)
    if "--json" in sys.argv:
        print(
            json.dumps(
                {
                    "torch": torch.__version__,
                    "python": sys.version.split()[0],
                    "counts": counts,
                }
            )
        )
    else:
        print(f"torch {torch.__version__}  python {sys.version.split()[0]}")
        print(f"{'expression':16} breaks graphs")
        for name, (breaks, graphs) in counts.items():
            print(f"{name:16} {breaks:6} {graphs:6}")
