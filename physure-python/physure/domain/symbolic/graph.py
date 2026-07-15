from abc import ABC
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class SymbolicNode(ABC):  # noqa: B024 — marker base class
    """Base class for symbolic expression nodes."""

    pass


@dataclass(frozen=True)
class LeafNode(SymbolicNode):
    """A leaf node representing a quantity with a symbol."""

    symbol: str
    unit_str: str | None = None


@dataclass(frozen=True)
class LiteralNode(SymbolicNode):
    """A node representing a literal value (e.g. constant)."""

    value: Any


@dataclass(frozen=True)
class OpNode(SymbolicNode):
    """An operation node representing a mathematical operation."""

    op_name: str
    args: tuple[SymbolicNode, ...]

    # Optional metadata for affine logic
    is_absolute: bool = False
