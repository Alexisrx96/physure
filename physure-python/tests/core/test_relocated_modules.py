"""Code that lives in the layer it belongs to, and still answers to its old name."""


def test_propagate_affine_lives_with_the_backends():
    from physure.backends.affine import propagate_affine
    from physure.domain.measurement.vectorized_uncertainty import (
        propagate_affine as old_name,
    )

    assert old_name is propagate_affine
    assert propagate_affine.__module__ == "physure.backends.affine"


def test_jit_lives_in_the_application_layer():
    import physure
    from physure.application.jit import jit

    assert physure.jit is jit
