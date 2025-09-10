from typing import Protocol

from notation.typing import ExponentsDict


class ExponentEntityProtocol(Protocol):
    @property
    def exponents(self) -> ExponentsDict: ...

    def __init__(self, exponents: ExponentsDict) -> None: ...

    def __mul__(
        self, other: "ExponentEntityProtocol"
    ) -> "ExponentEntityProtocol": ...

    def __truediv__(
        self, other: "ExponentEntityProtocol"
    ) -> "ExponentEntityProtocol": ...

    def __pow__(self, power: float) -> "ExponentEntityProtocol": ...

    def __eq__(self, other: object) -> bool: ...

    def __hash__(self) -> int: ...
