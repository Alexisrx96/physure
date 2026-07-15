import asyncio
import threading
import time

import pytest

from measurekit import Q_
from measurekit.application.context import get_current_system, use_system
from measurekit.application.startup import create_system


# Pre-create systems to avoid file I/O noise during concurrency tests
# (though use_system with string does I/O, we can pass objects)
def setup_systems():
    si = create_system("international.conf")
    imperial = create_system("imperial.conf")
    return si, imperial


SI_SYSTEM, IMPERIAL_SYSTEM = setup_systems()


def test_thread_isolation():
    """Verify that separate threads can use different systems simultaneously."""

    results = {"thread_a": None, "thread_b": None}

    def thread_a_work():
        with use_system(IMPERIAL_SYSTEM):
            # Simulate work
            time.sleep(0.1)
            sys_name = get_current_system().name
            # Check unit behavior
            q = Q_(10, "ft")  # Should work in Imperial
            results["thread_a"] = (sys_name, str(q.unit))

    def thread_b_work():
        with use_system(SI_SYSTEM):
            # Simulate work
            time.sleep(0.1)
            sys_name = get_current_system().name
            q = Q_(10, "m")
            results["thread_b"] = (sys_name, str(q.unit))

    t1 = threading.Thread(target=thread_a_work)
    t2 = threading.Thread(target=thread_b_work)

    t1.start()
    t2.start()

    t1.join()
    t2.join()

    # Assertions

    # Imperial
    assert (
        "Imperial" in results["thread_a"][0]
        or "British" in results["thread_a"][0]
    )
    assert results["thread_a"][1] == "ft"

    # SI
    assert "SI" in results["thread_b"][0]
    assert results["thread_b"][1] == "m"


@pytest.mark.asyncio
async def test_async_isolation():
    """Verify that asyncio tasks maintain separate contexts."""

    async def task_imperial():
        with use_system(IMPERIAL_SYSTEM):
            await asyncio.sleep(0.1)
            return get_current_system().name, str(Q_(1, "ft").unit)

    async def task_si():
        # Default fallback or explicit SI
        # Let's test explicit first to be sure
        with use_system(SI_SYSTEM):
            await asyncio.sleep(0.05)  # Different timing
            return get_current_system().name, str(Q_(1, "m").unit)

    # Run concurrently
    res_imp, res_si = await asyncio.gather(task_imperial(), task_si())

    assert "Imperial" in res_imp[0]
    assert res_imp[1] == "ft"

    assert "SI" in res_si[0]
    assert res_si[1] == "m"


def test_context_fallback():
    """Verify that get_default_system returns SI if no context is set."""
    # Ensure we are in a clean state (no active context var)
    sys = get_current_system()
    assert "SI" in sys.name


def test_nested_context():
    """Verify that nested contexts work correctly."""
    with use_system(SI_SYSTEM):
        assert "SI" in get_current_system().name

        with use_system(IMPERIAL_SYSTEM):
            assert "Imperial" in get_current_system().name
            q = Q_(1, "ft")
            assert str(q.unit) == "ft"

        # Should revert
        assert "SI" in get_current_system().name
