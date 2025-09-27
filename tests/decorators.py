# decorators.py (or added to tests/base_test_class.py)
import functools

from measurekit.context import system_context


def with_system_context(f):
    """
    Decorator to wrap a test method with a system context.

    This ensures that a fresh system is set up and torn down for the
    decorated test method, making it independent from other tests.
    """

    @functools.wraps(f)
    def wrapper(self, *args, **kwargs):
        with system_context(self.system):
            return f(self, *args, **kwargs)

    return wrapper
