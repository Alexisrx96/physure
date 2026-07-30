from physure._core import evaluate_phs_native


def test_evaluate_phs_native_simple_math():
    results = evaluate_phs_native("10 + 20")
    assert results == [30.0]


def test_evaluate_phs_native_variables_and_functions():
    source = """
    x = 5
    y = 10
    f(a, b) = a * b + 2
    f(x, y)
    """
    results = evaluate_phs_native(source)
    assert results[-1] == 52.0


def test_evaluate_phs_native_where_and_ternary():
    results = evaluate_phs_native("z > 2 ? 100 : 200 where z = 3")
    assert results == [100.0]
