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
