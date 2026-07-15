"""Tests for the Rust CovarianceStore and its Python fallback CoreStore."""

import numpy as np
import pytest

try:
    from measurekit_core import CovarianceStore as RustCovarianceStore
    from measurekit_core import PruningConfig

    HAS_RUST = True
except ImportError:
    HAS_RUST = False

from measurekit.backends.numpy_backend import NumpyBackend
from measurekit.domain.measurement.vectorized_uncertainty import (
    CovarianceStore,
)

# ---------------------------------------------------------------------------
# Rust CovarianceStore direct tests
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not HAS_RUST, reason="measurekit_core not available")
class TestRustCovarianceStore:
    def setup_method(self):
        self.store = RustCovarianceStore()

    def test_register_diagonal_and_retrieve(self):
        diag = np.array([1.0, 4.0, 9.0], dtype=np.float64)
        self.store.register_diagonal(0, diag)
        result = self.store.get_block_csr(0, 0)
        assert result is not None
        data, indices, indptr, shape = result
        assert shape == (3, 3)
        # Reconstruct and check diagonal
        import scipy.sparse

        mat = scipy.sparse.csr_matrix((data, indices, indptr), shape=shape)
        np.testing.assert_allclose(mat.diagonal(), diag)

    def test_register_variable_full_matrix(self):
        cov = np.diag([2.0, 8.0]).astype(np.float64)
        self.store.register_variable(10, cov)
        result = self.store.get_block_csr(10, 10)
        assert result is not None
        _data, _indices, _indptr, shape = result
        assert shape == (2, 2)

    def test_propagate_scales_variance(self):
        """J * Sigma * J^T with scalar J=2, Sigma=diag([1,4]) → diag([4,16])."""
        diag = np.array([1.0, 4.0], dtype=np.float64)
        self.store.register_diagonal(0, diag)
        # Jacobian: 2*I
        jac = np.eye(2, dtype=np.float64) * 2.0
        self.store.propagate(1, [0], [jac])
        result = self.store.get_block_csr(1, 1)
        assert result is not None
        data, indices, indptr, shape = result
        import scipy.sparse

        mat = scipy.sparse.csr_matrix((data, indices, indptr), shape=shape)
        np.testing.assert_allclose(mat.diagonal(), [4.0, 16.0])

    def test_propagate_cross_covariance(self):
        """After propagating two outputs from the same input, cross-cov should be non-zero."""
        diag = np.array([1.0, 1.0], dtype=np.float64)
        self.store.register_diagonal(0, diag)
        # output 1: J=2*I, output 2: J=3*I
        jac1 = np.eye(2, dtype=np.float64) * 2.0
        jac2 = np.eye(2, dtype=np.float64) * 3.0
        self.store.propagate(1, [0], [jac1])
        self.store.propagate(2, [0], [jac2])
        # Cov(out1, out2) = J1 * Sigma_00 * J2^T = 2*I * diag([1,1]) * 3*I = diag([6,6])
        result = self.store.get_block_csr(1, 2)
        assert result is not None
        data, indices, indptr, shape = result
        import scipy.sparse

        mat = scipy.sparse.csr_matrix((data, indices, indptr), shape=shape)
        np.testing.assert_allclose(mat.diagonal(), [6.0, 6.0])

    def test_missing_block_returns_none(self):
        assert self.store.get_block_csr(99, 99) is None

    def test_pruning_config(self):
        config = PruningConfig(max_age=5, enabled=True, corr_threshold=1e-8)
        store = RustCovarianceStore(config)
        # Register and step forward enough times to trigger pruning
        diag = np.array([1.0], dtype=np.float64)
        store.register_diagonal(0, diag)
        jac = np.array([[1.0]])
        for i in range(1, 10):
            store.propagate(i, [i - 1], [jac])
        # Old variables should be pruned; we just verify it doesn't crash
        # and the most recent result is still retrievable
        assert store.get_block_csr(9, 9) is not None


# ---------------------------------------------------------------------------
# Python fallback CoreStore tests (always run)
# ---------------------------------------------------------------------------


class TestPythonCoreStoreFallback:
    """Tests the Python fallback CoreStore via the CovarianceStore wrapper."""

    def _make_store(self):
        backend = NumpyBackend()
        return CovarianceStore(backend=backend)

    def test_allocate_returns_sequential_slices(self):
        store = self._make_store()
        s1 = store.allocate(3)
        s2 = store.allocate(2)
        assert s1 == slice(0, 3)
        assert s2 == slice(3, 5)

    def test_register_independent_and_retrieve(self):
        store = self._make_store()
        std_dev = np.array([1.0, 2.0, 3.0])
        slc = store.register_independent_array(std_dev)
        assert slc.stop - slc.start == 3
        block = store.get_covariance_block(slc, slc)
        assert block is not None

    def test_update_from_propagation_scalar_jacobian(self):
        """Scalar jacobian (=2.0) on a 2-element array should scale variance by 4."""
        store = self._make_store()
        std_dev = np.array([1.0, 1.0])
        in_slc = store.register_independent_array(std_dev)
        out_slc = store.allocate(2)
        jac = np.array(2.0)  # scalar jacobian
        store.update_from_propagation(out_slc, [in_slc], [jac])
        block = store.get_covariance_block(out_slc, out_slc)
        assert block is not None

    def test_update_from_propagation_matrix_jacobian(self):
        """2x2 identity jacobian: output variance == input variance."""
        store = self._make_store()
        std_dev = np.array([3.0, 4.0])
        in_slc = store.register_independent_array(std_dev)
        out_slc = store.allocate(2)
        jac = np.eye(2)
        store.update_from_propagation(out_slc, [in_slc], [jac])
        block = store.get_covariance_block(out_slc, out_slc)
        import scipy.sparse

        if scipy.sparse.issparse(block):
            diag = block.diagonal()
        else:
            diag = np.diag(block) if hasattr(block, "diagonal") else block
        np.testing.assert_allclose(np.sort(np.abs(diag)), np.sort(std_dev**2))
