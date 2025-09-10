from dataclasses import dataclass, field
from typing import Callable

import numpy as np
import sympy as sp

from measurekit.measurement.api import Q_
from measurekit.measurement.dimensions import Dimension
from measurekit.measurement.quantity import Quantity
from measurekit.measurement.units import CompoundUnit


@dataclass(frozen=True)
class Function:
    """Representa una función físico-matemática.

    Una instancia de esta clase es inmutable que es consciente de
    las dimensiones y unidades.
    """

    parameters: dict[str, Dimension]
    output_dimension: Dimension
    symbolic_func: sp.Expr
    arg_names: tuple[str, ...] = field(init=False, repr=False)

    numeric_func: Callable[..., np.ndarray] = field(init=False, repr=False)

    def __post_init__(self):
        """Este método se llama automáticamente después del __init__ de la dataclass.
        Lo usamos para pre-calcular los atributos de eficiencia y DX.
        """
        arg_symbols = tuple(self.symbolic_func.free_symbols)

        sorted_symbols = sorted(arg_symbols, key=lambda s: s.name)  # type: ignore
        object.__setattr__(
            self,
            "arg_names",
            tuple(s.name for s in sorted_symbols),  # type: ignore
        )

        callable_func = sp.lambdify(
            sorted_symbols, self.symbolic_func, "numpy"
        )
        object.__setattr__(self, "numeric_func", callable_func)

    def __call__(
        self, output_unit: CompoundUnit, **kwargs: Quantity
    ) -> Quantity:
        """Ejecuta la función, validando las entradas y devolviendo el resultado
        en la unidad de salida especificada.
        """
        if output_unit.dimension != self.output_dimension:
            raise ValueError(
                f"La unidad de salida '{output_unit}' tiene una dimensión "
                f"incorrecta. Esperada: {self.output_dimension}, "
                f"Recibida: {output_unit.dimension}"
            )

        # 2. Validación de parámetros
        if len(kwargs) != len(self.parameters):
            raise ValueError(
                f"Se esperaban {len(self.parameters)} argumentos, "
                f"pero se recibieron {len(kwargs)}"
            )

        # 3. Desempaquetado y validación de dimensiones
        numeric_args = []
        for name in self.arg_names:
            if name not in kwargs:
                raise ValueError(f"Parámetro faltante: '{name}'")

            quantity = kwargs[name]
            expected_dim = self.parameters[name]

            if quantity.dimension != expected_dim:
                raise ValueError(
                    f"Dimensión incorrecta para '{name}'. "
                    f"Esperada: {expected_dim}, "
                    f"Recibida: {quantity.dimension}"
                )

            numeric_args.append(quantity.magnitude)

        # 4. Llamada a la función numérica eficiente
        result_value = self.numeric_func(*numeric_args)

        # 5. Empaquetado del resultado en la unidad de salida correcta
        return Q_(result_value, output_unit)

    def derivative(self, respect_to: str) -> "Function":
        """Devuelve una NUEVA función que representa la derivada.
        El enfoque inmutable asegura que no modificamos la función original.
        """
        respect_to_sym = sp.Symbol(respect_to)
        if respect_to_sym not in self.symbolic_func.free_symbols:
            raise ValueError(
                f"'{respect_to}' no es un parámetro de la función."
            )

        derivative_expr = sp.diff(self.symbolic_func, respect_to_sym)

        # La nueva dimensión de salida es la original dividida por la
        # dimensión de la variable respecto a la que se derivó.
        new_output_dim = self.output_dimension / self.parameters[respect_to]

        return Function(
            parameters=self.parameters,
            output_dimension=new_output_dim,
            symbolic_func=derivative_expr,
        )

    def __repr__(self) -> str:
        param_str = ", ".join(
            f"{name}: {dim.analytical_representation}"
            for name, dim in self.parameters.items()
        )
        return (
            f"Function({self.symbolic_func}, "
            f"params={{ {param_str} }}, "
            f"output_dim={self.output_dimension.analytical_representation})"
        )
