from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar, cast

from notation.lexer import generate_tokens, to_superscript
from notation.parsers import (
    NotationParser,
)
from notation.typing import ExponentsDict

_DIMENSION_NAME_REGISTRY: dict[Dimension | None, str] = {}

DIMENSIONLESS = None
_PREFIX_BLOCKLIST: set[str] = set()


def block_prefixes_for_dimension_symbol(symbol: str) -> None:
    """Añade un símbolo de dimensión a la lista de bloqueo de prefijos."""
    _PREFIX_BLOCKLIST.add(symbol)


def prefixes_allowed_for_dimension_symbol(symbol: str) -> bool:
    """Comprueba si a un símbolo de dimensión se le pueden aplicar prefijos."""
    return symbol not in _PREFIX_BLOCKLIST


def register_dimension(dimension: Dimension, name: str):
    """Registra un nombre descriptivo para una instancia de Dimension.

    Esta función poblará el registro central para que las dimensiones
    pueden ser representadas de forma más legible.

    Args:
        dimension: El objeto Dimension a nombrar (e.g., Dimension({'L': 1})).
        name: El nombre legible por humanos (e.g., "Length").
    """
    _DIMENSION_NAME_REGISTRY[dimension] = name


@dataclass(frozen=True)
class Dimension:
    _cache: ClassVar[dict[tuple, Dimension]] = {}
    _base_dimensions: ClassVar[list[str]] = []
    exponents: ExponentsDict

    __slots__ = (
        "exponents",
        "_analytical_representation",
        "_display_exponents",
    )

    def __new__(cls, exponents: ExponentsDict) -> Dimension:
        normalized = {k: float(v) for k, v in exponents.items() if v != 0}
        key = tuple(sorted(normalized.items()))
        if key in cls._cache:
            return cls._cache[key]

        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", normalized)

        # Pre-cálculo de la representación analítica
        if not normalized:
            analytical_rep = "Dimensionless"
        else:
            parts = []
            for k, exp in sorted(normalized.items()):
                display_exp = int(exp) if exp == int(exp) else exp
                if display_exp == 1:
                    parts.append(k)
                else:
                    parts.append(f"{k}{to_superscript(display_exp)}")
            analytical_rep = "·".join(parts)

        # Pre-cálculo del diccionario de exponentes para display
        display_exp_dict = {
            k: int(v) if v == int(v) else v for k, v in normalized.items()
        }

        object.__setattr__(
            instance, "_analytical_representation", analytical_rep
        )
        object.__setattr__(instance, "_display_exponents", display_exp_dict)

        cls._cache[key] = instance
        return instance

    def __init__(self, exponents: ExponentsDict) -> None:
        pass

    def __mul__(self, other: Dimension) -> Dimension:
        new_exponents = {**self.exponents}
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) + exp
        return Dimension(new_exponents)

    def __truediv__(self, other: Dimension) -> Dimension:
        new_exponents = {**self.exponents}
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) - exp
        return Dimension(new_exponents)

    def __pow__(self, power: float) -> Dimension:
        return Dimension({k: v * power for k, v in self.exponents.items()})

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Dimension):
            return NotImplemented
        return self.exponents == other.exponents

    def __hash__(self) -> int:
        return hash(tuple(sorted(self.exponents.items())))

    def __rtruediv__(self, other: complex) -> Dimension:
        """Allows division where a scalar is divided by a Dimension (inverse)."""
        return Dimension({k: -v for k, v in self.exponents.items()})

    @property
    def analytical_representation(self) -> str:
        """Devuelve la descripción analítica pre-calculada de la dimensión."""
        return self._analytical_representation

    @property
    def name(self) -> str | None:
        """Devuelve el nombre descriptivo registrado para la dimensión."""
        return _DIMENSION_NAME_REGISTRY.get(self)

    def __str__(self) -> str:
        """La representación principal de la dimensión es su forma analítica."""
        return self.analytical_representation

    def __repr__(self) -> str:
        """Representación detallada para debugging con valores pre-calculados."""
        registered_name = self.name
        if registered_name:
            return (
                f"<Dimension: {self.analytical_representation} "
                f"({registered_name}) {self._display_exponents}>"
            )
        return f"<Dimension: {self.analytical_representation} {self._display_exponents}>"

    def is_dimensionless(self) -> bool:
        """Checks if the dimension is dimensionless."""
        return not self.exponents

    @classmethod
    def set_base_dimensions(cls, bases: list[str]):
        """Establece las dimensiones base que el sistema reconocerá.

        Este método debe ser llamado durante la inicialización del sistema.

        Args:
            bases (list[str]): Una lista de los símbolos de las dimensiones
            base.
                               Ej: ['L', 'M', 'T', 'I', 'O', 'J', 'N']
        """
        cls._base_dimensions = bases

    @classmethod
    def from_string(cls, dim_str: str) -> Dimension:
        """Crea un objeto Dimension a partir de una cadena de texto.

        Args:
            dim_str (str): La cadena que representa la dimensión.
                           Ej: "L", "T", "L·T⁻²", "M*L^2 / T^2"

        Returns:
            Dimension: Un nuevo objeto Dimension correspondiente.
        """
        tokens = generate_tokens(dim_str)

        parser = NotationParser(tokens, cls)  # type: ignore

        parsed_dim = parser.parse()
        return cast(Dimension, parsed_dim)


def get_dimension(unit_expression: str) -> Dimension:
    tokens = generate_tokens(unit_expression)
    parser = NotationParser(tokens, Dimension)
    return cast(Dimension, parser.parse())
