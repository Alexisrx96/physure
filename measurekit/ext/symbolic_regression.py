"""Dimensionally constrained symbolic regression (roadmap §8).

Searches for a formula relating `inputs` to `target` using genetic
programming over `measurekit.domain.symbolic.native` expression trees,
reusing that module's `infer_unit`/`check_add_compat` so every Add/Sub a
mutation or crossover produces is dimensionally valid by construction
(§8.1's "prune invalid mutations immediately" rule) — no separate
dimensional filter is needed.

The search only combines input symbols structurally (no embedded numeric
constants); the leading scale constant `k` is fit afterward in closed form
(ordinary least squares, zero intercept) and its unit is derived as
`target.unit / candidate.unit` (§8.1's automatic constant synthesis).

# ponytail: unweighted least squares — target/input `Uncertainty` is not
# used to weight the fit. Add chi-square weighting (1/sigma^2) if noisy
# datasets need it; the roadmap's own example doesn't exercise it.
# ponytail: Sin/Cos/Ln/Exp only wrap subtrees that already infer as
# dimensionless, so with a single dimensioned input variable they're
# effectively inert (no dimensionless subtree ever exists to wrap). Full
# support (e.g. auto-dividing by a reference scale) is future work.
"""

from __future__ import annotations

import math
import random
from dataclasses import dataclass
from typing import TYPE_CHECKING

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.units import CompoundUnit
from measurekit.domain.symbolic.native import (
    Add,
    Cos,
    Div,
    Exp,
    Ln,
    Mul,
    Number,
    Pow,
    Sin,
    Sub,
    Symbol,
    check_add_compat,
    infer_unit,
)
from measurekit.domain.symbolic.native import Quantity as _QuantityNode

if TYPE_CHECKING:
    from measurekit.domain.measurement.quantity import Quantity
    from measurekit.domain.symbolic.native import Node

_UNARY_OPS = {"Sin": Sin, "Cos": Cos, "Ln": Ln, "Exp": Exp}
_MAX_UNIT_RETRIES = 4
_LEAF_PROBABILITY = 0.35


def _children(node: Node) -> tuple[Node, ...]:
    if isinstance(node, Add):
        return node.terms
    if isinstance(node, Mul):
        return node.factors
    if isinstance(node, (Sub, Div)):
        return (node.left, node.right)
    if isinstance(node, Pow):
        return (node.base, node.exponent)
    if isinstance(node, (Sin, Cos, Ln, Exp)):
        return (node.arg,)
    return ()


def _node_count(node: Node) -> int:
    return 1 + sum(_node_count(c) for c in _children(node))


def _evaluate(node: Node, bindings: dict[str, float]) -> float:
    if isinstance(node, Number):
        return node.value
    if isinstance(node, (Symbol, _QuantityNode)):
        return bindings[node.name]
    if isinstance(node, Add):
        return sum(_evaluate(t, bindings) for t in node.terms)
    if isinstance(node, Sub):
        return _evaluate(node.left, bindings) - _evaluate(node.right, bindings)
    if isinstance(node, Mul):
        result = 1.0
        for factor in node.factors:
            result *= _evaluate(factor, bindings)
        return result
    if isinstance(node, Div):
        return _evaluate(node.left, bindings) / _evaluate(node.right, bindings)
    if isinstance(node, Pow):
        return _evaluate(node.base, bindings) ** _evaluate(
            node.exponent, bindings
        )
    if isinstance(node, Sin):
        return math.sin(_evaluate(node.arg, bindings))
    if isinstance(node, Cos):
        return math.cos(_evaluate(node.arg, bindings))
    if isinstance(node, Ln):
        return math.log(_evaluate(node.arg, bindings))
    if isinstance(node, Exp):
        return math.exp(_evaluate(node.arg, bindings))
    raise TypeError(f"Unknown node type: {type(node)!r}")  # pragma: no cover


def _format(node: Node) -> str:
    if isinstance(node, Number):
        return f"{node.value:.4g}"
    if isinstance(node, (Symbol, _QuantityNode)):
        return node.name
    if isinstance(node, Add):
        return " + ".join(_format(t) for t in node.terms)
    if isinstance(node, Sub):
        return f"({_format(node.left)} - {_format(node.right)})"
    if isinstance(node, Mul):
        return " * ".join(_format(f) for f in node.factors)
    if isinstance(node, Div):
        return f"({_format(node.left)} / {_format(node.right)})"
    if isinstance(node, Pow):
        return f"{_format(node.base)}^{_format(node.exponent)}"
    if isinstance(node, Sin):
        return f"sin({_format(node.arg)})"
    if isinstance(node, Cos):
        return f"cos({_format(node.arg)})"
    if isinstance(node, Ln):
        return f"ln({_format(node.arg)})"
    if isinstance(node, Exp):
        return f"exp({_format(node.arg)})"
    raise TypeError(f"Unknown node type: {type(node)!r}")  # pragma: no cover


def _combine(op: str, left: Node, right: Node) -> Node:
    if op == "Add":
        check_add_compat(left, right)
        return Add((left, right))
    if op == "Sub":
        check_add_compat(left, right)
        return Sub(left, right)
    if op == "Mul":
        return Mul((left, right))
    if op == "Div":
        return Div(left, right)
    raise ValueError(op)  # pragma: no cover


def _random_tree(
    depth: int, terminals: list[Node], allowed: set[str], rng: random.Random
) -> Node:
    if depth <= 0 or not allowed or rng.random() < _LEAF_PROBABILITY:
        return rng.choice(terminals)
    op = rng.choice(sorted(allowed))
    if op in ("Add", "Sub"):
        for _ in range(_MAX_UNIT_RETRIES):
            left = _random_tree(depth - 1, terminals, allowed, rng)
            right = _random_tree(depth - 1, terminals, allowed, rng)
            try:
                return _combine(op, left, right)
            except (IncompatibleUnitsError, ValueError):
                continue
        return rng.choice(terminals)
    if op in ("Mul", "Div"):
        left = _random_tree(depth - 1, terminals, allowed, rng)
        right = _random_tree(depth - 1, terminals, allowed, rng)
        return _combine(op, left, right)
    if op == "Pow":
        base = _random_tree(depth - 1, terminals, allowed, rng)
        return Pow(base, Number(float(rng.choice((2, 3)))))
    if op in _UNARY_OPS:
        arg = _random_tree(depth - 1, terminals, allowed, rng)
        unit = infer_unit(arg)
        if unit is not None and unit.exponents:
            return rng.choice(
                terminals
            )  # dimensioned arg — not a valid transcendental input
        return _UNARY_OPS[op](arg)
    return rng.choice(terminals)  # pragma: no cover


def _collect_nodes(node: Node) -> list[Node]:
    nodes = [node]
    for child in _children(node):
        nodes.extend(_collect_nodes(child))
    return nodes


def _replace_subtree(node: Node, target: Node, replacement: Node) -> Node:
    if node is target:
        return replacement
    if isinstance(node, Add):
        return Add(
            tuple(_replace_subtree(t, target, replacement) for t in node.terms)
        )
    if isinstance(node, Mul):
        return Mul(
            tuple(
                _replace_subtree(f, target, replacement) for f in node.factors
            )
        )
    if isinstance(node, Sub):
        return Sub(
            _replace_subtree(node.left, target, replacement),
            _replace_subtree(node.right, target, replacement),
        )
    if isinstance(node, Div):
        return Div(
            _replace_subtree(node.left, target, replacement),
            _replace_subtree(node.right, target, replacement),
        )
    if isinstance(node, Pow):
        return Pow(
            _replace_subtree(node.base, target, replacement), node.exponent
        )
    if isinstance(node, (Sin, Cos, Ln, Exp)):
        return type(node)(_replace_subtree(node.arg, target, replacement))
    return node


def _mutate(
    individual: Node,
    terminals: list[Node],
    allowed: set[str],
    rng: random.Random,
    depth_budget: int,
) -> Node:
    target = rng.choice(_collect_nodes(individual))
    replacement = _random_tree(depth_budget, terminals, allowed, rng)
    mutated = _replace_subtree(individual, target, replacement)
    try:
        infer_unit(mutated)
    except (IncompatibleUnitsError, ValueError):
        return individual  # mutation pruned (§8.1) — keep the parent
    return mutated


def _crossover(a: Node, b: Node, rng: random.Random) -> Node:
    target = rng.choice(_collect_nodes(a))
    donor = rng.choice(_collect_nodes(b))
    child = _replace_subtree(a, target, donor)
    try:
        infer_unit(child)
    except (IncompatibleUnitsError, ValueError):
        return a  # crossover pruned (§8.1) — keep parent a
    return child


def _fit_scale_and_sse(
    node: Node, rows: list[dict[str, float]], targets: list[float]
) -> tuple[float, float] | None:
    """Closed-form least-squares scale `k` minimizing sum((k*x - y)^2)."""
    xs = []
    for row in rows:
        try:
            xs.append(_evaluate(node, row))
        except (ZeroDivisionError, ValueError, OverflowError):
            return None
    sum_xx = sum(x * x for x in xs)
    if sum_xx == 0.0 or not math.isfinite(sum_xx):
        return None
    sum_xy = sum(x * y for x, y in zip(xs, targets, strict=True))
    k = sum_xy / sum_xx
    if not math.isfinite(k):
        return None
    sse = sum((k * x - y) ** 2 for x, y in zip(xs, targets, strict=True))
    if not math.isfinite(sse):
        return None
    return k, sse


@dataclass(frozen=True)
class FittedConstant:
    """A regression-synthesized constant with its inferred physical unit."""

    value: float
    units: CompoundUnit


@dataclass(frozen=True)
class FittedFormula:
    """The best formula `SymbolicRegressor.fit()` found."""

    node: Node
    formula_string: str
    constants: dict[str, FittedConstant]

    def __call__(self, **bindings: float) -> float:
        """Evaluates the formula at the given input values."""
        return self.constants["k"].value * _evaluate(self.node, bindings)


class SymbolicRegressor:
    """Discovers a dimensionally consistent formula fitting `target`.

    Runs a genetic search over expression trees built from `inputs` using
    `allowed_operations`; every candidate is dimensionally valid by
    construction, so the search never wastes evaluations on nonsense like
    adding meters to kilograms (§8.1). The winning tree's leading scale
    constant `k` — and its unit — are then fit in closed form.

    >>> from measurekit import Q_
    >>> t = Q_([1.0, 2.0, 3.0, 4.0, 5.0], "s", symbol="t")
    >>> s = Q_([4.9, 19.5, 44.0, 78.3, 122.1], "m", symbol="s")
    >>> regressor = SymbolicRegressor(
    ...     inputs={"t": t},
    ...     target=s,
    ...     allowed_operations=["Add", "Mul", "Pow"],
    ...     max_complexity=10,
    ...     seed=0,
    ... )
    >>> best_fit = regressor.fit()
    >>> best_fit.formula_string
    's = 4.887 * t * t'
    >>> best_fit.constants["k"].units
    CompoundUnit(exponents={'m': 1, 's': -2})
    """

    def __init__(
        self,
        inputs: dict[str, Quantity],
        target: Quantity,
        allowed_operations: list[str],
        max_complexity: int = 10,
        population_size: int = 40,
        generations: int = 25,
        seed: int | None = None,
    ) -> None:
        self.inputs = inputs
        self.target = target
        self.allowed_operations = set(allowed_operations)
        self.max_complexity = max_complexity
        self.population_size = population_size
        self.generations = generations
        self._rng = random.Random(seed)

    def fit(self) -> FittedFormula:
        """Runs the genetic search and returns the best formula found."""
        terminals: list[Node] = [
            _QuantityNode(name, q.unit) for name, q in self.inputs.items()
        ]
        rows = [
            dict(zip(self.inputs, values, strict=True))
            for values in zip(
                *(list(q.magnitude) for q in self.inputs.values()), strict=True
            )
        ]
        targets = [float(v) for v in self.target.magnitude]
        depth = max(1, self.max_complexity // 3)

        population = [
            _random_tree(depth, terminals, self.allowed_operations, self._rng)
            for _ in range(self.population_size)
        ]

        def scored(
            individual: Node,
        ) -> tuple[float, int, Node, float | None]:
            fit = _fit_scale_and_sse(individual, rows, targets)
            if fit is None or _node_count(individual) > self.max_complexity:
                return (math.inf, 2**31, individual, None)
            k, sse = fit
            return (sse, _node_count(individual), individual, k)

        best: tuple[float, int, Node, float | None] | None = None
        for _ in range(self.generations):
            ranked = sorted(
                (scored(ind) for ind in population),
                key=lambda r: (round(r[0], 6), r[1]),
            )
            candidate_key = (round(ranked[0][0], 6), ranked[0][1])
            if best is None or candidate_key < (round(best[0], 6), best[1]):
                best = ranked[0]
            survivors = [
                r[2] for r in ranked[: max(2, self.population_size // 4)]
            ]
            next_gen = list(survivors[:2])  # elitism
            while len(next_gen) < self.population_size:
                child = _crossover(
                    self._rng.choice(survivors),
                    self._rng.choice(survivors),
                    self._rng,
                )
                child = _mutate(
                    child, terminals, self.allowed_operations, self._rng, depth
                )
                next_gen.append(child)
            population = next_gen

        assert best is not None  # population_size >= 1 guarantees a candidate
        _, _, node, k = best
        if k is None:
            raise RuntimeError(
                "SymbolicRegressor found no dimensionally valid formula — "
                "widen allowed_operations or max_complexity"
            )

        candidate_unit = infer_unit(node) or CompoundUnit({})
        k_unit = self.target.unit / candidate_unit
        output_name = getattr(self.target, "symbol", None) or "y"
        formula_string = f"{output_name} = {k:.4g} * {_format(node)}"
        return FittedFormula(
            node=node,
            formula_string=formula_string,
            constants={"k": FittedConstant(k, k_unit)},
        )
