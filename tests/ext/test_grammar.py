import math

import pytest

from measurekit.domain.exceptions import (
    DimensionError,
    IncompatibleUnitsError,
    UnknownUnitError,
)
from measurekit.ext.grammar import (
    _FUNCTIONS,
    GrammarError,
    GrammarInterpreter,
    evaluate,
)


@pytest.fixture
def mn():
    return GrammarInterpreter()


def test_assignment_and_query(mn):
    mn.run("force = 500 N")
    result = mn.eval("force = ?")
    assert math.isclose(result.magnitude, 500)
    assert math.isclose(result.to("N").magnitude, 500)


def test_arrow_assignment_sugar(mn):
    mn.run("force -> 500 N")
    assert math.isclose(mn["force"].magnitude, 500)


def test_derived_calculation_and_assertion(mn):
    mn.run(
        """
        force = 500 N
        area = 2 m^2
        stress = force / area
        """
    )
    assert mn.eval("stress == 250 Pa") is True
    assert mn.eval("stress == 251 Pa") is False


def test_conversion(mn):
    mn.run("d = 1500 m")
    result = mn.eval("d => km")
    assert math.isclose(result.magnitude, 1.5)


def test_assignment_with_conversion(mn):
    mn.run("f = 500 N => kN")
    assert math.isclose(mn["f"].magnitude, 0.5)


def test_assignment_query_returns_value(mn):
    result = mn.eval("v = 10 m / 2 s = ?")
    assert math.isclose(result.magnitude, 5)
    assert math.isclose(mn["v"].magnitude, 5)


def test_implicit_mul_precedence(mn):
    # `500 N / 2 m^2` must parse as (500*N) / (2*m^2), not 500*N/2*m^2.
    result = mn.eval("500 N / 2 m^2 => Pa")
    assert math.isclose(result.magnitude, 250)


def test_compound_units_and_powers(mn):
    ke = mn.eval("0.5 * 2 kg * (3 m/s)^2 => J")
    assert math.isclose(ke.magnitude, 9.0)


def test_superscript_exponent(mn):
    result = mn.eval("2 m² == 2 m^2")
    assert result is True


def test_uncertainty_literal(mn):
    mn.run("g = 9.81 +/- 0.02 m/s^2")
    g = mn["g"]
    assert math.isclose(g.magnitude, 9.81)
    assert math.isclose(float(g.std_dev), 0.02, rel_tol=1e-6)


def test_comments_and_semicolons(mn):
    results = mn.run("a = 1 m; b = 2 m  # total\na + b = ?")
    assert math.isclose(results[-1].magnitude, 3)


def test_assignments_return_none(mn):
    assert mn.run("x = 5 m") == [None]


def test_bare_expression(mn):
    result = mn.eval("2 + 3")
    assert result == 5


def test_variables_shadow_units(mn):
    mn.run("m = 5 kg")  # shadows the meter
    assert math.isclose(mn["m"].magnitude, 5)


def test_unknown_unit_raises(mn):
    with pytest.raises(UnknownUnitError, match="furlonx"):
        mn.eval("x = 5 furlonx")


def test_incompatible_assertion_raises(mn):
    with pytest.raises(IncompatibleUnitsError):
        mn.eval("5 m == 5 s")


def test_invalid_assignment_target(mn):
    with pytest.raises(GrammarError):
        mn.eval("2 x = 5 m")


def test_unclosed_paren(mn):
    with pytest.raises(GrammarError):
        mn.eval("(2 + 3 m")


def test_one_shot_evaluate():
    assert evaluate("x = 2 m; y = 3 m; x * y == 6 m^2") is True


def test_negative_and_scientific(mn):
    result = mn.eval("-1.5e3 m => km")
    assert math.isclose(result.magnitude, -1.5)


def test_grammar_base_conversion(mn):
    q = mn.eval("500 N => base")
    assert str(q.unit) == "kg·m/s²"
    assert math.isclose(q.magnitude, 500)



def test_unicode_multiplication_and_division_operators(mn):
    assert mn.eval("2 × 3") == 6  # noqa: RUF001
    assert mn.eval("6 ÷ 2") == 3


def test_sqrt_unicode_prefix_parenthesized(mn):
    result = mn.eval("√(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_sqrt_unicode_prefix_bare(mn):
    mn.run("x = 16")
    assert math.isclose(mn.eval("√x"), 4)


def test_sqrt_ascii_function_form(mn):
    result = mn.eval("sqrt(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_abs_function(mn):
    result = mn.eval("abs(-3 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_abs_function_on_bare_number(mn):
    assert mn.eval("abs(-5)") == 5


def test_abs_function_identity_on_positive_quantity(mn):
    result = mn.eval("abs(3 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_abs_function_preserves_uncertainty(mn):
    result = mn.eval("abs(-3 +/- 0.1 m)")
    assert math.isclose(result.magnitude, 3)
    assert math.isclose(result.uncertainty, 0.1)


def test_abs_function_on_zero(mn):
    result = mn.eval("abs(0 m)")
    assert math.isclose(result.to("m").magnitude, 0)


def test_function_call_wrong_arity_raises(mn):
    with pytest.raises(GrammarError, match="abs"):
        mn.eval("abs(1 m, 2 m)")


def test_sqrt_function_still_works_after_migration(mn):
    # Regression: sqrt(...) used to be a hardcoded special case in _atom();
    # it now goes through the generic _FUNCTIONS dispatch table instead.
    result = mn.eval("sqrt(9 m^2)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_round_function(mn):
    result = mn.eval("round(3.7 m)")
    assert math.isclose(result.to("m").magnitude, 4)


def test_round_function_with_ndigits(mn):
    result = mn.eval("round(3.14159 m, 2)")
    assert math.isclose(result.to("m").magnitude, 3.14)


def test_round_function_wrong_arity_raises(mn):
    with pytest.raises(GrammarError, match="round"):
        mn.eval("round(3.14 m, 1, 2)")


def test_floor_function(mn):
    result = mn.eval("floor(3.7 m)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_ceil_function(mn):
    result = mn.eval("ceil(3.2 m)")
    assert math.isclose(result.to("m").magnitude, 4)


def test_floor_function_negative(mn):
    result = mn.eval("floor(-3.2 m)")
    assert math.isclose(result.to("m").magnitude, -4)


def test_ceil_function_negative(mn):
    result = mn.eval("ceil(-3.2 m)")
    assert math.isclose(result.to("m").magnitude, -3)


def test_min_function_cross_unit(mn):
    # 200 cm == 2 m, so the smaller of (3 m, 200 cm) is 200 cm/2 m.
    result = mn.eval("min(3 m, 200 cm)")
    assert math.isclose(result.to("m").magnitude, 2)


def test_max_function_cross_unit(mn):
    result = mn.eval("max(3 m, 200 cm)")
    assert math.isclose(result.to("m").magnitude, 3)


def test_min_function_incompatible_units_raises(mn):
    with pytest.raises(IncompatibleUnitsError):
        mn.eval("min(3 m, 2 s)")


def test_min_function_variadic(mn):
    result = mn.eval("min(5 m, 1 m, 3 m)")
    assert math.isclose(result.to("m").magnitude, 1)


def test_max_function_variadic(mn):
    result = mn.eval("max(5 m, 1 m, 3 m)")
    assert math.isclose(result.to("m").magnitude, 5)


def test_sqrt_of_negative_returns_complex(mn):
    # Matches Python's own `(-4) ** 0.5` semantics: no unit involved, so
    # this is bare-number arithmetic, not a Quantity/GrammarError concern.
    result = mn.eval("sqrt(-4)")
    assert isinstance(result, complex)
    assert math.isclose(result.imag, 2.0)


_LOG_DOMAIN_ERROR = "domain error|positive input"


def test_log_of_zero_raises(mn):
    # Message wording varies by Python version ("math domain error" vs.
    # "expected a positive input" on 3.14+).
    with pytest.raises(ValueError, match=_LOG_DOMAIN_ERROR):
        mn.eval("log(0)")


def test_log_of_negative_raises(mn):
    with pytest.raises(ValueError, match=_LOG_DOMAIN_ERROR):
        mn.eval("log(-1)")


def test_sin_function_dimensionless(mn):
    assert math.isclose(mn.eval("sin(0)"), 0.0, abs_tol=1e-12)


def test_sin_function_angle_unit(mn):
    result = mn.eval("sin(90 deg)")
    assert math.isclose(result.magnitude, 1.0, abs_tol=1e-9)


def test_cos_function_angle_unit(mn):
    result = mn.eval("cos(0 rad)")
    assert math.isclose(result.magnitude, 1.0, abs_tol=1e-9)


def test_tan_function_dimensionless(mn):
    assert math.isclose(mn.eval("tan(0)"), 0.0, abs_tol=1e-12)


def test_sin_function_wrong_dimension_raises(mn):
    with pytest.raises(DimensionError):
        mn.eval("sin(3 kg)")


def test_exp_function_dimensionless(mn):
    assert math.isclose(mn.eval("exp(0)"), 1.0)


def test_log_function_dimensionless(mn):
    assert math.isclose(mn.eval("log(1)"), 0.0, abs_tol=1e-12)


def test_ln_function_is_natural_log(mn):
    # ln and log are the same implementation (no log10 exists in the engine).
    assert mn.eval("ln(1)") == mn.eval("log(1)")


def test_log_function_wrong_dimension_raises(mn):
    with pytest.raises(DimensionError):
        mn.eval("log(3 kg)")


@pytest.mark.parametrize("name", sorted(_FUNCTIONS))
def test_function_names_are_reserved_assignment_targets(mn, name):
    with pytest.raises(GrammarError):
        mn.eval(f"{name} = 5 m")


def test_comparison_operators(mn):
    assert mn.eval("3 < 5") is True
    assert mn.eval("3 > 5") is False
    assert mn.eval("3 <= 3") is True
    assert mn.eval("4 >= 5") is False
    assert mn.eval("3 != 4") is True


def test_ternary_true_branch(mn):
    assert mn.eval("1 < 2 ? 10 : 20") == 10


def test_ternary_false_branch(mn):
    assert mn.eval("1 > 2 ? 10 : 20") == 20


def test_ternary_nested_false_branch(mn):
    result = mn.eval("1 > 2 ? 1 : (2 > 3 ? 20 : 30)")
    assert result == 30


def test_ternary_with_quantities(mn):
    result = mn.eval("5 m > 3 m ? 5 m : 3 m")
    assert math.isclose(result.to("m").magnitude, 5)


def test_ternary_inside_function_call_args(mn):
    result = mn.eval("max(1 < 2 ? 5 : 1, 3)")
    assert result == 5


def test_user_function_basic_call(mn):
    mn.run("f(x) = x^2")
    assert mn.eval("f(3)") == 9


def test_user_function_multi_param(mn):
    mn.run("area(w, h) = w * h")
    result = mn.eval("area(3 m, 4 m)")
    assert math.isclose(result.to("m^2").magnitude, 12)


def test_user_function_wrong_arity_raises(mn):
    mn.run("f(x) = x^2")
    with pytest.raises(GrammarError, match="f"):
        mn.eval("f(1, 2)")


def test_user_function_shadowing_builtin_raises(mn):
    with pytest.raises(GrammarError):
        mn.eval("abs(x) = x")


def test_variable_then_function_namespace_collision(mn):
    mn.run("f = 5")
    with pytest.raises(GrammarError):
        mn.eval("f(x) = x^2")


def test_function_then_variable_namespace_collision(mn):
    mn.run("f(x) = x^2")
    with pytest.raises(GrammarError):
        mn.eval("f = 5")


def test_user_function_redefinition_allowed(mn):
    mn.run("f(x) = x^2")
    mn.run("f(x) = x^3")
    assert mn.eval("f(2)") == 8


def test_user_function_call_inside_larger_expression(mn):
    mn.run("f(x) = x + 1")
    assert mn.eval("f(2) * 3") == 9


def test_recursion_factorial(mn):
    mn.run("fact(n) = n <= 1 ? 1 : n * fact(n - 1)")
    assert mn.eval("fact(5)") == 120


def test_recursion_fibonacci(mn):
    mn.run("fib(n) = n <= 1 ? n : fib(n - 1) + fib(n - 2)")
    assert mn.eval("fib(10)") == 55


def test_recursion_without_base_case_hits_limit(mn):
    mn.run("loop(n) = loop(n + 1)")
    with pytest.raises(GrammarError, match="recursion limit"):
        mn.eval("loop(0)")


def test_recursion_custom_limit(mn):
    original = mn.system.settings.get("mkml_recursion_limit")
    mn.system.settings["mkml_recursion_limit"] = "5"
    try:
        mn.run("loop(n) = loop(n + 1)")
        with pytest.raises(GrammarError, match=r"recursion limit \(5\)"):
            mn.eval("loop(0)")
    finally:
        if original is None:
            mn.system.settings.pop("mkml_recursion_limit", None)
        else:
            mn.system.settings["mkml_recursion_limit"] = original


def test_typed_parameter_valid_dimension(mn):
    mn.run("double_len(x: m) = x * 2")
    result = mn.eval("double_len(3 m)")
    assert math.isclose(result.to("m").magnitude, 6)


def test_typed_parameter_auto_converts_compatible_unit(mn):
    mn.run("double_len(x: m) = x * 2")
    result = mn.eval("double_len(300 cm)")
    assert math.isclose(result.to("m").magnitude, 6)


def test_typed_parameter_incompatible_dimension_raises(mn):
    mn.run("double_len(x: m) = x * 2")
    with pytest.raises(DimensionError):
        mn.eval("double_len(3 kg)")


def test_typed_parameter_bare_number_raises(mn):
    mn.run("double_len(x: m) = x * 2")
    with pytest.raises(DimensionError):
        mn.eval("double_len(3)")


def test_untyped_parameter_accepts_anything(mn):
    mn.run("identity(x) = x")
    assert mn.eval("identity(5)") == 5
    result = mn.eval("identity(3 kg)")
    assert math.isclose(result.to("kg").magnitude, 3)


def test_typed_and_untyped_parameters_mixed(mn):
    mn.run("scale(x: m, k) = x * k")
    result = mn.eval("scale(3 m, 2)")
    assert math.isclose(result.to("m").magnitude, 6)


def test_let_binding_inside_function_body(mn):
    mn.run("f(x) = let y = x^2 in y + 1")
    assert mn.eval("f(3)") == 10


def test_let_binding_nested(mn):
    mn.run("f(x) = let a = x + 1 in let b = a * 2 in b")
    assert mn.eval("f(3)") == 8


def test_let_at_top_level_raises(mn):
    with pytest.raises(
        GrammarError, match="only valid inside a function body"
    ):
        mn.eval("let y = 5 in y + 1")


def test_in_still_resolves_as_inches_outside_let(mn):
    result = mn.eval("5 in")
    assert math.isclose(result.to("in").magnitude, 5)


def test_display_text_block_inline(mn):
    results = mn.run("```Hello world```")
    assert results == ["Hello world"]


def test_display_text_block_multiline(mn):
    results = mn.run("```\nLine one\nLine two\n```")
    assert results == ["Line one\nLine two"]


def test_display_text_block_with_hash_and_backtick_adjacent_chars(mn):
    results = mn.run("```price is #5 and uses ` backtick```")
    assert results == ["price is #5 and uses ` backtick"]


def test_display_text_block_interleaved_with_statements(mn):
    results = mn.run("x = 5 m\n```note```\nx => m")
    assert results[0] is None
    assert results[1] == "note"
    assert math.isclose(results[2].magnitude, 5)


def test_script_with_no_blocks_unaffected(mn):
    results = mn.run("a = 1 m\nb = 2 m\na + b = ?")
    assert math.isclose(results[-1].magnitude, 3)


def test_indented_multiline_function(mn):
    mn.run("""
calcular_energia_k(m: kg, v: m/s) =
    v_cuadrado = v^2
    0.5 * m * v_cuadrado
""")
    result = mn.eval("calcular_energia_k(70 kg, 12 m/s)")
    assert math.isclose(result.to("J").magnitude, 5040)


def test_indented_multiline_function_scope_isolation(mn):
    mn.run("""
compute_val(x) =
    temp_var = x * 10
    temp_var + 5
""")
    assert mn.eval("compute_val(2)") == 25
    with pytest.raises(UnknownUnitError):
        mn.eval("temp_var")


def test_inline_significant_figures_formatting(mn):
    assert math.isclose(mn.eval("123.456 m : 2").magnitude, 120.0)
    assert math.isclose(mn.eval("123.456 m : 4").magnitude, 123.5)
    result = mn.eval("x = 500.123 N => kN : 2 = ?")
    assert math.isclose(result.magnitude, 0.5)


