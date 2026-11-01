"""PhyEquation and PhyFunction: the text they build, the errors they raise, and a call through the Rust engine."""

import math

import pytest

from physure import Q_, Interpreter, PhyEquation, PhyFunction, phy_function


@pytest.fixture
def interp():
    return Interpreter()


def test_an_equation_is_split_at_its_sign_and_unquoted():
    eq = PhyEquation('"V = R * I"')
    assert (eq.lhs, eq.rhs) == ("V", "R * I")
    assert repr(eq) == 'PhyEquation("V = R * I")'
    bare = PhyEquation("x + 1")
    assert (bare.lhs, bare.rhs) == ("x + 1", "0")


def test_solving_an_equation_names_the_variable():
    solved = PhyEquation("V = R * I").solve("I")
    assert solved.lhs == "I"
    assert "V" in solved.rhs
    assert "R" in solved.rhs


def test_calling_an_equation_solves_for_the_one_variable_left():
    eq = PhyEquation("V = R * I")
    volts = eq(R=Q_(10.0, "ohm"), I=Q_(2.0, "A"))
    assert math.isclose(float(volts.magnitude), 20.0)
    assert volts.unit == Q_(1.0, "V").unit

    amps = eq(V=Q_(20.0, "V"), R=Q_(10.0, "ohm"))
    assert math.isclose(float(amps.magnitude), 2.0)
    assert math.isclose(float(eq(V=12, R=4)), 3.0)


@pytest.mark.parametrize(
    ("operation", "expected"),
    [
        (lambda e, o: e + o, "(V) + (W) = (R * I) + (U)"),
        (lambda e, o: e - o, "(V) - (W) = (R * I) - (U)"),
        (lambda e, o: e * o, "(V) * (W) = (R * I) * (U)"),
        (lambda e, o: e / o, "(V) / (W) = (R * I) / (U)"),
    ],
)
def test_two_equations_combine_side_by_side(operation, expected):
    first = PhyEquation("V = R * I")
    second = PhyEquation("W = U")
    assert operation(first, second).expression == expected


def test_an_equation_combines_with_a_number_a_quantity_and_a_power():
    eq = PhyEquation("V = R * I")
    assert (eq + 2).expression == "(V) + 2 = (R * I) + 2"
    assert (2 + eq).expression == "2 + (V) = 2 + (R * I)"
    assert (eq - 2).expression == "(V) - 2 = (R * I) - 2"
    assert (2 - eq).expression == "2 - (V) = 2 - (R * I)"
    assert (eq * 3).expression == "(V) * 3 = (R * I) * 3"
    assert (3 * eq).expression == "3 * (V) = 3 * (R * I)"
    assert (eq / 3).expression == "(V) / 3 = (R * I) / 3"
    assert (3 / eq).expression == "3 / (V) = 3 / (R * I)"
    assert (eq**2).expression == "(V) ^ 2 = (R * I) ^ 2"

    with_unit = (eq + Q_(5.0, "V")).expression
    assert with_unit.startswith("(V) + (5.0 ")
    assert " = (R * I) + (5.0 " in with_unit


def test_substituting_replaces_whole_names_only():
    eq = PhyEquation("V = R * I + R2")
    assert eq.substitute("R", "a + b").expression == "V = (a + b) * I + R2"
    with_equation = eq.substitute("V", PhyEquation("W = 3 * U"))
    assert with_equation.lhs == "(3 * U)"


def test_a_function_is_called_by_position_by_name_and_by_composition(interp):
    f = PhyFunction(interp, "f", "f(x, y) = x - y")
    assert math.isclose(float(f(Q_(5.0, ""), Q_(2.0, ""))), 3.0)
    assert math.isclose(float(f(y=2, x=7)), 5.0)
    with pytest.raises(
        ValueError, match="Missing parameter 'y' for function 'f'"
    ):
        f(x=1)

    g = PhyFunction(interp, "g", "g(t) = 2 * t")
    composed = f(g)
    assert composed.get_params() == ["t", "y"]
    assert math.isclose(float(composed(3, 1)), 5.0)


def test_functions_combine_by_operators_and_refuse_other_operands(interp):
    f = PhyFunction(interp, "f", "f(x) = 2 * x")
    g = PhyFunction(interp, "g", "g(x, y) = x + y")
    assert math.isclose(float((f + g)(3, 1)), 10.0)
    assert math.isclose(float((g - f)(3, 1)), -2.0)
    assert math.isclose(float((f * g)(3, 1)), 24.0)
    assert math.isclose(float((g / f)(3, 1)), 4.0 / 6.0)
    assert (f + g).get_params() == ["x", "y"]

    for operation in (
        lambda: f + 1,
        lambda: f - 1,
        lambda: f * 1,
        lambda: f / 1,
    ):
        with pytest.raises(TypeError, match="another PhyFunction"):
            operation()
    other = PhyFunction(Interpreter(), "h", "h(x) = x")
    with pytest.raises(ValueError, match="same Interpreter"):
        f + other
    with pytest.raises(ValueError, match="same Interpreter"):
        f.compose(other)


def test_calculus_needs_a_function_with_parameters(interp):
    constant = PhyFunction(interp, "k", "k() = 4")
    for operation in (
        lambda: constant.deriv("x"),
        lambda: constant.integral("x"),
        lambda: constant.solve("x"),
    ):
        with pytest.raises(ValueError, match="no parameters"):
            operation()
    with pytest.raises(ValueError, match="at least one parameter"):
        constant.compose(PhyFunction(interp, "g", "g(t) = t"))


def test_a_python_function_can_stand_in_for_a_physical_one():
    @phy_function
    def double(x):
        return 2 * x

    assert double.name == "double"
    assert double.get_params() == ["x"]
    assert double(4) == 8

    @phy_function(name="triple", params=["a"])
    def thrice(a):
        return 3 * a

    assert (thrice.name, thrice.get_params(), thrice(2)) == (
        "triple",
        ["a"],
        6,
    )


def test_the_last_result_of_an_evaluation_is_a_quantity_or_none():
    from physure import symbolic
    from physure._core import evaluate_phs_native

    assert symbolic._last_result([]) is None
    assert symbolic._last_result(None) is None
    assert symbolic._last_result([1.0, "x"]) == "x"
    wrapped = symbolic._last_result(evaluate_phs_native("3 m\n"))
    assert isinstance(wrapped, symbolic.Quantity)
    assert float(wrapped.magnitude) == 3.0


def test_an_argument_is_written_as_phs_source():
    from physure import symbolic

    assert symbolic._argument_source(4) == "4"
    assert symbolic._argument_source(Q_(2.0, "m")) == "2.0 m"
