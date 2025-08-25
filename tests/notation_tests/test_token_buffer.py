"""Test suite for the token_buffer module."""
import unittest

from notation.token_buffer import TokenBuffer


class TestTokenBuffer(unittest.TestCase):
    """Tests for the TokenBuffer class."""

    def test_initialization(self):
        """Test initialization of TokenBuffer."""
        # Initialize with a list iterator
        tokens = iter([1, 2, 3])
        buffer = TokenBuffer(tokens)
        
        # Buffer should be empty initially
        self.assertEqual(len(buffer), 0)
        self.assertEqual(str(buffer), "deque([])")
        # Check that the repr starts with the expected prefix
        self.assertTrue(repr(buffer).startswith("TokenBuffer(deque(["))
        
    def test_current_and_getitem(self):
        """Test current() method and __getitem__."""
        tokens = iter([1, 2, 3])
        buffer = TokenBuffer(tokens)
        
        # Current should fetch the first token
        self.assertEqual(buffer.current(), 1)
        
        # Buffer should now contain the fetched token
        self.assertEqual(len(buffer), 1)
        
        # Getting by index should work
        self.assertEqual(buffer[0], 1)
        # This should fetch and buffer the second token
        self.assertEqual(buffer[1], 2)
        # This should fetch and buffer the third token
        self.assertEqual(buffer[2], 3)
        
        # Buffer should now contain all fetched tokens
        self.assertEqual(len(buffer), 3)
        
        # Accessing beyond the available tokens should raise IndexError
        with self.assertRaises(IndexError):
            buffer[3]

    def test_advance(self):
        """Test advance method."""
        tokens = iter([1, 2, 3])
        buffer = TokenBuffer(tokens)
        
        # Make sure we've fetched a token
        self.assertEqual(buffer.current(), 1)
        
        # Advance the buffer
        buffer.advance()
        
        # Current should now be the second token
        self.assertEqual(buffer.current(), 2)
        
        # Advance again
        buffer.advance()
        self.assertEqual(buffer.current(), 3)
        
        # Advance to the end
        buffer.advance()
        
        # Trying to advance an empty buffer should raise IndexError
        with self.assertRaises(IndexError):
            buffer.advance()
        
        # Trying to access beyond available tokens should raise IndexError
        with self.assertRaises(IndexError):
            buffer.current()

    def test_iteration(self):
        """Test iteration over the buffer."""
        tokens = iter([1, 2, 3])
        buffer = TokenBuffer(tokens)
        
        # Buffer is initially empty
        self.assertEqual(list(buffer), [])
        
        # After fetching tokens, the buffer should be iterable
        buffer.current()  # Fetch the first token
        buffer[1]  # Fetch the second token
        self.assertEqual(list(buffer), [1, 2])
        
        # After advancing, the buffer should only contain remaining tokens
        buffer.advance()
        self.assertEqual(list(buffer), [2])

    def test_with_custom_tokens(self):
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
        
        # Current should fetch the first token
        self.assertEqual(buffer.current(), CustomToken(1))
        
        # Buffer should now contain the fetched token
        self.assertEqual(len(buffer), 1)
        
        # Getting by index should work with custom tokens
        self.assertEqual(buffer[0], CustomToken(1))
        self.assertEqual(buffer[1], CustomToken(2))
        
        # Advance should work with custom tokens
        buffer.advance()
        self.assertEqual(buffer.current(), CustomToken(2))

    def test_empty_token_stream(self):
        """Test behavior with an empty token stream."""
        tokens = iter([])
        buffer = TokenBuffer(tokens)
        
        # Accessing an empty stream should raise IndexError
        with self.assertRaises(IndexError):
            buffer.current()
        
        with self.assertRaises(IndexError):
            buffer[0]
        
        # Trying to advance an empty buffer should raise IndexError
        with self.assertRaises(IndexError):
            buffer.advance()


if __name__ == '__main__':
    unittest.main()
