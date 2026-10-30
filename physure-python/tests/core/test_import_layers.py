"""The import-layer contract: which part of physure may import which.

The rule and the layer map live in ``import_layers.toml``. This test reads the
package's source with ``ast`` (nothing is imported, so it needs no optional
dependency) and checks three things:

* every module belongs to a layer, so a new file has to be classified;
* no import breaks the rule unless it is recorded as an exception;
* every recorded exception still happens, so the list can only shrink.
"""

from __future__ import annotations

import ast
import collections
import tomllib
from pathlib import Path
from typing import NamedTuple

import pytest

PACKAGE = "physure"
SOURCE = Path(__file__).resolve().parents[2] / PACKAGE
CONTRACT = Path(__file__).with_name("import_layers.toml")

# The compiled extension is not a Python module of this tree.
EXTERNAL = (f"{PACKAGE}._core",)


class Edge(NamedTuple):
    importer: str
    imported: str
    kind: str  # "top" (runs at import), "lazy" (inside a function), "typing"
    path: str
    line: int


def _module_name(path: Path) -> str:
    parts = list(path.relative_to(SOURCE.parent).with_suffix("").parts)
    if parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


class _Collector(ast.NodeVisitor):
    def __init__(self, module: str, path: Path, known: set[str]):
        self.module = module
        self.path = path
        self.known = known
        self.is_package = path.name == "__init__.py"
        self.in_function = 0
        self.in_typing = 0
        self.edges: list[Edge] = []

    def _resolve(self, name: str) -> str | None:
        if name == EXTERNAL[0] or name.startswith(EXTERNAL[0] + "."):
            return None
        parts = name.split(".")
        for end in range(len(parts), 0, -1):
            candidate = ".".join(parts[:end])
            if candidate in self.known:
                return candidate
        return None

    def _add(self, name: str, line: int) -> None:
        target = self._resolve(name)
        if target is None or target == self.module:
            return
        kind = (
            "typing"
            if self.in_typing
            else "lazy"
            if self.in_function
            else "top"
        )
        self.edges.append(
            Edge(self.module, target, kind, str(self.path), line)
        )

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self.in_function += 1
        self.generic_visit(node)
        self.in_function -= 1

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self.in_function += 1
        self.generic_visit(node)
        self.in_function -= 1

    def visit_If(self, node: ast.If) -> None:
        if "TYPE_CHECKING" in ast.unparse(node.test):
            self.in_typing += 1
            for child in node.body:
                self.visit(child)
            self.in_typing -= 1
            for child in node.orelse:
                self.visit(child)
        else:
            self.generic_visit(node)

    def visit_Import(self, node: ast.Import) -> None:
        for alias in node.names:
            if alias.name == PACKAGE or alias.name.startswith(PACKAGE + "."):
                self._add(alias.name, node.lineno)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        base = node.module or ""
        if node.level:
            package = self.module.split(".")
            if not self.is_package:
                package = package[:-1]
            package = package[: len(package) - (node.level - 1)]
            base = ".".join(
                [*package, *([node.module] if node.module else [])]
            )
        if not (base == PACKAGE or base.startswith(PACKAGE + ".")):
            return
        if base == EXTERNAL[0] or base.startswith(EXTERNAL[0] + "."):
            return
        found_submodule = False
        for (
            alias
        ) in node.names:  # `from physure.a import b` may import module b
            if f"{base}.{alias.name}" in self.known:
                self._add(f"{base}.{alias.name}", node.lineno)
                found_submodule = True
        if not found_submodule:
            self._add(base, node.lineno)


def _read_edges() -> tuple[set[str], list[Edge]]:
    paths = sorted(SOURCE.rglob("*.py"))
    known = {_module_name(path) for path in paths}
    edges: list[Edge] = []
    for path in paths:
        collector = _Collector(_module_name(path), path, known)
        collector.visit(ast.parse(path.read_text(encoding="utf-8")))
        edges.extend(collector.edges)
    return known, edges


class Contract(NamedTuple):
    rank: dict
    patterns: list
    lazy_may_reach: set
    exceptions: dict


def _load_contract() -> Contract:
    data = tomllib.loads(CONTRACT.read_text(encoding="utf-8"))
    rank = {layer["name"]: index for index, layer in enumerate(data["layer"])}
    patterns = [
        (pattern, layer["name"])
        for layer in data["layer"]
        for pattern in layer["modules"]
    ]
    exceptions = {
        (item["importer"], item["imported"], item["kind"]): item.get(
            "count", 1
        )
        for item in data.get("exception", [])
    }
    return Contract(rank, patterns, set(data["lazy_may_reach"]), exceptions)


def _layer_of(module: str, contract: Contract) -> str | None:
    best: tuple[int, str] | None = None
    for pattern, layer in contract.patterns:
        if pattern.endswith(".*"):
            base = pattern[:-2]
            hit = module == base or module.startswith(base + ".")
            weight = len(base)
        else:
            hit = module == pattern
            weight = len(pattern) + 1  # an exact module beats its package
        if hit and (best is None or weight > best[0]):
            best = (weight, layer)
    return best[1] if best else None


def _violations(edges, contract) -> dict:
    found = collections.defaultdict(list)
    for edge in edges:
        if edge.kind == "typing":
            continue
        source = _layer_of(edge.importer, contract)
        target = _layer_of(edge.imported, contract)
        if source is None or target is None or source == "facade":
            continue
        if contract.rank[target] <= contract.rank[source]:
            continue
        if edge.kind == "lazy" and target in contract.lazy_may_reach:
            continue
        found[(edge.importer, edge.imported, edge.kind)].append(edge)
    return found


def _toml_block(key, count, cause="<who removes it>") -> str:
    importer, imported, kind = key
    lines = [
        "[[exception]]",
        f'importer = "{importer}"',
        f'imported = "{imported}"',
        f'kind = "{kind}"',
    ]
    if count != 1:
        lines.append(f"count = {count}")
    lines.append(f'cause = "{cause}"')
    return "\n".join(lines)


@pytest.fixture(scope="module")
def analysis():
    known, edges = _read_edges()
    contract = _load_contract()
    return known, edges, contract, _violations(edges, contract)


def test_the_contract_reads_the_whole_package(analysis):
    """Guards against a contract that silently checks nothing."""
    known, edges, _, _ = analysis
    assert len(known) >= 80
    assert len(edges) >= 250


def test_every_module_belongs_to_a_layer(analysis):
    known, _, contract, _ = analysis
    unclassified = sorted(m for m in known if _layer_of(m, contract) is None)
    assert not unclassified, (
        "These modules match no pattern in tests/core/import_layers.toml. "
        "Add each one to the layer it belongs to:\n  "
        + "\n  ".join(unclassified)
    )


def test_no_import_breaks_the_layering_unrecorded(analysis):
    _, _, contract, violations = analysis
    problems = []
    for key, edges in sorted(violations.items()):
        recorded = contract.exceptions.get(key, 0)
        if len(edges) <= recorded:
            continue
        importer, imported, kind = key
        where = ", ".join(f"line {e.line}" for e in edges)
        problems.append(
            f"{importer} ({_layer_of(importer, contract)}) imports "
            f"{imported} ({_layer_of(imported, contract)}), "
            f"{kind}, at {Path(edges[0].path).name} {where}\n"
            f"{_toml_block(key, len(edges))}"
        )
    assert not problems, (
        "Imports that go up the layers (see the rule in import_layers.toml). "
        "Import from a lower layer, or move the code. Only if the import is "
        "temporary and someone owns removing it, record it:\n\n"
        + "\n\n".join(problems)
    )


def test_every_recorded_exception_still_happens(analysis):
    _, _, contract, violations = analysis
    stale = []
    for key, recorded in sorted(contract.exceptions.items()):
        actual = len(violations.get(key, ()))
        if actual != recorded:
            importer, imported, kind = key
            stale.append(
                f"{importer} -> {imported} ({kind}): recorded {recorded}, "
                f"found {actual}"
            )
    assert not stale, (
        "These exceptions in import_layers.toml no longer match the code. "
        "Delete them, or lower their count, now that the imports are gone:\n  "
        + "\n  ".join(stale)
    )
