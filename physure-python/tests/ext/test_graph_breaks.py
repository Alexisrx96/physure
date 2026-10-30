"""The number of TorchDynamo graph breaks of unit operations never goes up.

``benchmarks/count_graph_breaks.py`` counts them with ``torch._dynamo.explain``. The baseline is per torch minor
version because Dynamo's own behaviour changes between releases; a version without a baseline is skipped, not
guessed at. When a change lowers a count, lower it here: that is the point of the work, and the next change is held to it.
"""

import importlib.util
import sys
from pathlib import Path

import pytest

try:
    import torch
except ImportError:
    torch = None

# (graph breaks, graphs) per expression, measured by benchmarks/count_graph_breaks.py
BASELINE = {
    "2.13": {
        "(a + b) * 2.0": (6, 7),
        "a * b": (4, 5),
        "a / b": (7, 8),
        "a.to('km')": (2, 3),
        "s.to('km/h')": (2, 3),
        "t.to('K')": (2, 3),
    },
}


def _torch_minor() -> str:
    return ".".join(torch.__version__.split("+")[0].split(".")[:2])


@pytest.mark.skipif(torch is None, reason="PyTorch not installed")
@pytest.mark.skipif(
    torch is not None and sys.version_info >= (3, 14) and tuple(int(x) for x in torch.__version__.split(".")[:2]) < (2, 10),
    reason="torch.compile on Python 3.14+ requires torch >= 2.10",
)
def test_graph_breaks_do_not_go_up():
    if _torch_minor() not in BASELINE:
        pytest.skip(f"no graph-break baseline for torch {_torch_minor()}")
    script = Path(__file__).resolve().parents[2] / "benchmarks" / "count_graph_breaks.py"
    spec = importlib.util.spec_from_file_location("count_graph_breaks", script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    baseline = BASELINE[_torch_minor()]
    counts = module.count_breaks()
    assert set(counts) == set(baseline)
    worse = {name: (counts[name], baseline[name]) for name in baseline if counts[name][0] > baseline[name][0]}
    assert not worse, f"more graph breaks than the baseline (now, baseline): {worse}"
