"""The Monte Carlo seed policy seen from Python (conf freeze Phase 4b, ``[Settings] monte_carlo_seed``).

Python's ``Q_`` and ``Quantity`` have no top-level evaluation to scope a seed to, so their draws come from the core's
stream of the calling thread: created at the thread's first draw from ``mix(seed, k)`` (``k`` counts the threads of
the process that drew), advancing with every draw. What that gives, and what these tests prove:

* the same program run twice in fresh processes gives the same samples (it differed on every run before the
  setting existed): reproducible for one thread doing the same operations in the same order;
* two threads never draw the same samples (two streams that both started at the seed would);
* ``monte_carlo_seed = random`` in a working-directory ``physure.conf`` brings back operating-system entropy.

Not reproducible across threads (which thread is ``k`` depends on scheduling): documented, not tested.
"""

import subprocess
import sys
import threading

from physure._core import Quantity, RationalUnit

METRE = RationalUnit({"m": (1, 1)})

PROGRAM = """
from physure._core import Quantity, RationalUnit, UnitRegistry
UnitRegistry.from_conf()
metre = RationalUnit({"m": (1, 1)})
for _ in range(3):
    q = Quantity(10.0, metre, 1.0, mode="monte_carlo", samples=None)
    print(repr(q.mean), repr(q.std_dev))
"""


def _run_in(directory):
    done = subprocess.run(
        [sys.executable, "-c", PROGRAM],
        cwd=directory,
        capture_output=True,
        text=True,
        check=True,
    )
    return done.stdout


def _measure():
    q = Quantity(10.0, METRE, 1.0, mode="monte_carlo", samples=None)
    return (q.mean, q.std_dev)


def test_a_program_gives_the_same_samples_in_every_process(tmp_path):
    first = _run_in(tmp_path)
    assert len(set(first.splitlines())) == 3, (
        "the stream advances: three measurements, three sample sets"
    )
    assert _run_in(tmp_path) == first


def test_a_working_directory_seed_changes_the_samples_and_random_brings_entropy_back(
    tmp_path,
):
    default = _run_in(tmp_path)
    (tmp_path / "physure.conf").write_text(
        "[Settings]\nmonte_carlo_seed = 5\n"
    )
    five = _run_in(tmp_path)
    assert five != default
    assert _run_in(tmp_path) == five
    (tmp_path / "physure.conf").write_text(
        "[Settings]\nmonte_carlo_seed = random\n"
    )
    assert _run_in(tmp_path) != _run_in(tmp_path)


def test_two_threads_draw_from_different_streams():
    results = {}

    def first_draw(name):
        results[name] = _measure()

    threads = [
        threading.Thread(target=first_draw, args=(name,))
        for name in ("a", "b")
    ]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert results["a"] != results["b"], (
        "two streams that both started at the seed would draw the same samples"
    )


def test_two_measurements_on_one_thread_are_independent():
    a = Quantity(10.0, METRE, 1.0, mode="monte_carlo", samples=None)
    b = Quantity(10.0, METRE, 1.0, mode="monte_carlo", samples=None)
    difference = a - b
    assert difference.std_dev != 0.0, (
        "two sample sets that were the same would cancel exactly"
    )
    assert abs(difference.std_dev / a.std_dev - 2**0.5) < 0.1
