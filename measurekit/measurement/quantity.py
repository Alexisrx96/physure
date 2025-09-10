from __future__ import annotations

import math
import operator
from dataclasses import dataclass
from fractions import Fraction
from typing import Any, ClassVar, Generic, Self, TypeVar, cast, overload

import numpy as np
import sympy as sp
from numpy.typing import NDArray

from measurekit.config import config
from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.uncertainty import Uncertainty
from measurekit.measurement.units import CompoundUnit, get_unit

OtherValueType = TypeVar("OtherValueType", float, int, NDArray[Any])
ValueType = TypeVar(
    "ValueType",
    float,
    int,
    NDArray[Any],
)

UncType = TypeVar("UncType", float, NDArray[Any])

Numeric = int | float | NDArray[Any]


@dataclass(frozen=True)
class Quantity(Generic[ValueType, UncType]):
    """Represents a physical quantity with magnitude, unit, and uncertainty.

    The `Quantity` class encapsulates a value (scalar or array), its associated
    unit, and the uncertainty of the measurekit.measurement. It supports arithmetic
    operations, unit conversions, formatting, and propagation of uncertainties
    according to physical rules.

    Parameters
    ----------
    magnitude : ValueType
        The numerical value of the quantity. Can be a scalar (int, float) or a
        NumPy array.
    unit : CompoundUnit
        The unit of measurement, supporting compound and derived units.
    uncertainty_obj : Uncertainty[UncType]
        The uncertainty object representing the standard deviation
        (absolute uncertainty).
    fraction : Fraction or None, optional
        Fractional representation of the value, if applicable.
    dimension : Dimension
        The physical dimension (e.g., length, mass, time) associated with the
        unit.

    Class Variables:
    --------------
    _cache : dict[CompoundUnit, type]
        Internal cache for unit-specific Quantity subclasses.

    Methods:
    -------
    from_input(value, unit, uncertainty=0.0)
        Constructs a Quantity from raw input, preserving generic types and
        handling scalars or arrays, and uncertainty propagation.

    to(target_unit)
        Converts the quantity to a new unit, adjusting magnitude and
        uncertainty.

    Arithmetic Operations
    --------------------
    Supports addition, subtraction, multiplication, division, power, and their
    reflected counterparts, with correct unit and uncertainty propagation.

    Comparison Operations
    ---------------------
    Supports equality and ordering comparisons, enforcing dimension
    consistency.

    Formatting & Representation
    --------------------------
    __format__(format_spec)
        Flexible formatting for numeric and unit representation, including
        LaTeX output.
    to_latex()
        Returns a LaTeX string representation of the quantity.
    __str__(), __repr__()
        Human-readable and developer representations.

    NumPy Integration
    ----------------
    __array_ufunc__(ufunc, method, *inputs, **kwargs)
        Enables NumPy universal function support, including reduction and
        element-wise operations, with correct unit and uncertainty handling.

    Vector Operations
    ----------------
    dot(other)
        Computes the dot product of two quantities.
    cross(other)
        Computes the cross product of two quantities.

    Indexing & Length
    -----------------
    __getitem__(key)
        Supports array-like indexing for quantities with array values.
    __len__()
        Returns the length of the underlying array value.

    Other Methods
    -------------
    __neg__(), __pos__(), __abs__()
        Unary operations.
    __float__()
        Converts the quantity to a float (if possible).
    __trunc__(), __floor__(), __ceil__(), __round__(ndigits)
        Rounding and truncation operations, compatible with arrays.

    Notes:
    -----
    - All arithmetic and comparison operations enforce dimension consistency.
    - Uncertainty propagation follows standard physical rules for each
    operation.
    - Supports both scalar and array quantities, with correct broadcasting and
    propagation.
    - Formatting supports plain, fractional, and LaTeX representations.

    Examples:
    --------
    >>> q1 = Quantity.from_input(5.0, meter, uncertainty=0.1)
    >>> q2 = Quantity.from_input([1, 2, 3], meter)
    >>> q3 = q1 + q1
    >>> q4 = q2 * 2
    >>> print(q1.to("cm"))
    >>> print(f"{q1:.2f|alias}")
    >>> print(q1.to_latex())
    """

    magnitude: ValueType
    unit: CompoundUnit
    uncertainty_obj: Uncertainty[UncType]
    fraction: Fraction | None
    dimension: Dimension

    # --- Variables de Clase ---
    _cache: ClassVar[dict[CompoundUnit, type]] = {}

    # --- __slots__ ahora usa los nombres públicos ---
    __slots__ = (
        "magnitude",
        "unit",
        "uncertainty_obj",
        "fraction",
        "dimension",
    )

    @classmethod
    def from_input(
        cls,
        value: Any,
        unit: CompoundUnit,
        uncertainty: Any = 0.0,
        # ) -> Any:
    ) -> Self:
        """Método constructor que procesa las entradas y PRESERVA los tipos
        genéricos.
        """
        if isinstance(value, Quantity):
            val, u, uncertainty_processed = (
                value.magnitude,
                value.unit,
                value.uncertainty_obj,
            )
        else:
            val, u = value, unit
            uncertainty_processed = (
                uncertainty
                if isinstance(uncertainty, Uncertainty)
                else Uncertainty(uncertainty)
            )

        uncertainty_obj_processed = (
            uncertainty_processed
            if isinstance(uncertainty_processed, Uncertainty)
            else Uncertainty(uncertainty_processed)
        )

        val_is_arr = isinstance(val, np.ndarray)
        unc_is_arr = isinstance(uncertainty_obj_processed.std_dev, np.ndarray)

        if (
            val_is_arr
            and not unc_is_arr
            and uncertainty_obj_processed.std_dev != 0.0
        ):
            unc_array = np.full_like(
                val, uncertainty_obj_processed.std_dev, dtype=float
            )
            uncertainty_obj_processed = Uncertainty(unc_array)
        elif val_is_arr != unc_is_arr and not (
            not unc_is_arr and uncertainty_obj_processed.std_dev == 0.0
        ):
            raise TypeError(
                "El tipo de 'value' y 'uncertainty' debe ser consistente."
            )

        if isinstance(val, np.ndarray):
            val.flags.writeable = False
        if isinstance(uncertainty_obj_processed.std_dev, np.ndarray):
            uncertainty_obj_processed.std_dev.flags.writeable = False

        frac = Fraction(str(val)) if np.isscalar(val) else None
        dim = u.dimension

        return cls(
            magnitude=cast(ValueType, val),
            unit=u,
            uncertainty_obj=cast(
                Uncertainty[UncType], uncertainty_obj_processed
            ),
            fraction=frac,
            dimension=dim,
        )

    @classmethod
    def __class_getitem__(cls, item: CompoundUnit) -> type:
        if item in cls._cache:
            return cls._cache[item]
        new_cls = type(
            f"{cls.__name__}[{item}]", (cls,), {"default_unit": item}
        )
        cls._cache[item] = new_cls
        return new_cls

    @property
    def uncertainty(self) -> UncType:
        """La desviación estándar (incertidumbre absoluta), manteniendo el tipo."""
        return self.uncertainty_obj.std_dev

    def to(
        self, target_unit: CompoundUnit | str
    ) -> Quantity[ValueType, UncType]:
        """Convierte la cantidad a una nueva unidad."""
        if not isinstance(self.unit, CompoundUnit):
            raise TypeError(
                "Conversion is only supported for CompoundUnit types"
            )
        if isinstance(target_unit, str):
            target_unit = get_unit(target_unit)

        conversion_factor = self.unit.conversion_factor_to(target_unit)
        new_value = self.magnitude * conversion_factor
        new_uncertainty = self.uncertainty * conversion_factor
        return Quantity.from_input(
            new_value, target_unit, uncertainty=new_uncertainty
        )

    @overload
    def __add__(
        self: Quantity[NDArray[Any], Any], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __add__(
        self: Quantity[Any, NDArray[Any]], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __add__(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __add__(
        self, other: Quantity[Any, NDArray[Any]]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __add__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...

    @overload
    def __add__(
        self: Quantity[float, Any], other: Any
    ) -> Quantity[float, float]: ...
    @overload
    def __add__(
        self, other: Quantity[float, Any]
    ) -> Quantity[float, float]: ...
    @overload
    def __add__(self, other: float) -> Quantity[float, float]: ...

    @overload
    def __add__(
        self: Quantity[int, float], other: Quantity[int, float]
    ) -> Quantity[int, float]: ...

    def __add__(self, other: Any) -> Any:  # noqa: D105
        # La implementación real es genérica y maneja todos los casos.
        if isinstance(other, Quantity):
            if self.dimension != other.dimension:
                raise ValueError(
                    "No se pueden sumar cantidades con diferentes dimensiones"
                )
            other_converted = other.to(self.unit)
            new_value = self.magnitude + other_converted.magnitude
            new_uncertainty_obj = self.uncertainty_obj.add(
                other_converted.uncertainty_obj
            )
            return Quantity.from_input(
                new_value, self.unit, uncertainty=new_uncertainty_obj
            )
        if isinstance(other, (int, float, np.ndarray)):
            if not self.unit.is_dimensionless():
                raise ValueError(
                    "No se puede sumar un escalar a una cantidad con dimensiones."
                )
            new_value = self.magnitude + other
            return Quantity.from_input(
                new_value, self.unit, uncertainty=self.uncertainty_obj
            )
        return NotImplemented

    # (La resta sigue exactamente el mismo patrón de sobrecargas explícitas)
    @overload
    def __sub__(
        self: Quantity[NDArray[Any], Any], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __sub__(
        self: Quantity[Any, NDArray[Any]], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __sub__(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __sub__(
        self, other: Quantity[Any, NDArray[Any]]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __sub__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __sub__(
        self: Quantity[float, Any], other: Any
    ) -> Quantity[float, float]: ...
    @overload
    def __sub__(
        self, other: Quantity[float, Any]
    ) -> Quantity[float, float]: ...
    @overload
    def __sub__(self, other: float) -> Quantity[float, float]: ...
    @overload
    def __sub__(
        self: Quantity[int, float], other: Quantity[int, float]
    ) -> Quantity[int, float]: ...

    def __sub__(self, other: Any) -> Any:  # noqa: D105
        if isinstance(other, Quantity):
            if self.dimension != other.dimension:
                raise ValueError(
                    "No se pueden restar cantidades con diferentes dimensiones"
                )
            other_converted = other.to(self.unit)
            new_value = self.magnitude - other_converted.magnitude
            new_uncertainty_obj = self.uncertainty_obj.add(
                other_converted.uncertainty_obj
            )
            return Quantity.from_input(
                new_value, self.unit, uncertainty=new_uncertainty_obj
            )
        if isinstance(other, (int, float, np.ndarray)):
            if not self.unit.is_dimensionless():
                raise ValueError(
                    "No se puede restar un escalar de una cantidad"
                    " con dimensiones."
                )
            new_value = self.magnitude - other
            return Quantity.from_input(
                new_value, self.unit, uncertainty=self.uncertainty_obj
            )
        return NotImplemented

    # --- Multiplicación (__mul__) ---
    @overload
    def __mul__(
        self: Quantity[NDArray[Any], Any], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __mul__(
        self: Quantity[Any, NDArray[Any]], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __mul__(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __mul__(
        self, other: Quantity[Any, NDArray[Any]]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __mul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __mul__(
        self: Quantity[float, Any], other: Any
    ) -> Quantity[float, float]: ...
    @overload
    def __mul__(
        self, other: Quantity[float, Any]
    ) -> Quantity[float, float]: ...
    @overload
    def __mul__(self, other: float) -> Quantity[float, float]: ...
    @overload
    def __mul__(
        self: Quantity[int, float], other: Quantity[int, float]
    ) -> Quantity[int, float]: ...
    @overload
    def __mul__(self, other: CompoundUnit) -> Quantity[ValueType, UncType]: ...

    def __mul__(self, other: Any) -> Any:  # noqa: D105
        if isinstance(other, Quantity):
            new_value = self.magnitude * other.magnitude
            new_unit = self.unit * other.unit
            new_uncertainty_obj = self.uncertainty_obj.propagate_mul_div(
                other.uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_value,
            )
            return Quantity.from_input(
                new_value, new_unit, uncertainty=new_uncertainty_obj
            )
        if isinstance(other, (int, float, np.ndarray)):
            new_value = self.magnitude * other
            new_uncertainty = self.uncertainty * np.abs(other)
            return Quantity.from_input(
                new_value, self.unit, uncertainty=new_uncertainty
            )
        if isinstance(other, CompoundUnit):
            return Quantity.from_input(
                self.magnitude,
                self.unit * other,
                uncertainty=self.uncertainty_obj,
            )
        return NotImplemented

    # --- División (__truediv__) ---
    # REGLA: La división en Python 3 siempre produce floats o arrays de floats.
    @overload
    def __truediv__(
        self: Quantity[NDArray[Any], Any], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __truediv__(
        self: Quantity[Any, NDArray[Any]], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...

    @overload
    def __truediv__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    # Cualquier otra combinación (float/int) resulta en float/float
    @overload
    def __truediv__(
        self, other: Quantity[Any, Any]
    ) -> Quantity[float, float]: ...
    @overload
    def __truediv__(self, other: float) -> Quantity[float, float]: ...
    @overload
    def __truediv__(
        self, other: CompoundUnit
    ) -> Quantity[ValueType, UncType]: ...

    def __truediv__(self, other: Any) -> Any:  # noqa: D105
        if isinstance(other, Quantity):
            new_value = self.magnitude / other.magnitude
            new_unit = self.unit / other.unit
            new_uncertainty_obj = self.uncertainty_obj.propagate_mul_div(
                other.uncertainty_obj,
                self.magnitude,
                other.magnitude,
                new_value,
            )
            return Quantity.from_input(
                new_value, new_unit, uncertainty=new_uncertainty_obj
            )
        if isinstance(other, (int, float, np.ndarray)):
            new_value = self.magnitude / other
            new_uncertainty = self.uncertainty / np.abs(other)
            return Quantity.from_input(
                new_value, self.unit, uncertainty=new_uncertainty
            )
        if isinstance(other, CompoundUnit):
            return Quantity.from_input(
                self.magnitude,
                self.unit / other,
                uncertainty=self.uncertainty_obj,
            )
        return NotImplemented

    def __pow__(self, exponent: float) -> Quantity[ValueType, UncType]:
        """Eleva esta cantidad a una potencia."""
        new_value = self.magnitude**exponent
        new_unit = self.unit**exponent

        calc_value = np.asarray(self.magnitude, dtype=float)
        new_uncertainty_obj = self.uncertainty_obj.power(
            exponent, cast(UncType, calc_value)
        )

        return Quantity.from_input(
            value=new_value, unit=new_unit, uncertainty=new_uncertainty_obj
        )

    def __format__(self, format_spec: str) -> str:
        """Format the Quantity using a composite format specification.

        The format_spec can be provided in one of the following forms:
          1. "<numeric_format>|<unit_format>" (e.g., ".2f|full" or
          "frac|alias")
          2. "<numeric_format>" (e.g., ".2f", where the unit defaults to
          'full')
          3. "<unit_format>" if the format_spec is recognized as a unit format
          (e.g., "alias" or "full")
             so that the numeric part defaults to str(self.magnitude).

        Special case:
          - "frac" for the numeric part will output the Fraction
          representation.
        """
        recognized_unit_formats = {"alias", "full"}

        # Check for a composite spec using the delimiter '|'.
        if "|" in format_spec:
            numeric_format, unit_format = format_spec.split("|", 1)
        else:
            # If the provided format spec is one of the recognized unit
            # formats, treat it as a unit spec and default the numeric part.
            if (
                format_spec in recognized_unit_formats
                or format_spec.startswith("alias:")
                or format_spec.startswith("full:")
            ):
                numeric_format = ""
                unit_format = format_spec
            else:
                numeric_format = format_spec
                unit_format = "full"  # Default unit format.

        # Format numeric part.
        if numeric_format == "frac":
            numeric_str = str(self.fraction)
        elif numeric_format:
            # Check if self.magnitude is an array
            if isinstance(self.magnitude, np.ndarray):
                numeric_str = np.array2string(
                    self.magnitude,
                    formatter={
                        "float_kind": lambda x: format(x, numeric_format)
                    },
                )
            else:
                try:
                    numeric_str = format(float(self.magnitude), numeric_format)
                except (ValueError, TypeError):
                    numeric_str = str(self.magnitude)
        else:
            numeric_str = str(self.magnitude)

        # Format unit part.
        unit_str = format(self.unit, unit_format)
        return f"{numeric_str} {unit_str}"

    def to_latex(self):
        """Devuelve una representación LaTeX de la cantidad."""
        # sympy tiene excelentes capacidades de impresión LaTeX
        value_latex = sp.latex(self.magnitude)
        unit_latex = self.unit.to_latex()

        if self.uncertainty > 0:
            unc_latex = sp.latex(self.uncertainty)
            return f"({value_latex} \\pm {unc_latex}) \\; {unit_latex}"

        return f"{value_latex} \\; {unit_latex}"

    def __str__(self):
        """Representación en string, dependiente de la configuración."""
        output_format = config.get_setting("default_output", "plain")
        if output_format == "latex":
            return self.to_latex()

        is_array_unc = isinstance(self.uncertainty, np.ndarray)
        if not is_array_unc and self.uncertainty == 0:
            return f"{self.magnitude} {self.unit:full}"

        if is_array_unc:
            return (
                f"Quantity(value={self.magnitude}, unit={self.unit:full}"
                ", uncertainty=[...])"
            )

        return f"({self.magnitude} ± {self.uncertainty}) {self.unit:full}"

    def _repr_latex_(self):
        """Método especial para renderizado automático en Jupyter Notebooks."""
        return f"${self.to_latex()}$"

    def __repr__(self) -> str:
        return (
            f"Quantity({self.magnitude!r}, {self.unit!r}, "
            f"uncertainty={self.uncertainty!r})"
        )

    # --- Suma Inversa: __radd__ ---
    # La suma es conmutativa, por lo que delega en __add__.
    # Las sobrecargas son un espejo de las de __add__ para other: Numeric
    @overload
    def __radd__(
        self: Quantity[NDArray[Any], Any], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __radd__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __radd__(
        self: Quantity[float, Any], other: Any
    ) -> Quantity[float, float]: ...
    @overload
    def __radd__(self, other: float) -> Quantity[float, float]: ...
    def __radd__(self, other: Any) -> Any:
        return self.__add__(other)

    # --- Resta Inversa: __rsub__ ---
    # La resta NO es conmutativa: other - self != self - other
    # La implementación es: other - self = -(self - other)
    # Las sobrecargas deben reflejar el resultado de esa operación.
    @overload
    def __rsub__(
        self: Quantity[NDArray[Any], Any], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __rsub__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __rsub__(
        self: Quantity[float, Any], other: Any
    ) -> Quantity[float, float]: ...
    @overload
    def __rsub__(self, other: float) -> Quantity[float, float]: ...

    def __rsub__(self, other: Any) -> Any:
        return self.__neg__().__add__(other)

    # --- Multiplicación Inversa: __rmul__ ---
    # La multiplicación es conmutativa, por lo que delega en __mul__.
    # Las sobrecargas son un espejo de las de __mul__ para other: Numeric
    @overload
    def __rmul__(
        self: Quantity[NDArray[Any], Any], other: Any
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __rmul__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __rmul__(
        self: Quantity[float, Any], other: Any
    ) -> Quantity[float, float]: ...
    @overload
    def __rmul__(self, other: float) -> Quantity[float, float]: ...

    def __rmul__(self, other: Any) -> Any:
        return self.__mul__(other)

    # --- División Inversa: __rtruediv__ ---
    # Ya lo habíamos corregido, pero aquí está la versión final consistente.
    # El tipo de retorno depende del tipo de `other`.
    @overload
    def __rtruediv__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __rtruediv__(self, other: float) -> Quantity[float, float]: ...

    def __rtruediv__(self, other: Any) -> Any:
        if np.any(np.asarray(self.magnitude) == 0):
            raise ZeroDivisionError(
                "División por una Quantity con valor cero."
            )

        new_value = other / self.magnitude
        new_unit = 1 / self.unit
        other_uncertainty = Uncertainty(0.0)
        new_uncertainty_obj = other_uncertainty.propagate_mul_div(
            self.uncertainty_obj, other, self.magnitude, new_value
        )

        return Quantity.from_input(
            new_value, new_unit, uncertainty=new_uncertainty_obj
        )

    @overload
    def __rdiv__(
        self, other: NDArray[Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]: ...
    @overload
    def __rdiv__(self, other: float) -> Quantity[float, float]: ...
    def __rdiv__(self, other: Any) -> Any:
        return self.__rtruediv__(other)

    def __rpow__(self, base: Numeric) -> Quantity[ValueType, UncType]:
        """Calcula la potencia de un número base elevado a una cantidad.

        Esta operación solo es físicamente válida si la cantidad (self)
        es adimensional.
        """
        # 1. Verificación de la dimensión
        if not self.unit.is_dimensionless():
            raise ValueError(
                "Exponentiation with a Quantity as the exponent is only "
                "supported for dimensionless quantities."
            )

        # 2. Cálculo del nuevo valor
        new_value = base**self.magnitude

        # --- PROPAGACIÓN DE INCERTIDUMBRE (Regla para z = c^x) ---
        # Fórmula: δz = |z * ln(c)| * δx
        # donde 'c' es la base (sin incertidumbre) y 'x' es nuestro Quantity.
        if isinstance(base, (int, float)) and base > 0:
            # math.log es el logaritmo natural (ln)
            new_uncertainty = (
                abs(new_value * math.log(base)) * self.uncertainty
            )
        else:
            # Para bases complejas, arrays, o negativas, la propagación es más
            # compleja. Por ahora, no la propagamos en esos casos.
            new_uncertainty = 0.0

        # 3. El resultado es siempre una cantidad adimensional.
        #    Creamos una unidad vacía para representarlo.
        dimensionless_unit = CompoundUnit({})

        # 4. Devolvemos un objeto Quantity, cumpliendo con la firma de tipo.
        return Quantity.from_input(
            new_value, dimensionless_unit, uncertainty=new_uncertainty
        )

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Quantity):
            return NotImplemented

        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions "
                f"{self.dimension} != {other.dimension}"
            )

        other_converted = other.to(self.unit)
        return math.isclose(self.magnitude, other_converted.magnitude)

    def __hash__(self) -> int:
        """Hash robusto para Quantity.

        Calcula el hash de la Quantity de forma robusta, con soporte para
        arrays de NumPy.
        """
        # Para los arrays de NumPy, que no son hashables, creamos un hash
        # a partir de su representación inmutable en bytes.
        if isinstance(self.magnitude, np.ndarray):
            # Hacemos el array de solo lectura para asegurar la inmutabilidad
            # conceptual antes de obtener los bytes.
            self.magnitude.flags.writeable = False
            value_hash = hash(self.magnitude.tobytes())
        else:
            # Para escalares (int, float), el hash es directo.
            value_hash = hash(self.magnitude)

        # Combinamos el hash del valor con el hash de la unidad.
        # Es una buena práctica usar el operador XOR (^) para combinar hashes,
        # ya que distribuye bien los bits.
        return value_hash ^ hash(self.unit)

    def __neg__(self) -> Quantity[ValueType, UncType]:
        return Quantity.from_input(-self.magnitude, self.unit)

    def __pos__(self) -> Quantity[ValueType, UncType]:
        return Quantity.from_input(+self.magnitude, self.unit)

    def __abs__(self) -> Quantity[ValueType, UncType]:
        return Quantity.from_input(abs(self.magnitude), self.unit)

    def __float__(self) -> float:
        return float(self.magnitude)

    def __trunc__(self) -> Quantity[ValueType, UncType]:
        """Devuelve una Quantity con la parte entera de su valor, usando np.trunc
        para compatibilidad con arrays.
        """
        # --- CORRECCIÓN ---
        # Usamos np.trunc en lugar de math.trunc.
        new_value = np.trunc(self.magnitude)
        # La incertidumbre no se ve afectada por el truncamiento en este modelo.
        return Quantity.from_input(
            value=new_value, unit=self.unit, uncertainty=self.uncertainty_obj
        )

    def __floor__(self) -> Quantity[ValueType, UncType]:
        """Devuelve una Quantity con el valor redondeado hacia abajo, usando np.floor
        para compatibilidad con arrays.
        """
        # --- CORRECCIÓN ---
        # Usamos np.floor en lugar de math.floor.
        new_value = np.floor(self.magnitude)
        return Quantity.from_input(
            value=new_value, unit=self.unit, uncertainty=self.uncertainty_obj
        )

    def __ceil__(self) -> Quantity[ValueType, UncType]:
        """Devuelve una Quantity con el valor redondeado hacia arriba, usando np.ceil
        para compatibilidad con arrays.
        """
        # --- CORRECCIÓN ---
        # Usamos np.ceil en lugar de math.ceil.
        new_value = np.ceil(self.magnitude)
        return Quantity.from_input(
            value=new_value, unit=self.unit, uncertainty=self.uncertainty_obj
        )

    def __round__(
        self, ndigits: int | None = None
    ) -> Quantity[ValueType, UncType]:
        """Devuelve una Quantity con el valor redondeado, usando np.round
        para compatibilidad con arrays.
        """
        # --- CORRECCIÓN ---
        # Usamos la función np.round que funciona con escalares y arrays.
        if ndigits is None:
            new_value = np.round(self.magnitude)
        else:
            new_value = np.round(self.magnitude, ndigits)

        return Quantity.from_input(
            value=new_value, unit=self.unit, uncertainty=self.uncertainty_obj
        )

    def __floordiv__(
        self, other: Quantity[ValueType, UncType]
    ) -> Quantity[ValueType, UncType]:
        if not isinstance(other, Quantity):
            raise TypeError(f"Expected Quantity, got {type(other).__name__}")
        return Quantity.from_input(
            self.magnitude // other.magnitude, self.unit / other.unit
        )

    def __rfloordiv__(self, other: Numeric) -> Quantity[ValueType, UncType]:
        return Quantity.from_input(other // self.magnitude, 1 / self.unit)

    def __mod__(
        self, other: Quantity[ValueType, UncType]
    ) -> Quantity[ValueType, UncType]:
        if not isinstance(other, Quantity):
            raise TypeError(f"Expected Quantity, got {type(other).__name__}")
        return Quantity.from_input(self.magnitude % other.magnitude, self.unit)

    def __rmod__(self, other: Numeric) -> Quantity[ValueType, UncType]:
        return Quantity.from_input(other % self.magnitude, self.unit)

    def __lt__(
        self, other: Quantity[ValueType, UncType] | Any
    ) -> bool | NDArray[np.bool_]:
        if not isinstance(other, Quantity):
            raise TypeError(f"Expected Quantity, got {type(other).__name__}")
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
                f"{self.dimension} != {other.dimension}"
            )
        return self.magnitude < other.magnitude

    def __le__(
        self, other: Quantity[ValueType, UncType] | Any
    ) -> bool | NDArray[np.bool_]:
        if not isinstance(other, Quantity):
            raise TypeError(f"Expected Quantity, got {type(other).__name__}")
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
                f"{self.dimension} != {other.dimension}"
            )
        return self.magnitude <= other.magnitude

    def __gt__(
        self, other: Quantity[ValueType, UncType] | Any
    ) -> bool | NDArray[np.bool_]:
        if not isinstance(other, Quantity):
            raise TypeError(f"Expected Quantity, got {type(other).__name__}")
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
                f"{self.dimension} != {other.dimension}"
            )
        return self.magnitude > other.magnitude

    def __ge__(
        self, other: Quantity[ValueType, UncType] | Any
    ) -> bool | NDArray[np.bool_]:
        if not isinstance(other, Quantity):
            raise TypeError(f"Expected Quantity, got {type(other).__name__}")
        if self.dimension != other.dimension:
            raise ValueError(
                "Cannot compare quantities with different dimensions"
                f"{self.dimension} != {other.dimension}"
            )
        return self.magnitude >= other.magnitude

    def __array_ufunc__(self, ufunc, method, *inputs, **kwargs):
        # Primero, extraemos el objeto Quantity de los inputs
        q_input = None
        other_inputs = []
        for inp in inputs:
            if isinstance(inp, Quantity):
                q_input = inp
            else:
                other_inputs.append(inp)

        if q_input is None:
            return NotImplemented

        # **LA CORRECCIÓN ESTÁ AQUÍ**
        # Maneja métodos de reducción como .max(), .min(), .sum()
        if method == "reduce":
            # La ufunc de reducción (ej. np.maximum) se aplica al valor
            # numérico
            result_value = ufunc.reduce(q_input.magnitude, **kwargs)
            # El resultado es un Quantity escalar con la misma unidad
            return Quantity.from_input(
                result_value, q_input.unit, uncertainty=0.0
            )

        # Maneja llamadas de función estándar como np.abs(q)
        if method == "__call__":
            # Recrea los inputs para la llamada a la ufunc, pero solo con
            # valores numéricos
            numeric_inputs = [
                inp.magnitude if isinstance(inp, Quantity) else inp
                for inp in inputs
            ]
            result_value = ufunc(*numeric_inputs, **kwargs)

            # Casos especiales para manejar unidades y incertidumbre
            if ufunc == np.absolute:
                return Quantity.from_input(
                    result_value, q_input.unit, uncertainty=q_input.uncertainty
                )

            if ufunc == np.sqrt:
                result_unit = q_input.unit**0.5
                if np.all(result_value == 0):
                    result_uncertainty = 0
                else:
                    rel_unc = (
                        q_input.uncertainty / q_input.magnitude
                        if np.all(q_input.magnitude != 0)
                        else 0
                    )
                    result_uncertainty = np.abs(result_value * 0.5) * rel_unc
                return Quantity.from_input(
                    result_value, result_unit, uncertainty=result_uncertainty
                )

            elif ufunc in {np.sin, np.cos, np.tan}:
                if (
                    q_input.unit.dimension.name != "dimensionless"
                    and q_input.unit.dimension.name != "angle"
                ):
                    raise ValueError(
                        f"{ufunc.__name__} requires a"
                        " dimensionless quantity or an angle."
                    )
                result_unit = CompoundUnit({})
                if ufunc == np.sin:
                    derivative = np.abs(np.cos(q_input.magnitude))
                elif ufunc == np.cos:
                    derivative = np.abs(-np.sin(q_input.magnitude))
                else:
                    derivative = np.abs(1 / np.cos(q_input.magnitude) ** 2)
                result_uncertainty = derivative * q_input.uncertainty
                return Quantity.from_input(
                    result_value, result_unit, uncertainty=result_uncertainty
                )

            # Si es una operación binaria (ej. np.add), delega a los dunder
            op_map = {
                np.add: operator.add,
                np.subtract: operator.sub,
                np.multiply: operator.mul,
                np.true_divide: operator.truediv,
            }
            if ufunc in op_map and len(inputs) == 2:
                return op_map[ufunc](inputs[0], inputs[1])

            # Si no es un caso especial, devuelve un Quantity sin unidades
            if q_input.unit.is_dimensionless():
                return Quantity.from_input(result_value, q_input.unit)

    def dot(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[float, float]:
        """Calcula el producto punto entre dos cantidades.

        El tipo de retorno (escalar o array) depende de las dimensiones de los
        valores de entrada.
        """
        if not isinstance(other, Quantity):
            return NotImplemented

        # La implementación no cambia. Su comportamiento ya es polimórfico.
        result_value = np.dot(self.magnitude, other.magnitude)
        result_unit = self.unit * other.unit

        # Dejamos que from_input infiera el tipo correcto (float o NDArray)
        # a partir del resultado de np.dot.
        return cast(
            Quantity[float, float],
            Quantity.from_input(result_value, result_unit, uncertainty=0.0),
        )

    # --- Producto Cruz: cross ---
    # REGLA: El producto cruz de dos vectores devuelve otro vector (NDArray).
    # No se usa @overload porque solo hay una firma principal.
    def cross(
        self, other: Quantity[NDArray[Any], Any]
    ) -> Quantity[NDArray[Any], NDArray[Any]]:
        """Calcula el producto cruz entre dos cantidades vectoriales."""
        if not isinstance(other, Quantity):
            return NotImplemented

        result_value = np.cross(self.magnitude, other.magnitude)
        result_unit = self.unit * other.unit

        # La propagación de incertidumbre es compleja y se omite.
        # Creamos una Quantity vectorial con el resultado.
        return Quantity.from_input(result_value, result_unit, uncertainty=0.0)

    def __len__(self):
        """Permite que len() funcione en un objeto Quantity que contiene un
        arreglo.
        """
        if isinstance(self.magnitude, np.ndarray):
            return len(self.magnitude)
        raise TypeError(
            f"Object of type '{type(self).__name__}' has no len()."
        )

    def __getitem__(self, key):
        """Permite el indexado (ej. quantity[i]) en un objeto Quantity que
        contiene un arreglo.
        """
        if not isinstance(self.magnitude, np.ndarray):
            raise TypeError(
                f"'{type(self).__name__}' object is not subscriptable."
            )

        # Obtiene el valor o la rebanada (slice) del arreglo
        sliced_value = self.magnitude[key]

        # Maneja la incertidumbre de manera correspondiente
        sliced_uncertainty = 0.0
        if isinstance(self.uncertainty, np.ndarray):
            # Si la incertidumbre es un arreglo, se rebana también
            sliced_uncertainty = self.uncertainty[key]
        else:
            # Si la incertidumbre es un escalar, se aplica al elemento/rebanada
            sliced_uncertainty = self.uncertainty

        return Quantity.from_input(
            sliced_value, self.unit, uncertainty=sliced_uncertainty
        )


__all__ = ["Quantity"]
