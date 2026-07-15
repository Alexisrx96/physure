"""Mandatory native Rust parser for symbolic unit and dimension expressions."""

from __future__ import annotations

from typing import TYPE_CHECKING

from physure._core import parse_unit_expression as _rust_parse_unit_expr

if TYPE_CHECKING:
    from physure.domain.notation.protocols import ExponentEntityProtocol


def parse_unit_expression(expr: str) -> ExponentEntityProtocol:
    """Parses a unit expression string using the mandatory Rust core parser."""
    return _rust_parse_unit_expr(expr)
