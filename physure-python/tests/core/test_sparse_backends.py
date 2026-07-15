import pytest

from physure.core.dispatcher import BackendManager


def get_backends():
    import importlib.util

    backends = []
    if importlib.util.find_spec("numpy"):
        backends.append("numpy")

    if importlib.util.find_spec("torch"):
        backends.append("torch")

    if importlib.util.find_spec("jax"):
        backends.append("jax")
    return backends


@pytest.mark.parametrize("backend_name", get_backends())
def test_sparse_eye(backend_name):
    backend = BackendManager._get_or_load_backend(backend_name)
    n = 10

    # Check sparse_eye
    # Check sparse_eye
    sp_eye = backend.sparse_eye(n)
    assert backend.shape(sp_eye) == (n, n)

    # Just check diagonal is 1
    # We don't have a 'diag' method on backend directly that works on sparse
    # reliably in consistent way across backends (sparse_diagonal exists)
    diag = backend.sparse_diagonal(sp_eye)
    expected = backend.ones((n,))
    assert backend.allclose(diag, expected)


@pytest.mark.parametrize("backend_name", get_backends())
def test_sparse_diags(backend_name):
    backend = BackendManager._get_or_load_backend(backend_name)

    # 3 diagonals: main, +1, -1
    d0 = backend.ones((5,))
    d1 = backend.ones((4,))
    dm1 = backend.ones((4,))

    diags = [d0, d1, dm1]
    offsets = [0, 1, -1]

    try:
        sp_mat = backend.sparse_diags(diags, offsets, shape=(5, 5))
        assert backend.shape(sp_mat) == (5, 5)

        # Check trace or something
        # sparse_diagonal main
        diag = backend.sparse_diagonal(sp_mat)
        assert backend.allclose(diag, d0)

        # Check structure
        # In torch/jax we can check sum
        total_sum = backend.sum(sp_mat)

        # Handle potential scalar array wrapper
        val = (
            total_sum.item()
            if hasattr(total_sum, "item")
            else float(total_sum)
        )

        # 5 + 4 + 4 = 13
        # allow small float error
        assert abs(val - 13.0) < 1e-5

    except NotImplementedError:
        pytest.skip(f"sparse_diags not implemented for {backend_name}")


if __name__ == "__main__":
    pass
