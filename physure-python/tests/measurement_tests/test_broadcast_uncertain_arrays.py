"""Operations between uncertain arrays of different shapes (broadcasting), against the GUM (JCGM 100:2008, 5.1).

When an operand is broadcast, its elements are copies of each other: every result element that came from the
same operand element carries the same error, so they are fully correlated. The expected values below are the
GUM formulas written out for the broadcast shapes; the correlation is checked by combining a result with an
operand again, where the shared error has to cancel: ``(a * b) / b`` has exactly the uncertainty of ``a``.
"""

import numpy as np
import pytest

from physure import Q_

torch = pytest.importorskip("torch")

BACKENDS = ["numpy", "torch"]
# (shape of a, shape of b): the result has the shape numpy broadcasts them to
PAIRS = [
    ((4, 5), (5,)),
    ((5,), (4, 5)),
    ((4, 1), (1, 5)),
    ((3,), (1,)),
    ((1,), (3,)),
    ((2, 3), (2, 3)),
    ((4, 5), ()),
    ((), (4, 5)),
]


def operands(shape_a, shape_b, lib):
    rng = np.random.default_rng(7)
    a, b = rng.random(shape_a) + 1.0, rng.random(shape_b) + 1.0
    ua, ub = 0.05 * a, 0.03 * b
    wrap = torch.as_tensor if lib == "torch" else (lambda x: x)

    def quantity(value, uncertainty, unit):
        # a 0-d value is a Python float on both backends: that is how a scalar quantity is written
        if np.ndim(value) == 0:
            return Q_(float(value), unit, float(uncertainty))
        return Q_(wrap(value), unit, wrap(uncertainty))

    return (a, b, ua, ub), (
        quantity(a, ua, "m"),
        quantity(b, ub, "m"),
        quantity(b, ub, "s"),
    )


def as_numpy(x):
    return (
        x.detach().numpy()
        if hasattr(x, "detach")
        else np.asarray(x, dtype=float)
    )


def broadcast(*arrays):
    return np.broadcast_arrays(*arrays)


EXPECTED = {
    "add": lambda a, b, ua, ub: np.sqrt(ua**2 + ub**2),
    "sub": lambda a, b, ua, ub: np.sqrt(ua**2 + ub**2),
    "mul": lambda a, b, ua, ub: np.sqrt((b * ua) ** 2 + (a * ub) ** 2),
    "div": lambda a, b, ua, ub: np.sqrt((ua / b) ** 2 + (a * ub / b**2) ** 2),
}


@pytest.mark.parametrize("lib", BACKENDS)
@pytest.mark.parametrize("shapes", PAIRS, ids=str)
@pytest.mark.parametrize("op", list(EXPECTED))
def test_broadcast_standard_uncertainty_follows_the_gum(op, shapes, lib):
    (a, b, ua, ub), (qa, qb_same_unit, qb_other_unit) = operands(*shapes, lib)
    right = qb_same_unit if op in ("add", "sub") else qb_other_unit
    result = {
        "add": lambda: qa + right,
        "sub": lambda: qa - right,
        "mul": lambda: qa * right,
        "div": lambda: qa / right,
    }[op]()
    a_, b_, ua_, ub_ = broadcast(a, b, ua, ub)
    got = as_numpy(result.uncertainty).reshape(a_.shape)
    np.testing.assert_allclose(
        got, EXPECTED[op](a_, b_, ua_, ub_), rtol=1e-9, atol=1e-12
    )
    np.testing.assert_allclose(
        as_numpy(result.magnitude).reshape(a_.shape),
        {"add": a_ + b_, "sub": a_ - b_, "mul": a_ * b_, "div": a_ / b_}[op],
    )


@pytest.mark.parametrize("lib", BACKENDS)
@pytest.mark.parametrize(
    "shapes",
    [((4, 5), (5,)), ((4, 1), (1, 5)), ((3,), (1,)), ((4, 5), ())],
    ids=str,
)
def test_a_broadcast_operand_is_one_error_shared_by_its_copies(shapes, lib):
    """The error of ``b`` is common to every copy of it, so dividing it out again leaves ``a`` alone."""
    (_, _, ua, _), (qa, _, qb) = operands(*shapes, lib)
    ua_full = np.broadcast_to(ua, np.broadcast_shapes(*shapes))
    by_multiplying = (qa * qb) / qb
    np.testing.assert_allclose(
        as_numpy(by_multiplying.uncertainty).reshape(ua_full.shape),
        ua_full,
        rtol=1e-9,
        atol=1e-12,
    )
    by_dividing = (qa / qb) * qb
    np.testing.assert_allclose(
        as_numpy(by_dividing.uncertainty).reshape(ua_full.shape),
        ua_full,
        rtol=1e-9,
        atol=1e-12,
    )


@pytest.mark.parametrize("lib", BACKENDS)
def test_a_small_uncertain_operand_times_a_plain_array_is_scaled_by_it(lib):
    """An uncertain ``b`` of shape (5,) times an array without uncertainty of shape (4, 5): ``sigma = |array| * ub``."""
    rng = np.random.default_rng(9)
    b, plain = rng.random(5) + 1.0, rng.random((4, 5)) + 1.0
    ub = 0.03 * b
    wrap = torch.as_tensor if lib == "torch" else (lambda x: x)
    qb = Q_(wrap(b), "s", wrap(ub))
    for result, expected in (
        (qb * wrap(plain), np.abs(plain) * ub),
        (qb / wrap(plain), ub / np.abs(plain)),
    ):
        np.testing.assert_allclose(
            as_numpy(result.uncertainty).reshape(plain.shape),
            expected,
            rtol=1e-9,
        )


def test_an_unsupported_combination_is_a_clean_error_not_a_rust_panic():
    """What stays unsupported must be an ordinary exception: a Rust panic is a BaseException."""
    a = Q_(np.ones((3, 4)), "m", np.full((3, 4), 0.1))
    b = Q_(np.ones((2, 4)), "s", np.full((2, 4), 0.1))  # not broadcastable
    with pytest.raises(ValueError, match="broadcast"):
        a * b
