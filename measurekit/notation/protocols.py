"""Protocols for symbolic entities with exponents.

Defines the ExponentEntityProtocol which specifies the required interface
for entities with exponents.
"""

from __future__ import annotations

from typing import Protocol

from measurekit.notation.typing import ExponentsDict


class ExponentEntityProtocol(Protocol):
    @property
    def exponents(self) -> ExponentsDict: ...

    def __init__(self, exponents: ExponentsDict) -> None: ...

    def __mul__(
        self, other: ExponentEntityProtocol
    ) -> ExponentEntityProtocol: ...

    def __truediv__(
        self, other: ExponentEntityProtocol
    ) -> ExponentEntityProtocol: ...

    def __pow__(self, power: float) -> ExponentEntityProtocol: ...

    def __eq__(self, other: object) -> bool: ...

    def __hash__(self) -> int: ...
