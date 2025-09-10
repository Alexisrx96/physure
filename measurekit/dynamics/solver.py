from typing import Callable

import numpy as np
from scipy.integrate import solve_ivp

from measurekit.measurement.api import Q_
from measurekit.measurement.quantity import Quantity


class ODESolution:
    """Clase para almacenar y presentar la solución de una EDO.

    Permite acceder a los resultados de forma sencilla.
    """

    def __init__(self, t: Quantity, y: list[Quantity]):
        self.t = t
        self.y = y

    def __repr__(self):
        return (
            f"ODESolution(t=[{self.t[0]:.2f}...{self.t[-1]:.2f}],"
            f" num_states={len(self.y)})"
        )


def solve_unit_aware_ivp(
    fun: Callable[[Quantity, list[Quantity]], list[Quantity]],
    t_span: list[Quantity],
    y0: list[Quantity],
    t_eval: np.ndarray | None = None,
    **kwargs,
) -> ODESolution:
    """Resuelve un problema de valor inicial manejando unidades de forma
    consciente y eficiente.
    """
    # --- 1. Desempaquetado de Unidades (UNA SOLA VEZ) ---
    # ¿Por qué? Extraemos toda la información de unidades ANTES de entrar
    # al bucle del solucionador. Esto es la clave de la eficiencia.
    t_unit = t_span[0].unit
    y0_values = np.array([q.magnitude for q in y0])
    y0_units = [q.unit for q in y0]

    # Calculamos las unidades esperadas para las derivadas de antemano.
    dydt_units = [state_unit / t_unit for state_unit in y0_units]

    t_span_values = [t_span[0].magnitude, t_span[1].to(t_unit).magnitude]

    # --- 2. Creación del Wrapper de la Función (Enfoque Eficiente) ---
    # ¿Por qué? Este wrapper ahora trabaja exclusivamente con arrays de NumPy.
    # El truco es que "cierra" (hace un closure) sobre las variables de
    # unidades (t_unit, y0_units, dydt_units) para poder re-empaquetar y
    # desempaquetar en los límites de la llamada.
    def fun_wrapper(t_val: float, y_val: np.ndarray) -> np.ndarray:
        # a. Re-empaquetado en Quantities para la DX del usuario
        t_q = Q_(t_val, t_unit)
        y_q = [Q_(val, unit) for val, unit in zip(y_val, y0_units)]

        # b. Llamada a la función original del usuario
        dy_dt_q = fun(t_q, y_q)

        # c. Desempaquetado de las derivadas a un array numérico
        # Se realizan las conversiones necesarias para asegurar la consistencia.
        dy_dt_values = np.array(
            [
                res.to(expected_unit).magnitude
                for res, expected_unit in zip(dy_dt_q, dydt_units)
            ]
        )

        return dy_dt_values

    # --- 3. Llamada al Solucionador de SciPy ---
    # SciPy solo ve números, lo que le permite correr a máxima velocidad.
    sol = solve_ivp(
        fun_wrapper, t_span_values, y0_values, t_eval=t_eval, **kwargs
    )

    # --- 4. Re-empaquetado de la Solución Final (UNA SOLA VEZ) ---
    # ¿Por qué? Una vez que SciPy ha terminado su trabajo, tomamos los arrays
    # numéricos resultantes y los convertimos de vuelta en objetos Quantity
    # para el usuario.
    solution_t = Q_(sol.t, t_unit)
    solution_y = [
        Q_(state_values, y0_units[i]) for i, state_values in enumerate(sol.y)
    ]

    return ODESolution(solution_t, solution_y)
