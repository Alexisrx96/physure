"""measurement/conversions.py - Unit conversion and registration system."""

from __future__ import annotations

from collections import defaultdict
from typing import TYPE_CHECKING, Any

from measurement.dimensions import Dimension

if TYPE_CHECKING:
    from measurement.units import CompoundUnit


_UNIT_RECIPES: dict[str, CompoundUnit] = {}

PREFIX_REGISTRY: dict[str, dict[str, Any]] = {}
UNIT_SYMBOL_REGISTRY: dict[str, UnitDefinition] = {}
UNIT_REGISTRY: dict[Dimension, dict[str, UnitDefinition]] = defaultdict(dict)
UNIT_DIMENSIONS: dict[str, Dimension] = {}

_CONVERSION_FACTORS: dict[str, float] = {}

# Almacenará los detalles de los prefijos.
PREFIXES: dict[str, dict[str, Any]] = {}

# Almacenará toda la información de las unidades leída del archivo.
UNITS_DATA: dict[str, dict[str, Any]] = {}


class UnitDefinition:
    """Singleton class representing a unit definition."""

    _instances = {}
    symbol: str
    dimension: Dimension
    factor_to_base: float
    name: str | None
    recipe: CompoundUnit | None
    allow_prefixes: bool

    def __new__(
        cls,
        symbol: str,
        dimension: Dimension,
        factor_to_base: float,
        name: str | None = None,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
    ):
        from .units import get_unit

        # Usamos una clave más simple para el singleton, ya que los otros
        # atributos son descriptivos.
        key = symbol
        if key in cls._instances:
            # Si ya existe, actualizamos sus propiedades por si se redefine.
            instance = cls._instances[key]
            instance.dimension = dimension
            instance.factor_to_base = factor_to_base
            instance.name = name
            instance.recipe = (
                recipe if recipe is not None else get_unit(symbol)
            )
            instance.allow_prefixes = allow_prefixes
            return instance

        instance = super().__new__(cls)
        cls._instances[key] = instance
        return instance

    def __init__(
        self,
        symbol: str,
        dimension: Dimension,
        factor_to_base: float,
        name: str | None = None,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,  # <-- AÑADIR ARGUMENTO AQUÍ TAMBIÉN
    ):
        """Inicializa los atributos de la instancia."""
        self.symbol = symbol
        self.dimension = dimension
        self.factor_to_base = factor_to_base
        self.name = name
        self.recipe = recipe
        self.allow_prefixes = allow_prefixes

    def __str__(self) -> str:
        return f"UnitDefinition({self.symbol}, {self.dimension}, {self.factor_to_base})"

    def __repr__(self) -> str:
        return f"UnitDefinition({self.symbol}, {self.dimension}, {self.factor_to_base}, {self.name})"


def register_prefix(
    symbol: str, factor: float, name: str | None = None
) -> None:
    """Registra un prefijo de unidad en el sistema, incluyendo su símbolo,
    factor y nombre descriptivo."""
    if symbol in PREFIX_REGISTRY:
        print(f"[WARNING] Prefix '{symbol}' is being redefined.")
    # Guardamos el diccionario completo con toda la información del prefijo.
    PREFIX_REGISTRY[symbol] = {"factor": factor, "name": name or symbol}


# --- CAMBIO 3: Añadir la función get_all_prefixes ---
def get_all_prefixes() -> dict[str, dict[str, Any]]:
    """Devuelve el registro completo de prefijos."""
    return PREFIX_REGISTRY


# En tu archivo: measurement/conversions.py


def register_unit(
    symbol: str,
    dimension: Dimension,
    factor_to_base: float,
    name: str | None,
    *aliases: str,
    recipe: CompoundUnit | None = None,
    allow_prefixes: bool = True,
) -> None:
    """Registers a new unit in the system."""
    from .units import CompoundUnit

    # Usa el símbolo directamente.
    normalized_symbol = symbol

    # El resto de la función permanece igual...
    unit_def = UnitDefinition(
        normalized_symbol,
        dimension,
        factor_to_base,
        name,
        recipe=recipe,
        allow_prefixes=allow_prefixes,
    )

    UNIT_REGISTRY[dimension][normalized_symbol] = unit_def
    UNIT_DIMENSIONS[normalized_symbol] = dimension

    for alias in aliases:
        UNIT_SYMBOL_REGISTRY[alias] = unit_def

    if recipe:
        _UNIT_RECIPES[normalized_symbol] = recipe
        for alias in aliases:
            _UNIT_RECIPES[alias] = recipe

    if aliases:
        base_unit_for_aliasing = CompoundUnit({symbol: 1})
        CompoundUnit.register_alias(base_unit_for_aliasing.exponents, *aliases)


def get_conversion_factor(
    dimension: Dimension, from_unit: str, to_unit: str
) -> float:
    """Returns the conversion factor between two units of the same dimension."""
    try:
        return (
            UNIT_REGISTRY[dimension][from_unit].factor_to_base
            / UNIT_REGISTRY[dimension][to_unit].factor_to_base
        )
    except KeyError as exc:
        raise ValueError(
            "Invalid conversion: "
            f"{from_unit} to {to_unit} in dimension {dimension}"
        ) from exc


def find_dimension_for_unit(unit: str) -> Dimension:
    """Finds the dimension of a given unit."""
    if unit in UNIT_DIMENSIONS:
        return UNIT_DIMENSIONS[unit]
    raise ValueError(f"Unit '{unit}' is not registered.")


def compound_factor(compound: CompoundUnit) -> float:
    """Calculates the conversion factor for a compound unit."""
    factor = 1.0
    unit_def = UNIT_REGISTRY.get(compound.dimension, {}).get(str(compound))
    if unit_def is not None:
        return unit_def.factor_to_base
    for unit, exp in compound.exponents.items():
        dim = find_dimension_for_unit(unit)
        unit_def = UNIT_REGISTRY.get(dim, {}).get(unit, None)
        if unit_def is None:
            raise ValueError(f"Unit '{unit}' is not registered for conversion")
        factor *= unit_def.factor_to_base**exp
    return factor


def get_compound_unit_conversion_factor(
    source: CompoundUnit, target: CompoundUnit
) -> float:
    """Calculates the conversion factor between two compound units."""
    if source.dimension != target.dimension:
        raise ValueError(
            f"Incompatible compound unit dimensions. {source} != {target}"
        )
    return compound_factor(source) / compound_factor(target)
