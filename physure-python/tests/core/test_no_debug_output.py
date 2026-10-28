"""Library code must not print debugging output."""

import ast
import pathlib

import physure


def test_the_library_prints_no_debug_output():
    root = pathlib.Path(physure.__file__).parent
    found = []
    for path in sorted(root.rglob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if (
                isinstance(node, ast.Call)
                and isinstance(node.func, ast.Name)
                and node.func.id == "print"
                and node.args
                and "DEBUG" in ast.unparse(node.args[0])
            ):
                found.append(f"{path.relative_to(root)}:{node.lineno}")
    assert not found, f"debug prints left in the library: {found}"
