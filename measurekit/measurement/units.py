# measurekit/measurement/units.py (Final Version with All Methods)

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from functools import singledispatchmethod
from typing import TYPE_CHECKING, Any, ClassVar, overload

import numpy as np
import sympy as sp

from measurekit.measurement.dimensions import Dimension
from measurekit.notation.lexer import to_superscript
from measurekit.notation.typing import ExponentsDict

if TYPE_CHECKING:
    from numpy.typing import NDArray

    from measurekit.measurement.quantity import Quantity
    from measurekit.system import UnitSystem


@dataclass(frozen=True)
class CompoundUnit:
    _cache: ClassVar[dict[tuple, CompoundUnit]] = {}
    _aliases: ClassVar[dict[tuple, list[str]]] = defaultdict(list)
    _alias_to_exponents: ClassVar[dict[str, tuple]] = {}

    __slots__ = ("exponents",)
    exponents: ExponentsDict

    def __new__(cls, exponents: ExponentsDict) -> CompoundUnit:
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0))
        if key in cls._cache:
            return cls._cache[key]
        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", dict(key))
        cls._cache[key] = instance
        return instance

    def __init__(self, exponents: ExponentsDict) -> None:
        pass

    # --- Arithmetic Methods ---
    def __mul__(self, other: CompoundUnit) -> CompoundUnit:
        result_exponents: ExponentsDict = self.exponents.copy()
        for unit, exp in other.exponents.items():
            result_exponents[unit] = result_exponents.get(unit, 0) + exp
        return CompoundUnit(result_exponents)

    def __truediv__(self, other: CompoundUnit) -> CompoundUnit:
        result = self.exponents.copy()
        for unit, exp in other.exponents.items():
            result[unit] = result.get(unit, 0) - exp
        return CompoundUnit(result)

    def __pow__(self, exponent: float) -> CompoundUnit:
        return CompoundUnit(
            {u: exp * exponent for u, exp in self.exponents.items()}
        )

    # THIS IS THE FIX: Adding the missing __rtruediv__ method
    @singledispatchmethod
    def __rtruediv__(self, other: Any) -> CompoundUnit:
        """Handles reverse division for operations like `1 / unit`."""
        return NotImplemented

    @__rtruediv__.register(int)
    @__rtruediv__.register(float)
    def _(self, other: float) -> Any:
        # This operation is only valid for creating a new unit when dividing by 1
        if other != 1:
            return NotImplemented
        return type(self)({u: -exp for u, exp in self.exponents.items()})

    @classmethod
    def register_alias(cls, exponents: ExponentsDict, *aliases: str) -> None:
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0))
        for alias in aliases:
            if alias not in cls._aliases[key]:
                cls._aliases[key].append(alias)
            cls._alias_to_exponents[alias] = key

    # --- System-Dependent Methods ---
    def conversion_factor_to(
        self, system: UnitSystem, target: CompoundUnit
    ) -> float:
        if self.dimension(system) != target.dimension(system):
            raise ValueError(
                "Incompatible compound unit dimensions for conversion."
            )
        source_factor = self._compound_factor(system)
        target_factor = target._compound_factor(system)
        return source_factor / target_factor

    def _compound_factor(self, system: UnitSystem) -> float:
        factor = 1.0
        for unit, exp in self.exponents.items():
            dim = system.UNIT_DIMENSIONS.get(unit)
            if dim is None:
                raise ValueError(
                    f"Unit '{unit}' not found in system for conversion."
                )
            unit_def = system.UNIT_REGISTRY.get(dim, {}).get(unit)
            if unit_def is None:
                raise ValueError(f"Unit definition for '{unit}' not found.")
            factor *= unit_def.factor_to_base**exp
        return factor

    def dimension(self, system: UnitSystem) -> Dimension:
        overall = Dimension({})
        for unit, exp in self.exponents.items():
            if unit in system.UNIT_DIMENSIONS:
                overall *= system.UNIT_DIMENSIONS[unit] ** exp
            else:
                raise ValueError(
                    f"Unknown dimension for unit '{unit}' in the provided system."
                )
        return overall

    # --- Syntactic Sugar and Representation ---
    @overload
    def __rmul__(self, other: float) -> Quantity[float, float]: ...
    @overload
    def __rmul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...

    def __rmul__(self, other: Any) -> Any:
        from measurekit import default_system
        from measurekit.measurement.quantity import Quantity

        if isinstance(other, (float, int, np.ndarray)):
            return Quantity.from_input(
                value=other, unit=self, system=default_system
            )
        return NotImplemented

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

    def __repr__(self) -> str:
        return f"CompoundUnit({self.exponents!r})"

    def __eq__(self, other: object) -> bool:
        """Checks for equality based on exponents."""
        if not isinstance(other, CompoundUnit):
            return NotImplemented
        return self.exponents == other.exponents

    def __hash__(self) -> int:
        """
        Creates a hash from the immutable representation of the exponents.
        """
        # Convert the dict to a sorted tuple of items, which is hashable
        return hash(tuple(sorted(self.exponents.items())))


def get_unit(unit_expression: str) -> CompoundUnit:
    from measurekit import default_system

    return default_system.get_unit(unit_expression)
