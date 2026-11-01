import pytest

from ._array_backends import x64_for


@pytest.fixture
def x64(lib):
    """Runs a test that is parametrized over `lib` with JAX in float64."""
    with x64_for(lib):
        yield
