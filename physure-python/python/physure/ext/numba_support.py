# pyright: reportAny=false, reportExplicitAny=false, reportUnknownMemberType=false, reportUnknownVariableType=false, reportUnknownArgumentType=false, reportUnknownParameterType=false
# ponytail: numba ships no type stubs; every value crossing its compiler
# API (c.pyapi, cgutils, c.context, ...) is genuinely untyped upstream, not
# a gap in our own annotations.
"""Numba extension for Physure.

Allows Quantity objects to be passed into @njit decorated functions.
"""

from typing import Any

from physure.domain.measurement.quantity import Quantity
from physure.domain.measurement.units import CompoundUnit

try:
    from numba import types
    from numba.core import cgutils
    from numba.extending import (
        NativeValue,
        box,
        make_attribute_wrapper,
        models,
        register_model,
        typeof_impl,
        unbox,
    )

    HAS_NUMBA = True

    # ponytail: numba ships no type stubs, so every value that crosses its
    # compiler API (dtype/typ/c/obj/dmm/fe_type/val below) is genuinely
    # Any from pyright's perspective — no annotation here can recover the
    # unknown-member errors numba's own untyped API produces downstream.

    # --- 1. Definir el Tipo en Numba ---
    class QuantityType(types.Type):
        """Representa una Quantity en el sistema de tipos de Numba.

        Guardamos la 'unit' como parte del tipo (constante en compilación)
        para poder reconstruir el objeto al salir.
        """

        def __init__(self, dtype: Any, unit: CompoundUnit) -> None:
            """Initializes the QuantityType."""
            self.dtype = dtype  # El tipo de la magnitud (ej. float64)
            self.unit = unit  # La unidad (CompoundUnit)
            # El nombre debe ser único para cada combinación de tipo/unidad
            super().__init__(name=f"Quantity({dtype}, unit={unit!r})")

        @property
        def key(self) -> tuple[Any, CompoundUnit]:
            """Numba usa esto para diferenciar tipos."""
            return (self.dtype, self.unit)

    # --- 2. Inferencia de Tipos (Python -> Numba) ---
    @typeof_impl.register(Quantity)
    def typeof_quantity(val: Quantity, c: Any) -> QuantityType:
        """Le dice a Numba: 'Cuando veas est objeto, crea este Tipo'."""
        # Inferimos el tipo de la magnitud (float, array, etc.)
        mag_type = typeof_impl(val.magnitude, c)
        # Creamos nuestro tipo personalizado llevando la unidad
        return QuantityType(mag_type, val.unit)

    # --- 3. Modelo de Datos (Struct en C) ---
    @register_model(QuantityType)
    class QuantityModel(models.StructModel):
        """Define cómo se ve la Quantity en memoria (bajo nivel).

        Para eficiencia máxima, SOLO guardamos la magnitud en el struct.
        La unidad es manejada estáticamente por el sistema de tipos.
        """

        def __init__(self, dmm: Any, fe_type: QuantityType) -> None:
            """Initializes the QuantityModel."""
            members = [
                ("magnitude", fe_type.dtype),
            ]
            super().__init__(dmm, fe_type, members)

    # --- 4. Exponer Atributos ---
    # Esto permite hacer `q.magnitude` dentro de una función @njit
    make_attribute_wrapper(QuantityType, "magnitude", "magnitude")

    # --- 5. Unboxing (Python -> Native) ---
    @unbox(QuantityType)
    def unbox_quantity(typ: QuantityType, obj: Any, c: Any) -> NativeValue:
        """Convierte el objeto Python Quantity a nuestro struct nativo."""
        # 1. Obtener el atributo .magnitude del objeto Python
        mag_obj = c.pyapi.object_getattr_string(obj, "magnitude")

        # 2. Desempaquetar esa magnitud a su tipo nativo (ej. array nativo)
        native_mag = c.unbox(typ.dtype, mag_obj)

        # 3. Crear el struct proxy y llenarlo
        quantity_struct = cgutils.create_struct_proxy(typ)(
            c.context, c.builder
        )
        quantity_struct.magnitude = native_mag.value

        # 4. Limpieza de referencias (Refcounting)
        c.pyapi.decref(mag_obj)

        # Retornar el valor nativo y el flag de error
        return NativeValue(
            quantity_struct._getvalue(),
            is_error=native_mag.is_error,
            cleanup=native_mag.cleanup,
        )

    # --- 6. Boxing (Native -> Python) ---
    @box(QuantityType)
    def box_quantity(typ: QuantityType, val: Any, c: Any) -> Any:
        """Convierte struct nativo de vuelta a un objeto Python.

        Aquí es donde recuperamos la unidad que guardamos en QuantityType.
        """
        # 1. Leer el struct
        quantity_struct = cgutils.create_struct_proxy(typ)(
            c.context, c.builder, value=val
        )

        # 2. Convertir la magnitud nativa a objeto Python (float o ndarray)
        mag_obj = c.box(typ.dtype, quantity_struct.magnitude)

        # 3. Obtener la clase Quantity y la Unidad desde el entorno Python
        # Serializamos la unidad via pickle o la pasamos como objeto.
        # Truco: Usamos unpickle para inyectar el objeto `unit`.
        unit_obj = c.pyapi.unserialize(c.pyapi.serialize_object(typ.unit))

        # 4. Llamar a Quantity(magnitude, unit)
        # Necesitamos importar la clase dentro del runtime generado
        quantity_cls = c.pyapi.unserialize(c.pyapi.serialize_object(Quantity))

        # Constructor args: (magnitude, unit)
        args = c.pyapi.tuple_pack([mag_obj, unit_obj])

        # Instanciar: res = Quantity(mag, unit)
        res = c.pyapi.call(quantity_cls, args, None)

        # Limpieza
        c.pyapi.decref(args)
        c.pyapi.decref(quantity_cls)
        c.pyapi.decref(unit_obj)
        c.pyapi.decref(mag_obj)

        return res

except (ImportError, AttributeError):
    # ponytail: HAS_NUMBA toggles between True/False across the
    # try/except branches by design; not a real constant-redefinition bug.
    HAS_NUMBA = False  # pyright: ignore[reportConstantRedefinition]
