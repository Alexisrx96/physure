"""This module provides functionality for handling units in the measurement
system.

It includes the definition and manipulation of compound units, as well as
utility functions for unit conversion and retrieval. The module leverages the
concept of dimensions to ensure compatibility and correctness in unit
operations.

Classes:
- CompoundUnit: Represents a unit composed of various base units raised to
different powers. Provides methods for arithmetic operations, conversion, and
string representation.

Functions:
- get_unit: Retrieves a unit definition based on a string expression.

Imports:
- Various utility functions and classes from related modules for parsing and
  conversions.
"""

from __future__ import annotations

import importlib
import logging
from collections import defaultdict
from dataclasses import dataclass
from functools import singledispatchmethod
from typing import TYPE_CHECKING, Any, ClassVar, Self, cast, overload

import numpy as np
import sympy as sp
from numpy.typing import NDArray

from measurekit.config import config
from measurekit.measurement.conversions import (
    _UNIT_RECIPES,
    UNIT_DIMENSIONS,
    get_compound_unit_conversion_factor,
)
from measurekit.measurement.dimensions import Dimension
from measurekit.notation.lexer import generate_tokens, to_superscript
from measurekit.notation.parsers import NotationParser
from measurekit.notation.typing import ExponentsDict

if TYPE_CHECKING:
    from measurekit.measurement.quantity import Quantity, UncType, ValueType


def should_auto_simplify() -> bool:
    """Reads the configuration to determine if auto-simplification is enabled.

    Returns:
        bool: True if auto-simplification is enabled, False otherwise.
    """
    return config.get_setting("auto_simplify", "true").lower() == "true"  # type: ignore


@dataclass(frozen=True)
class CompoundUnit:
    """Represents a unit composed of various base units raised to different
    powers.

    Provides methods for arithmetic operations, conversion, and string
    representation.
    """

    _cache: ClassVar[dict[tuple, CompoundUnit]] = {}
    _aliases: ClassVar[dict[tuple, list[str]]] = defaultdict(list)
    _alias_to_exponents: ClassVar[dict[str, tuple]] = {}

    __slots__ = ("exponents",)

    exponents: ExponentsDict

    def __new__(cls, exponents: ExponentsDict) -> CompoundUnit:
        """Creates a new CompoundUnit instance with a unique set of
        exponents.
        """
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0))
        if key in cls._cache:
            return cls._cache[key]
        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", dict(key))
        cls._cache[key] = instance
        return instance

    def __init__(self, exponents: ExponentsDict | None = None) -> None:
        """Initializes a CompoundUnit with given exponents."""

    @classmethod
    def register_alias(cls, exponents: ExponentsDict, *aliases: str) -> None:
        """Registers aliases for a given set of exponents."""
        if not aliases:
            return

        # Sort the exponents to create a consistent key
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0))

        # Initialize if not already present
        if key not in cls._aliases:
            cls._aliases[key] = []

        # Add new aliases while preserving existing ones and their order
        for alias in aliases:
            # Check if this alias is already used for a different exponent set
            if (
                alias in cls._alias_to_exponents
                and cls._alias_to_exponents[alias] != key
            ):
                # If already registered with different exponents, skip it
                logging.warning(
                    f"Alias '{alias}' already registered with different"
                    f" exponents. Existing: {cls._alias_to_exponents[alias]}, "
                    f"Attempted: {key}"
                )
                continue

            # Remove the alias if it already exists in the current list
            if alias in cls._aliases[key]:
                cls._aliases[key].remove(alias)

            # Add to the beginning of the list for highest priority
            cls._aliases[key].insert(-1, alias)

            # Update the reverse lookup
            cls._alias_to_exponents[alias] = key

    def to_string(
        self, use_alias: bool = False, alias_preference: str | None = None
    ) -> str:
        """Converts the CompoundUnit to a string representation."""
        # First, check if we should use an alias and if one exists
        if use_alias:
            # Use a tuple of sorted exponents as a key for the aliases
            # dictionary
            key = tuple(
                sorted((k, v) for k, v in self.exponents.items() if v != 0)
            )
            if key in self._aliases and self._aliases[key]:
                aliases = self._aliases[key]
                if alias_preference and alias_preference in aliases:
                    return alias_preference
                # Return the first alias (highest priority)
                return aliases[0]

        # If no alias or if aliases are not to be used, generate the string
        # representation
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
        if d and n:
            return f"{n}/{f'({d})' if '·' in d else d}"
        if d and not n:
            return f"1/{f'({d})' if '·' in d else d}"
        if n and not d:
            return n
        return "1"

    def get_aliases(self) -> list[str]:
        """Returns a list of registered aliases for the current exponents,
        ordered by priority.
        """
        key = tuple(
            sorted((k, v) for k, v in self.exponents.items() if v != 0)
        )
        return self._aliases.get(
            key, []
        ).copy()  # Return a copy to prevent modification

    def simplify(self) -> CompoundUnit:
        """Simplifica la unidad expandiendo todas sus componentes a sus
        unidades base y cancelando términos.
        """
        final_exponents = {}

        for unit_symbol, exponent in self.exponents.items():
            # Buscamos la 'receta' de la unidad.
            # Si no está, es una unidad base.
            recipe = _UNIT_RECIPES.get(unit_symbol)
            if not recipe:
                # Si no hay receta, la unidad es su propia base.
                final_exponents[unit_symbol] = (
                    final_exponents.get(unit_symbol, 0) + exponent
                )
                continue

            # Si hay receta, la expandimos.
            for base_unit, base_exponent in recipe.exponents.items():
                final_exponents[base_unit] = final_exponents.get(
                    base_unit, 0
                ) + (base_exponent * exponent)

        return CompoundUnit(final_exponents)

    def __format__(self, format_spec: str) -> str:
        """Formats the CompoundUnit based on the given specification."""
        if not format_spec or format_spec == "full":
            # Default empty format spec returns full representation
            return self.to_string(use_alias=False)

        parts = format_spec.split(":")
        if parts[0] == "alias":
            alias_preference = parts[1] if len(parts) > 1 else None
            return self.to_string(
                use_alias=True, alias_preference=alias_preference
            )

        # Default to full format for any unrecognized format spec
        return self.to_string(use_alias=False)

    def __str__(self) -> str:
        """Returns a string representation of the CompoundUnit.

        Uses the first registered alias if available, otherwise uses the
        standard exponent representation.
        """
        if config.get_setting("default_output", "plain") == "latex":
            return self.to_latex()
        return self.to_string(use_alias=False)

    def __repr__(self) -> str:
        """Returns a detailed string representation of the CompoundUnit."""
        return f"CompoundUnit({self.exponents!r})"

    def __eq__(self, other: object) -> bool:
        """Checks equality between two CompoundUnit instances."""
        return (
            isinstance(other, CompoundUnit)
            and self.exponents == other.exponents
        )

    def __hash__(self) -> int:
        """Returns a hash value for the CompoundUnit."""
        return hash(tuple(sorted(self.exponents.items())))

    def __mul__(self, other: CompoundUnit) -> CompoundUnit:
        """Multiplies two CompoundUnit instances."""
        result_exponents: ExponentsDict = self.exponents.copy()
        for unit, exp in other.exponents.items():
            result_exponents[unit] = result_exponents.get(unit, 0) + exp

        # Creamos la unidad compuesta resultante
        result_unit = CompoundUnit(result_exponents)

        # --- LÓGICA DE AUTOSIMPLIFICACIÓN ---
        # Leemos la configuración. El 'true' es el valor por defecto si no se
        # encuentra.
        if should_auto_simplify():
            return result_unit.simplify()

        return result_unit

    def __truediv__(
        self, other: CompoundUnit | Quantity[ValueType, UncType]
    ) -> CompoundUnit:
        """Divides the current CompoundUnit by another CompoundUnit or a
        Quantity.
        """
        # --- LÓGICA CORREGIDA ---
        # Si 'other' es una Quantity, la división de unidades solo debe
        # considerar
        # la parte de la unidad de esa Quantity, no su valor.
        # El resultado de dividir una unidad por otra es siempre una nueva
        # unidad.

        other_unit = other if isinstance(other, CompoundUnit) else other.unit

        # Realizamos la operación de división de exponentes.
        result = self.exponents.copy()
        for unit, exp in other_unit.exponents.items():
            result[unit] = result.get(unit, 0) - exp

        new_unit = CompoundUnit(result)

        # La lógica de auto-simplificación que ya teníamos.
        if config.get_setting("auto_simplify", "true").lower() == "true":  # type: ignore
            return new_unit.simplify()

        return new_unit

    def __pow__(self, exponent: float) -> CompoundUnit:
        """Raises the CompoundUnit to the power of exponent."""
        return CompoundUnit(
            {u: exp * exponent for u, exp in self.exponents.items()}
        )

    @property
    def dimension(self) -> Dimension:
        """Calculates the dimension of the CompoundUnit."""
        overall = Dimension({})
        for unit, exp in self.exponents.items():
            if unit in UNIT_DIMENSIONS:
                overall *= UNIT_DIMENSIONS[unit] ** exp
            else:
                raise ValueError(f"Unknown dimension for unit '{unit}'")
        return overall

    def conversion_factor_to(self, target: CompoundUnit) -> float:
        """Calculates the conversion factor to another CompoundUnit."""
        return get_compound_unit_conversion_factor(self, target)

    def convert_value(self, value: float, target: CompoundUnit) -> float:
        """Converts a value to the target CompoundUnit."""
        return value * self.conversion_factor_to(target)

    @singledispatchmethod
    def __rtruediv__(self, other: Any) -> Self:
        return NotImplemented

    @__rtruediv__.register(int)
    @__rtruediv__.register(float)
    def _(self, other: float) -> Self:
        """Inverts the CompoundUnit."""
        return type(self)({u: -exp for u, exp in self.exponents.items()})

    # --- MÉTODO __rmul__ CON SOBRECARGAS EXPLÍCITAS Y PRECISAS ---
    @overload
    def __rmul__(self, other: float) -> Quantity[float, float]: ...
    @overload
    def __rmul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __rmul__(
        self, other: Quantity[ValueType, UncType]
    ) -> Quantity[ValueType, UncType]: ...

    def __rmul__(self, other: Any) -> Any:
        """Implements the reverse multiplication.

        Handles multiplication of a scalar or Quantity by a CompoundUnit.

        - If other is a scalar (float or int), returns a new Quantity with the
        value of other and this CompoundUnit.
        - If other is a Quantity, returns a new Quantity with the value of
        other, but with the unit of the product of other's unit and this
        CompoundUnit.
        - Otherwise, returns NotImplemented.

        The auto-simplification setting is taken into account when other is a
         Quantity.
        """
        Quantity_ = importlib.import_module(
            "measurekit.measurement.quantity"
        ).Quantity

        if isinstance(other, (float, int, np.ndarray)):
            return Quantity_.from_input(value=other, unit=self)

        if isinstance(other, Quantity_):
            new_unit = other.unit * self
            if should_auto_simplify():
                new_unit = new_unit.simplify()
            return Quantity_.from_input(
                value=other.magnitude,
                unit=new_unit,
                uncertainty=other.uncertainty_obj,
            )

        return NotImplemented

    def to_latex(self) -> str:
        r"""Genera una representación en formato LaTeX.

        Utiliza SymPy para la unidad compuesta
        utilizando el motor de renderizado de SymPy para una salida robusta.

        Ejemplos:
        - m -> m
        - m/s -> \frac{m}{s}
        - kg*m/s^2 -> \frac{kg \\cdot m}{s^{2}}
        """
        if not self.exponents:
            return ""

        # 1. Convertir cada nombre de unidad en un Símbolo de SymPy.
        #    Usamos sp.Symbol para que se renderice como una variable
        #    (ej. 'm').
        #    Si quisiéramos que no fuera cursiva, usaríamos sp.Function('m'),
        #    pero el símbolo es más estándar para unidades.
        symbols = {name: sp.Symbol(name) for name in self.exponents}

        # 2. Construir la expresión simbólica completa.
        #    Ej: si exponents es {'kg': 1, 'm': 1, 's': -2},
        #    esto construye la expresión: kg**1 * m**1 * s**-2
        expr = sp.S.One
        for unit_name, exponent in self.exponents.items():
            expr *= symbols[unit_name] ** exponent

        # 3. Usar el potente motor de LaTeX de SymPy para renderizar la
        # expresión.
        #    SymPy se encarga automáticamente de crear la fracción (\frac),
        #    manejar los exponentes y formatear correctamente.
        return sp.latex(expr, mul_symbol="dot")

    def _repr_latex_(self):
        """Método especial para renderizado automático en Jupyter Notebooks."""
        return f"${self.to_latex()}$"

    def is_dimensionless(self) -> bool:
        """Checks if the CompoundUnit is dimensionless.

        A CompoundUnit is considered dimensionless if all its exponents are 0.
        """
        return not self.exponents


# measurekit/measurement/units.py

# ... (otros imports existentes)
from measurekit.measurement.conversions import (
    _UNIT_RECIPES,
    UNIT_DIMENSIONS,
    _CONVERSION_FACTORS,
    PREFIXES,
    UNITS_DATA,
)


def get_unit(unit_expression: str) -> CompoundUnit:
    """
    Analiza una expresión de unidad y devuelve el CompoundUnit correspondiente.

    Esta función primero comprueba si la expresión de la unidad proporcionada ya
    está registrada como un alias. Si es así, devuelve la unidad directamente.

    Si no se encuentra como un alias, la función intenta analizarla como una
    unidad con prefijo (por ejemplo, 'km' para kilómetro). Si detecta una
    combinación válida de un prefijo conocido y una unidad base, registrará
    dinámicamente esta nueva unidad en el sistema para su uso futuro.

    Finalmente, si no es un alias ni una unidad con prefijo detectable, pasa la
    expresión a un parser de notación para manejar unidades compuestas
    (ej. 'm/s^2').

    Args:
        unit_expression (str): La expresión de la unidad a analizar.

    Returns:
        CompoundUnit: La unidad parseada como CompoundUnit.
    """
    # --- LÓGICA EXISTENTE: COMPROBAR SI ES UN ALIAS CONOCIDO ---
    # Esto es rápido y maneja todas las unidades predefinidas.
    if unit_expression in CompoundUnit._alias_to_exponents:
        key = CompoundUnit._alias_to_exponents[unit_expression]
        exponents = dict(key)
        return CompoundUnit(exponents)

    # --- NUEVA LÓGICA DE SOPORTE DE PREFIJOS ---
    # Comprobamos si la unidad ya ha sido registrada dinámicamente antes.
    # La clave `unit_expression` actuaría como el símbolo de la nueva unidad, ej. "km".
    if unit_expression not in _CONVERSION_FACTORS:
        # Ordenamos los prefijos por longitud descendente para que "da" (deca)
        # se compruebe antes que "d" (deci).
        sorted_prefixes = sorted(PREFIXES.keys(), key=len, reverse=True)

        for prefix_symbol in sorted_prefixes:
            if unit_expression.startswith(prefix_symbol) and len(
                unit_expression
            ) > len(prefix_symbol):
                unit_symbol = unit_expression[len(prefix_symbol) :]

                # Ahora buscamos si `unit_symbol` corresponde a una unidad base válida.
                # `UNITS_DATA` debería contener la información de la sección [Units] del .conf
                # incluyendo sus alias y si tienen la bandera 'noprefix'.
                base_unit_name = None
                for name, details in UNITS_DATA.items():
                    # Comprobamos si el símbolo de la unidad o uno de sus alias coincide
                    # y si la unidad permite prefijos.
                    if unit_symbol in details.get(
                        "aliases", []
                    ) and not details.get("noprefix", False):
                        base_unit_name = name
                        break

                if base_unit_name:
                    # ¡Éxito! Hemos encontrado una combinación válida, ej: 'k' y 'meter'.
                    # Ahora registramos la nueva unidad ('km') en el sistema.

                    prefix_factor = PREFIXES[prefix_symbol]["factor"]
                    base_unit_info = UNITS_DATA[base_unit_name]
                    base_unit_symbol = base_unit_info["aliases"][0]  # ej. 'm'

                    # 1. Registrar el factor de conversión a SI
                    # El factor de la nueva unidad es el producto de los factores del prefijo y la unidad base.
                    _CONVERSION_FACTORS[unit_expression] = (
                        prefix_factor * _CONVERSION_FACTORS[base_unit_symbol]
                    )

                    # 2. Registrar la dimensión
                    # La dimensión es la misma que la de la unidad base.
                    UNIT_DIMENSIONS[unit_expression] = UNIT_DIMENSIONS[
                        base_unit_symbol
                    ]

                    # 3. Registrar la "receta" de simplificación
                    # Esto le dice al método `simplify()` que 'km' se descompone en 'm'.
                    _UNIT_RECIPES[unit_expression] = CompoundUnit(
                        {base_unit_symbol: 1}
                    )

                    # 4. Registrar la nueva unidad como un alias de sí misma.
                    # Esto acelera futuras llamadas a get_unit con la misma unidad.
                    CompoundUnit.register_alias(
                        {unit_expression: 1}, unit_expression
                    )

                    # Salimos del bucle una vez que hemos encontrado y registrado el prefijo.
                    break

    # --- LÓGICA EXISTENTE: PARSEAR LA EXPRESIÓN ---
    # Ya sea que la unidad existiera desde el principio, o la acabemos de registrar
    # dinámicamente, ahora el parser la reconocerá sin problemas.
    tokens = generate_tokens(unit_expression)
    parser = NotationParser(tokens, CompoundUnit)
    return cast(CompoundUnit, parser.parse())
