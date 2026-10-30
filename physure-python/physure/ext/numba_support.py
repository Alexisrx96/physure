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

    # --- 1. Define the Numba type ---
    class QuantityType(types.Type):
        """Represents a Quantity in Numba's type system.

        We keep the 'unit' as part of the type (a compile-time constant)
        so the object can be rebuilt on the way out.
        """

        def __init__(self, dtype: Any, unit: CompoundUnit) -> None:
            """Initializes the QuantityType."""
            self.dtype = dtype  # The type of the magnitude (e.g. float64)
            self.unit = unit  # The unit (CompoundUnit)
            # The name must be unique for each type/unit combination
            super().__init__(name=f"Quantity({dtype}, unit={unit!r})")

        @property
        def key(self) -> tuple[Any, CompoundUnit]:
            """Numba uses this to tell types apart."""
            return (self.dtype, self.unit)

    # --- 2. Type inference (Python -> Numba) ---
    @typeof_impl.register(Quantity)
    def typeof_quantity(val: Quantity, c: Any) -> QuantityType:
        """Tells Numba: 'when you see this object, create this type'."""
        # Infer the type of the magnitude (float, array, etc.)
        mag_type = typeof_impl(val.magnitude, c)
        # Create our custom type carrying the unit
        return QuantityType(mag_type, val.unit)

    # --- 3. Data model (C struct) ---
    @register_model(QuantityType)
    class QuantityModel(models.StructModel):
        """Defines what a Quantity looks like in memory (low level).

        For maximum efficiency we store ONLY the magnitude in the struct.
        The unit is handled statically by the type system.
        """

        def __init__(self, dmm: Any, fe_type: QuantityType) -> None:
            """Initializes the QuantityModel."""
            members = [
                ("magnitude", fe_type.dtype),
            ]
            super().__init__(dmm, fe_type, members)

    # --- 4. Expose attributes ---
    # This allows `q.magnitude` inside an @njit function
    make_attribute_wrapper(QuantityType, "magnitude", "magnitude")

    # --- 5. Unboxing (Python -> Native) ---
    @unbox(QuantityType)
    def unbox_quantity(typ: QuantityType, obj: Any, c: Any) -> NativeValue:
        """Converts the Python Quantity object to our native struct."""
        # 1. Get the .magnitude attribute of the Python object
        mag_obj = c.pyapi.object_getattr_string(obj, "magnitude")

        # 2. Unbox that magnitude to its native type (e.g. a native array)
        native_mag = c.unbox(typ.dtype, mag_obj)

        # 3. Create the struct proxy and fill it
        quantity_struct = cgutils.create_struct_proxy(typ)(
            c.context, c.builder
        )
        quantity_struct.magnitude = native_mag.value

        # 4. Clean up references (refcounting)
        c.pyapi.decref(mag_obj)

        # Return the native value and the error flag
        return NativeValue(
            quantity_struct._getvalue(),
            is_error=native_mag.is_error,
            cleanup=native_mag.cleanup,
        )

    # --- 6. Boxing (Native -> Python) ---
    @box(QuantityType)
    def box_quantity(typ: QuantityType, val: Any, c: Any) -> Any:
        """Converts the native struct back to a Python object.

        This is where we recover the unit that we stored in QuantityType.
        """
        # 1. Read the struct
        quantity_struct = cgutils.create_struct_proxy(typ)(
            c.context, c.builder, value=val
        )

        # 2. Convert the native magnitude to a Python object (float or ndarray)
        mag_obj = c.box(typ.dtype, quantity_struct.magnitude)

        # 3. Get the Quantity class and the unit from the Python environment
        # We serialize the unit via pickle or pass it as an object.
        # Trick: we use unpickle to inject the `unit` object.
        unit_obj = c.pyapi.unserialize(c.pyapi.serialize_object(typ.unit))

        # 4. Call Quantity(magnitude, unit)
        # We need to import the class inside the generated runtime
        quantity_cls = c.pyapi.unserialize(c.pyapi.serialize_object(Quantity))

        # Constructor args: (magnitude, unit)
        args = c.pyapi.tuple_pack([mag_obj, unit_obj])

        # Instantiate: res = Quantity(mag, unit)
        res = c.pyapi.call(quantity_cls, args, None)

        # Cleanup
        c.pyapi.decref(args)
        c.pyapi.decref(quantity_cls)
        c.pyapi.decref(unit_obj)
        c.pyapi.decref(mag_obj)

        return res

except (ImportError, AttributeError):
    # ponytail: HAS_NUMBA toggles between True/False across the
    # try/except branches by design; not a real constant-redefinition bug.
    HAS_NUMBA = False  # pyright: ignore[reportConstantRedefinition]
