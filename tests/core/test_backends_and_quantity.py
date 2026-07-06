try:
    import numpy as np
except ImportError:
    np = None

import pytest

try:
    from measurekit.backends.numpy_backend import NumpyBackend
except (ImportError, AttributeError):
    NumpyBackend = None


from measurekit.backends.python_backend import PythonBackend
from measurekit.core.dispatcher import BackendManager, get_backend
from measurekit.core.protocols import BackendOps
from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.system import UnitSystem
from measurekit.domain.measurement.units import CompoundUnit


@pytest.fixture
def clean_backend_manager():
    # Reset singleton state if needed
    BackendManager._backends.clear()
    BackendManager._python_backend = None
    # CovarianceStore is now stateless/context-aware
    yield
    BackendManager._backends.clear()
    BackendManager._python_backend = None


def test_numpy_backend_compliance():
    """Verify NumpyBackend implements BackendOps."""
    backend = NumpyBackend()
    assert isinstance(backend, BackendOps)


def test_python_backend_compliance():
    """Verify PythonBackend implements BackendOps."""
    backend = PythonBackend()
    assert isinstance(backend, BackendOps)


@pytest.mark.skipif(NumpyBackend is None, reason="numpy not available")
class TestNumpyBackend:
    @pytest.fixture
    def backend(self):
        return NumpyBackend()

    def test_creation(self, backend):
        arr = np.array([1, 2, 3])
        assert backend.is_array(arr)
        assert not backend.is_array([1, 2, 3])

        converted = backend.asarray([1, 2, 3])
        assert isinstance(converted, np.ndarray)
        assert np.all(converted == np.array([1, 2, 3]))

    def test_math(self, backend):
        x = np.array([1.0, 2.0, 4.0])
        y = np.array([2.0, 3.0, 0.5])

        assert np.allclose(backend.add(x, y), [3.0, 5.0, 4.5])
        assert np.allclose(backend.sub(x, y), [-1.0, -1.0, 3.5])
        assert np.allclose(backend.mul(x, y), [2.0, 6.0, 2.0])
        assert np.allclose(backend.truediv(x, y), [0.5, 2.0 / 3.0, 8.0])
        assert np.allclose(backend.pow(x, 2), [1.0, 4.0, 16.0])
        assert np.allclose(backend.sqrt(x), [1.0, 1.41421356, 2.0])

    def test_reduction(self, backend):
        x = np.array([[1, 2], [3, 4]])
        assert backend.sum(x) == 10
        assert np.all(backend.sum(x, axis=0) == [4, 6])
        assert backend.mean(x) == 2.5

    def test_sparse_helpers(self, backend):
        from scipy import sparse

        eye = backend.eye(3)
        assert sparse.issparse(eye)
        assert eye.shape == (3, 3)

        diags = backend.diags([[1, 2]], [0])
        assert sparse.issparse(diags)


class TestPythonBackend:
    @pytest.fixture
    def backend(self):
        return PythonBackend()

    def test_creation(self, backend):
        assert not backend.is_array([1, 2])
        assert backend.asarray(5) == 5

    def test_math(self, backend):
        x, y = 10.0, 2.0
        assert backend.add(x, y) == 12.0
        assert backend.sub(x, y) == 8.0
        assert backend.mul(x, y) == 20.0
        assert backend.truediv(x, y) == 5.0
        assert backend.sqrt(16.0) == 4.0
        assert backend.sin(0) == 0.0
        assert backend.cos(0) == 1.0

    def test_reduction(self, backend):
        lst = [1, 2, 3, 4]
        assert backend.sum(lst) == 10
        assert backend.mean(lst) == 2.5
        assert backend.any([0, 1, 0]) is True
        assert backend.all([1, 1, 1]) is True


class TestDispatcher:
    def test_dispatch(self, clean_backend_manager):
        arr = np.array([1, 2, 3])
        assert isinstance(get_backend(arr), NumpyBackend)

        scalar = 5.0
        assert isinstance(get_backend(scalar), PythonBackend)

        lst = [1, 2, 3]
        assert isinstance(get_backend(lst), PythonBackend)


class TestQuantityBackendIntegration:
    @pytest.fixture
    def system(self):
        s = UnitSystem()
        L = Dimension({"L": 1})
        s.register_unit("m", L, LinearConverter(1.0), "meter")
        return s

    @pytest.mark.skipif(np is None, reason="numpy not available")
    def test_numpy_operations(self, system):
        # Create Quantity with NumPy array
        m = CompoundUnit({"m": 1})
        q_arr = Quantity.from_input(np.array([1.0, 2.0, 3.0]), m, system)

        # Add scalar Quantity
        q_scalar = Quantity.from_input(1.0, m, system)

        res = q_arr + q_scalar
        assert isinstance(res.magnitude, np.ndarray)
        assert np.allclose(res.magnitude, [2.0, 3.0, 4.0])
        assert isinstance(res.uncertainty, (np.ndarray, float))

    def test_python_scalar_operations(self, system):
        m = CompoundUnit({"m": 1})
        q1 = Quantity.from_input(10.0, m, system)
        q2 = Quantity.from_input(5.0, m, system)

        res = q1 / q2
        assert res.magnitude == 2.0
        assert res._backend.is_array(res.magnitude) is False

    @pytest.mark.skipif(np is None, reason="numpy not available")
    def test_cross_backend_interaction(self, system):
        # Array + Scalar (native python)
        m = CompoundUnit({"m": 1})
        q_arr = Quantity.from_input(np.array([1.0, 2.0]), m, system)

        # Quantity arithmetic handles raw numbers via backend
        res = q_arr * 2.0
        assert np.allclose(res.magnitude, [2.0, 4.0])
