from dataclasses import dataclass
from functools import singledispatchmethod

from notation.lexer import to_superscript
from notation.typing import ExponentsDict


@dataclass(frozen=True)
class BaseExponentEntity:
    exponents: ExponentsDict

    def __new__(cls, exponents: ExponentsDict) -> "BaseExponentEntity":
        normalized = {k: v for k, v in exponents.items() if v}
        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", normalized)
        return instance

    def __init__(self, exponents: ExponentsDict) -> None:
        normalized = {k: v for k, v in exponents.items() if v}
        object.__setattr__(self, "exponents", normalized)

    def __mul__(
        self: "BaseExponentEntity", other: "BaseExponentEntity"
    ) -> "BaseExponentEntity":
        new_exponents = self.exponents.copy()
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) + exp
        return BaseExponentEntity(new_exponents)

    def __truediv__(
        self: "BaseExponentEntity", other: "BaseExponentEntity"
    ) -> "BaseExponentEntity":
        new_exponents = self.exponents.copy()
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) - exp
        return type(self)(new_exponents)

    def __pow__(
        self: "BaseExponentEntity", power: float
    ) -> "BaseExponentEntity":
        return BaseExponentEntity(
            {k: v * power for k, v in self.exponents.items()}
        )

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, BaseExponentEntity):
            return NotImplemented
        return self.exponents == other.exponents

    def __hash__(self) -> int:
        return hash(tuple(sorted(self.exponents.items())))

    def __repr__(self) -> str:
        return str(self.exponents)

    def __str__(self) -> str:
        numerator, denominator = [], []
        for unit, exp in sorted(
            self.exponents.items(), key=lambda x: (-x[1], x[0])
        ):
            formatted = (
                f"{unit}{to_superscript(abs(exp)) if abs(exp) != 1 else ''}"
            )
            (numerator if exp > 0 else denominator).append(formatted)
        n = "·".join(numerator)
        d = "·".join(denominator)
        if "·" in d:
            d = f"({d})"
        if d and n:
            return f"{n}/{d}"
        if d and not n:
            return f"1/{d}"
        if n and not d:
            return n
        return "1"

    @singledispatchmethod
    def __rtruediv__(
        self, other: complex
    ) -> "BaseExponentEntity":
        return type(self)({u: -exp for u, exp in self.exponents.items()})
