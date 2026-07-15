"""Test suite for the token_buffer module using pytest."""

import pytest

from physure.domain.notation.token_buffer import TokenBuffer


def test_initialization():
    """Test initialization of TokenBuffer."""
    tokens = iter([1, 2, 3])
    buffer = TokenBuffer(tokens)
    assert len(buffer) == 0
    assert str(buffer) == "deque([])"
    assert repr(buffer).startswith("TokenBuffer(deque([")


def test_current_and_getitem():
    """Test current() method and __getitem__."""
    tokens = iter([1, 2, 3])
    buffer = TokenBuffer(tokens)

    # Current should fetch the first token
    assert buffer.current() == 1
    assert len(buffer) == 1

    # Getting by index should work
    assert buffer[0] == 1
    assert buffer[1] == 2
    assert buffer[2] == 3
    assert len(buffer) == 3

    # Accessing beyond the available tokens should raise IndexError
    with pytest.raises(IndexError):
        _ = buffer[3]


def test_advance():
    """Test advance method."""
    tokens = iter([1, 2, 3])
    buffer = TokenBuffer(tokens)

    assert buffer.current() == 1
    buffer.advance()
    assert buffer.current() == 2
    buffer.advance()
    assert buffer.current() == 3
    buffer.advance()

    with pytest.raises(IndexError):
        buffer.advance()

    with pytest.raises(IndexError):
        buffer.current()


def test_iteration():
    """Test iteration over the buffer."""
    tokens = iter([1, 2, 3])
    buffer = TokenBuffer(tokens)

    assert list(buffer) == []

    buffer.current()
    buffer[1]
    assert list(buffer) == [1, 2]

    buffer.advance()
    assert list(buffer) == [2]


def test_with_custom_tokens():
    """Test TokenBuffer with custom token types."""

    class CustomToken:
        def __init__(self, value):
            self.value = value

        def __eq__(self, other):
            if isinstance(other, CustomToken):
                return self.value == other.value
            return False

        def __repr__(self):
            return f"CustomToken({self.value})"

    tokens = iter([CustomToken(1), CustomToken(2), CustomToken(3)])
    buffer = TokenBuffer(tokens)

    assert buffer.current() == CustomToken(1)
    assert len(buffer) == 1
    assert buffer[0] == CustomToken(1)
    assert buffer[1] == CustomToken(2)

    buffer.advance()
    assert buffer.current() == CustomToken(2)


def test_empty_token_stream():
    """Test behavior with an empty token stream."""
    tokens = iter([])
    buffer = TokenBuffer(tokens)

    with pytest.raises(IndexError):
        buffer.current()

    with pytest.raises(IndexError):
        _ = buffer[0]

    with pytest.raises(IndexError):
        buffer.advance()
