"""Defines the CompoundUnit class for representing and manipulating units.

This module provides the CompoundUnit class, which is used to represent
any physical unit as a combination of base units raised to specific powers
(e.g., meters per second as m^1 * s^-1). It supports arithmetic operations,
alias registration, and various string formatting options, including LaTeX.
"""

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
    """Represents a unit composed of base units raised to various powers.

    This class is immutable and uses a caching mechanism to ensure that
    identical units are represented by the same object instance. It provides
    methods for arithmetic operations, unit conversions, and dimensional
    analysis within a given unit system.

    Attributes:
    exponents (ExponentsDict): A dictionary mapping base unit symbols to their
    floating-point exponents.
    """

    _cache: ClassVar[dict[tuple, CompoundUnit]] = {}
    _aliases: ClassVar[dict[tuple, list[str]]] = defaultdict(list)
    _alias_to_exponents: ClassVar[dict[str, tuple]] = {}

    __slots__ = ("exponents",)
    exponents: ExponentsDict

    def __new__(cls, exponents: ExponentsDict):
        """Create or retrieve a cached CompoundUnit instance.

        Ensures that only one instance exists for each unique combination of
        unit exponents, promoting memory efficiency and enabling direct object
        comparisons.

        Args:
        exponents (ExponentsDict): A dictionary of unit symbols and their
        exponents.

        Returns:
        CompoundUnit: A new or cached instance of the class.
        """
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0))
        if key in cls._cache:
            return cls._cache[key]
        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", dict(key))
        cls._cache[key] = instance
        return instance

    def __init__(self, exponents: ExponentsDict) -> None:
        """Initializes a new CompoundUnit instance.

        Args:
        exponents (ExponentsDict): A dictionary of unit symbols and their
        exponents.
        """
        pass

    # --- Arithmetic Methods ---
    def __mul__(self, other: CompoundUnit) -> CompoundUnit:
        """Multiply two CompoundUnit instances by adding their exponents.

        Args:
        other (CompoundUnit): The unit to multiply with.

        Returns:
        CompoundUnit: A new unit representing the product.
        """
        result_exponents: ExponentsDict = self.exponents.copy()
        for unit, exp in other.exponents.items():
            result_exponents[unit] = result_exponents.get(unit, 0) + exp
        return CompoundUnit(result_exponents)

    def __truediv__(self, other: CompoundUnit) -> CompoundUnit:
        """Divide one CompoundUnit by another by subtracting exponents.

        Args:
        other (CompoundUnit): The unit to divide by.

        Returns:
        CompoundUnit: A new unit representing the result of the division.
        """
        result = self.exponents.copy()
        for unit, exp in other.exponents.items():
            result[unit] = result.get(unit, 0) - exp
        return CompoundUnit(result)

    def __pow__(self, exponent: float) -> CompoundUnit:
        """Raise a CompoundUnit to a power by multiplying its exponents.

        Args:
        exponent (float): The power to raise the unit to.

        Returns:
        CompoundUnit: A new unit representing the result.
        """
        return CompoundUnit(
            {u: exp * exponent for u, exp in self.exponents.items()}
        )

    @singledispatchmethod
    def __rtruediv__(self, other: Any) -> CompoundUnit:
        """Handle reverse division to create an inverse unit (e.g., 1 / unit).

        Args:
        other (Any): The numerator, typically the number 1.

        Returns:
        CompoundUnit: A new unit with all exponents negated.
        """
        return NotImplemented

    @__rtruediv__.register(int)
    @__rtruediv__.register(float)
    def _(self, other: float) -> Any:
        if other != 1:
            return NotImplemented
        return type(self)({u: -exp for u, exp in self.exponents.items()})

    @classmethod
    def register_alias(cls, exponents: ExponentsDict, *aliases: str) -> None:
        """Register one or more aliases for a specific set of unit exponents.

        This class method allows alternative names (like 'velocity' for 'm/s')
        to be associated with a unit's structure.

        Args:
        exponents (ExponentsDict): The unit structure to alias.
        *aliases (str): A variable number of alias strings.
        """
        key = tuple(sorted((k, v) for k, v in exponents.items() if v != 0))
        for alias in aliases:
            if alias not in cls._aliases[key]:
                cls._aliases[key].append(alias)
            cls._alias_to_exponents[alias] = key

    # --- System-Dependent Methods ---
    def conversion_factor_to(
        self, system: UnitSystem, target: CompoundUnit
    ) -> float:
        """Calculate the conversion factor to a target unit within a system.

        Args:
        system (UnitSystem): The unit system providing conversion definitions.
        target (CompoundUnit): The unit to convert to.

        Returns:
        float: The numerical factor to multiply by to convert to the target
        unit.

        Raises:
        ValueError: If the units have incompatible dimensions.
        """
        if self.dimension(system) != target.dimension(system):
            raise ValueError(
                "Incompatible compound unit dimensions for conversion."
            )
        source_factor = self._compound_factor(system)
        target_factor = target._compound_factor(system)
        return source_factor / target_factor

    def _compound_factor(self, system: UnitSystem) -> float:
        """Calculate the unit's total conversion factor relative to SI units.

        This is a helper method used for conversions.

        Args:
        system (UnitSystem): The unit system providing conversion definitions.

        Returns:
        float: The unit's conversion factor.

        Raises:
        ValueError: If any base unit in the composition is not found in the
        system.
        """
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
        """Determine the physical dimension of the unit within a system.

        Args:
        system (UnitSystem): The unit system that defines the dimensions of
        base units.

        Returns:
        Dimension: The resulting physical dimension of the compound unit.

        Raises:
        ValueError: If any base unit in the composition is not found in the
        system.
        """
        overall = Dimension({})
        for unit, exp in self.exponents.items():
            if unit in system.UNIT_DIMENSIONS:
                overall *= system.UNIT_DIMENSIONS[unit] ** exp
            else:
                raise ValueError(
                    f"Unknown dimension for unit '{unit}'"
                    " in the provided system."
                )
        return overall

    @overload
    def __rmul__(self, other: float) -> Quantity[float, float]: ...
    @overload
    def __rmul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...

    def __rmul__(self, other: Any) -> Any:
        """Handle right-side multiplication, typically for creating a Quantity.

        This allows for intuitive syntax like 5 * meter.

        Args:
        other (Any): The scalar or array to be multiplied with the unit.

        Returns:
        Any: A new Quantity instance, or NotImplemented if the operation is
        not supported.
        """
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
        """Generate a human-readable string representation of the unit.

        Args:
        use_alias (bool, optional): If True, uses a registered alias if one
        exists. Defaults to False.
        alias_preference (str | None, optional): A preferred alias to use if
        multiple exist. Defaults to None.

        Returns:
        str: The string representation of the unit.
        """
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
        """Format the CompoundUnit using a format specification.

        Supports specifications like 'full' for the complete unit string or
        'alias' to use a registered alias.

        Args:
        format_spec (str): The string specifying how to format the unit.

        Returns:
        str: The formatted unit string.
        """
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
        r"""Generate a LaTeX representation of the unit for display.

        This method uses SymPy to produce a properly formatted LaTeX string,
        handling fractions and exponents correctly.

        Examples:
        - m/s becomes \frac{m}{s}
        - kg*m/s^2 becomes \frac{kg \cdot m}{s^{2}}

        Returns:
        str: The LaTeX formatted string.
        """
        if not self.exponents:
            return ""

        # 1. Convert each unit name into a SymPy symbol.
        #    We use sp.Symbol so that it renders as a variable (e.g., 'm').
        #    If we wanted to avoid the italic font, we could use sp.Function('m'),
        #    but the symbol is more standard for units.
        symbols = {name: sp.Symbol(name) for name in self.exponents}

        # 2. Build the complete symbolic expression.
        #    Ej: if exponents is {'kg': 1, 'm': 1, 's': -2},
        #    this builds the expression: kg**1 * m**1 * s**-2
        expr = sp.S.One
        for unit_name, exponent in self.exponents.items():
            expr *= symbols[unit_name] ** exponent

        # 3. Use SymPy's powerful LaTeX rendering engine to render the
        # expression.
        #    SymPy automatically takes care of creating the fraction (\frac),
        #    handling exponents and formatting correctly.
        return sp.latex(expr, mul_symbol="dot")

    def _repr_latex_(self):
        """Provide a LaTeX representation for automatic rendering in Jupyter.

        Returns:
        str: The LaTeX string wrapped in '$' for display.
        """
        return f"${self.to_latex()}$"

    def __repr__(self) -> str:
        """Return an unambiguous, developer-focused string representation.

        This representation shows the exact internal structure of the object,
        which is useful for debugging.

        Returns:
        str: The string CompoundUnit({...}) showing the exponents dict.
        """
        return f"CompoundUnit({self.exponents!r})"

    def __eq__(self, other: object) -> bool:
        """Check for equality with another CompoundUnit.

        Two units are considered equal if their normalized exponent
        dictionaries are identical.

        Args:
        other (object): The object to compare against.

        Returns:
        bool: True if the units are equal, False otherwise.
        """
        if not isinstance(other, CompoundUnit):
            return NotImplemented
        return self.exponents == other.exponents

    def __hash__(self) -> int:
        """Compute a hash based on the unit's exponents.

        This allows CompoundUnit objects to be used in hash-based collections
        like sets and dictionary keys.

        Returns:
        int: The hash value.
        """
        # Convert the dict to a sorted tuple of items, which is hashable
        return hash(tuple(sorted(self.exponents.items())))

    def is_dimensionless(self) -> bool:
        """Check if the unit is dimensionless (i.e., has no components).

        Returns:
        bool: True if the unit is dimensionless, False otherwise.
        """
        return not self.exponents


def get_unit(unit_expression: str) -> CompoundUnit:
    """Parse a string expression and return the corresponding CompoundUnit.

    This function acts as the primary factory for creating unit objects from
    strings. It delegates the parsing logic to the currently active default
    unit system.

    Args:
    unit_expression (str): The string to parse (e.g., "m/s", "kilometer").

    Returns:
    CompoundUnit: The corresponding unit object.
    """
    from measurekit import default_system

    return default_system.get_unit(unit_expression)
