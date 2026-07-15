import pytest

from measurekit import Quantity

try:
    from pydantic import BaseModel, ValidationError

    PYDANTIC_AVAILABLE = True
except ImportError:
    PYDANTIC_AVAILABLE = False


@pytest.mark.skipif(not PYDANTIC_AVAILABLE, reason="Pydantic not installed")
def test_pydantic_validation_success(common_system):
    class Model(BaseModel, arbitrary_types_allowed=True):
        q: Quantity

    # From string
    m1 = Model(q="10 m")
    assert m1.q.magnitude == pytest.approx(10.0)
    assert str(m1.q.unit) == "m"

    # From dict
    m2 = Model(q={"magnitude": 5.5, "unit": "kg"})
    assert m2.q.magnitude == pytest.approx(5.5)
    assert str(m2.q.unit) == "kg"

    # From existing Quantity
    m3 = Model(q=m1.q)
    assert m3.q is m1.q


@pytest.mark.skipif(not PYDANTIC_AVAILABLE, reason="Pydantic not installed")
def test_pydantic_validation_error(common_system):
    class Model(BaseModel, arbitrary_types_allowed=True):
        q: Quantity

    # The SymPy parser rejects invalid strings, causing a ValidationError.
    with pytest.raises(ValidationError):
        Model(q="10 * /")

    with pytest.raises(ValidationError):
        Model(q=10)  # Invalid type (int)


def test_pydantic_schema_metadata():
    """Verify schema method exists regardless of Pydantic."""
    assert hasattr(Quantity, "__get_pydantic_core_schema__")
