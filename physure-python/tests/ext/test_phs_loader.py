import pytest

from physure._core import Interpreter
from physure.ext.phs_loader import load_ext_functions


def test_load_ext_functions_registers_and_evaluates(tmp_path):
    ext_dir = tmp_path / "ext"
    ext_dir.mkdir()
    (ext_dir / "helpers.py").write_text(
        "def double(x):\n    return x * 2\n\ndef _hidden(x):\n    return x\n"
    )

    interp = Interpreter(base_dir=str(tmp_path))
    registered = load_ext_functions(interp, tmp_path)

    assert registered == ["double"]
    results = interp.evaluate("double(21)")
    assert results[-1] == 42.0


def test_load_ext_functions_no_ext_dir_returns_empty(tmp_path):
    interp = Interpreter(base_dir=str(tmp_path))
    assert load_ext_functions(interp, tmp_path) == []


def test_a_host_function_cannot_take_a_reserved_name(tmp_path):
    interpreter = Interpreter(base_dir=str(tmp_path))
    with pytest.raises(
        ValueError, match="cannot be bound as a function a host registers"
    ):
        interpreter.register_function(lambda *args: 1.0, "max")
    interpreter.register_function(
        lambda *args: 2.0, "double"
    )  # a free name registers


def test_the_loader_says_which_file_took_a_reserved_name(tmp_path):
    ext_dir = tmp_path / "ext"
    ext_dir.mkdir()
    (ext_dir / "helpers.py").write_text("def max(*args):\n    return 1.0\n")
    interp = Interpreter(base_dir=str(tmp_path))
    with pytest.raises(ValueError, match=r"helpers.py") as info:
        load_ext_functions(interp, tmp_path)
    assert "'max'" in str(info.value)
    assert "cannot be bound as a function a host registers" in str(info.value)
