import pytest

from physure._jit.tracer import DimensionalError
from physure.ext.compiler import compile_physics_model


def drag_force(density, velocity, area, drag_coeff):
    return 0.5 * density * (velocity**2) * area * drag_coeff


def test_compile_physics_model_matches_roadmap_example():
    fast_drag = compile_physics_model(
        drag_force,
        input_units={
            "density": "kg/m^3",
            "velocity": "m/s",
            "area": "m^2",
            "drag_coeff": "",
        },
        output_unit="N",
    )
    assert fast_drag(1.225, 10.0, 2.5, 0.3) == pytest.approx(45.9375)
    forces = [fast_drag(1.225, v, 2.5, 0.3) for v in range(1, 5)]
    assert forces == [
        pytest.approx(0.5 * 1.225 * v**2 * 2.5 * 0.3) for v in range(1, 5)
    ]


def test_compile_physics_model_returns_raw_float_not_quantity():
    fast_drag = compile_physics_model(
        drag_force,
        input_units={
            "density": "kg/m^3",
            "velocity": "m/s",
            "area": "m^2",
            "drag_coeff": "",
        },
        output_unit="N",
    )
    result = fast_drag(1.225, 10.0, 2.5, 0.3)
    assert isinstance(result, float)


def test_compile_physics_model_rejects_internal_unit_mismatch():
    def bad(mass, length):
        return mass + length

    with pytest.raises(DimensionalError):
        compile_physics_model(
            bad,
            input_units={"mass": "kg", "length": "m"},
            output_unit="kg",
        )


def test_compile_physics_model_rejects_wrong_output_unit():
    def energy(mass, velocity):
        return 0.5 * mass * velocity**2

    with pytest.raises(ValueError, match="does not match declared"):
        compile_physics_model(
            energy,
            input_units={"mass": "kg", "velocity": "m/s"},
            output_unit="N",
        )


def test_compile_physics_model_target_param_is_accepted_but_ignored():
    fast_drag_llvm = compile_physics_model(
        drag_force,
        input_units={
            "density": "kg/m^3",
            "velocity": "m/s",
            "area": "m^2",
            "drag_coeff": "",
        },
        output_unit="N",
        target="llvm",
    )
    fast_drag_wasm = compile_physics_model(
        drag_force,
        input_units={
            "density": "kg/m^3",
            "velocity": "m/s",
            "area": "m^2",
            "drag_coeff": "",
        },
        output_unit="N",
        target="wasm",
    )
    assert fast_drag_llvm(1.225, 10.0, 2.5, 0.3) == pytest.approx(
        fast_drag_wasm(1.225, 10.0, 2.5, 0.3)
    )
