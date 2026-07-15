"""TokenBuffer module.

This module contains the TokenBuffer class, which is a class that provides a
way to look ahead in a token stream. It uses a deque to store the tokens and
provides a way to look ahead in the stream by indexing the buffer. It also
provides a way to advance the buffer by one token at a time, which is useful
for parsing.

The buffer is initially empty and is populated by the token stream as tokens
are requested. The buffer is only populated up to the point where the requested
token is, so if the token stream is infinite, the buffer will never contain
more than one token.

"""

from collections import deque
from collections.abc import Iterator
from typing import Generic, TypeVar

T = TypeVar("T")


class TokenBuffer(Generic[T]):
    """TokenBuffer class.

    A TokenBuffer is a class that provides a way to look ahead in a token
    stream. It uses a deque to store the tokens and provides a way to look
    ahead in the streamby indexing the buffer. It also provides a way to
    advance the buffer by one token
    at a time, which is useful for parsing.

    The buffer is initially empty and is populated by the token stream as
    tokens are requested. The buffer is only populated up to the point where
    the requested token is, so if the token stream is infinite, the buffer will
    never contain more than one token.

    :param token_stream: An iterator over the tokens in the stream.
    """

    def __init__(self, token_stream: Iterator[T]):
        """Initialize the TokenBuffer with an empty deque and a token stream.

        :param token_stream: An iterator over the tokens in the stream.
        """
        self.token_stream = token_stream
        self.buffer: deque[T] = deque()

    def __getitem__(self, index: int) -> T:
        """Return the token at the given index.

        If the index is out of range,raise an IndexError.
        """
        while len(self.buffer) <= index:
            try:
                self.buffer.append(next(self.token_stream))
            except StopIteration as exc:
                raise IndexError("TokenBuffer: out of tokens.") from exc
        return self.buffer[index]

    def advance(self) -> None:
        """Advance the buffer by one token."""
        if not self.buffer:
            raise IndexError("TokenBuffer: trying to advance empty buffer.")
        self.buffer.popleft()

    def current(self) -> T:
        """Return the current token."""
        return self[0]

    def __len__(self) -> int:
        """Return the number of tokens in the buffer."""
        return len(self.buffer)

    def __repr__(self) -> str:
        """Return a string representation of the buffer."""
        return f"TokenBuffer({self.buffer!r})"

    def __str__(self) -> str:
        """Return a string representation of the buffer."""
        return str(self.buffer)

    def __iter__(self):
        """Return an iterator over the tokens in the buffer."""
        return iter(self.buffer)
