# physure/domain/symbolic/native.py
"""Pure-Python fallback for the native symbolic-math AST (`physure._core.Expr`).

Mirrors `physure._core/src/symbolic.rs` node-for-node: build, simplify, and
differentiate expressions with physical-dimension checks threaded through
every operator (roadmap `docs/symbolic_math_roadmap.md` §2-§6). `PyExpr` is
always available for explicit fallback testing; `Expr` resolves to the Rust
engine when `physure._core` is importable, else falls back to `PyExpr`.
"""

from __future__ import annotations

from dataclasses import dataclass

from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.units import CompoundUnit


@dataclass(frozen=True)
class Number:
    """A numeric literal leaf."""

    value: float


@dataclass(frozen=True)
class Symbol:
    """A free (unit-less) variable leaf."""

    name: str


@dataclass(frozen=True)
class Quantity:
    """A named, unit-carrying variable leaf."""

    name: str
    unit: CompoundUnit


@dataclass(frozen=True)
class Add:
    """An n-ary sum."""

    terms: tuple[Node, ...]


@dataclass(frozen=True)
class Mul:
    """An n-ary product."""

    factors: tuple[Node, ...]


@dataclass(frozen=True)
class Sub:
    """A binary difference."""

    left: Node
    right: Node


@dataclass(frozen=True)
class Div:
    """A binary quotient."""

    left: Node
    right: Node


@dataclass(frozen=True)
class Pow:
    """A binary exponentiation."""

    base: Node
    exponent: Node


@dataclass(frozen=True)
class Sin:
    """The sine of `arg`."""

    arg: Node


@dataclass(frozen=True)
class Cos:
    """The cosine of `arg`."""

    arg: Node


@dataclass(frozen=True)
class Ln:
    """The natural log of `arg`."""

    arg: Node


@dataclass(frozen=True)
class Exp:
    """`e` raised to `arg`."""

    arg: Node


Node = (
    Number
    | Symbol
    | Quantity
    | Add
    | Mul
    | Sub
    | Div
    | Pow
    | Sin
    | Cos
    | Ln
    | Exp
)


# --- unit inference (§6) ----------------------------------------------


def infer_unit(node: Node) -> CompoundUnit | None:
    """Infers the physical unit of `node`, raising on incompatible dims."""
    if isinstance(node, (Number, Symbol)):
        return None
    if isinstance(node, Quantity):
        return node.unit
    if isinstance(node, Add):
        return _add_like_unit(node.terms)
    if isinstance(node, Sub):
        return _add_like_unit((node.left, node.right))
    if isinstance(node, Mul):
        acc: CompoundUnit | None = None
        for f in node.factors:
            u = infer_unit(f)
            if u is not None:
                acc = u if acc is None else acc * u
        return acc
    if isinstance(node, Div):
        ua, ub = infer_unit(node.left), infer_unit(node.right)
        if ua is not None and ub is not None:
            return ua / ub
        if ua is not None:
            return ua
        if ub is not None:
            return CompoundUnit({}) / ub
        return None
    if isinstance(node, Pow):
        base_unit = infer_unit(node.base)
        if base_unit is None:
            return None
        if isinstance(node.exponent, Number):
            return base_unit**node.exponent.value
        raise ValueError(
            "Cannot raise a dimensioned quantity to a non-constant power"
        )
    if isinstance(node, (Sin, Cos, Ln, Exp)):
        arg_unit = infer_unit(node.arg)
        if arg_unit is not None and arg_unit.exponents:
            raise ValueError(
                "Transcendental function argument must be dimensionless"
            )
        return None
    raise TypeError(f"Unknown node type: {type(node)!r}")


def _add_like_unit(nodes: tuple[Node, ...]) -> CompoundUnit | None:
    result: CompoundUnit | None = None
    for t in nodes:
        u = infer_unit(t)
        if u is not None:
            if result is None:
                result = u
            elif result != u:
                raise IncompatibleUnitsError(result, u)
    return result


def check_add_compat(a: Node, b: Node) -> None:
    """Raises `IncompatibleUnitsError` if `a` and `b` carry different units."""
    ua, ub = infer_unit(a), infer_unit(b)
    if ua is not None and ub is not None and ua != ub:
        raise IncompatibleUnitsError(ua, ub)


# --- differentiation (§4) ----------------------------------------------


def diff_node(node: Node, var: str) -> Node:
    """Symbolic differentiation; unit propagation falls out of `infer_unit`."""
    if isinstance(node, Number):
        return Number(0.0)
    if isinstance(node, Symbol):
        return Number(1.0 if node.name == var else 0.0)
    if isinstance(node, Quantity):
        return Number(1.0 if node.name == var else 0.0)
    if isinstance(node, Add):
        return Add(tuple(diff_node(t, var) for t in node.terms))
    if isinstance(node, Sub):
        return Sub(diff_node(node.left, var), diff_node(node.right, var))
    if isinstance(node, Mul):
        factors = node.factors
        sum_terms = []
        for i in range(len(factors)):
            term_factors = list(factors)
            term_factors[i] = diff_node(factors[i], var)
            sum_terms.append(Mul(tuple(term_factors)))
        return Add(tuple(sum_terms))
    if isinstance(node, Div):
        a, b = node.left, node.right
        da, db = diff_node(a, var), diff_node(b, var)
        numerator = Sub(Mul((da, b)), Mul((a, db)))
        denom = Pow(b, Number(2.0))
        return Div(numerator, denom)
    if isinstance(node, Pow):
        base, exponent = node.base, node.exponent
        if isinstance(exponent, Number):
            n = exponent.value
            db = diff_node(base, var)
            return Mul((Number(n), Pow(base, Number(n - 1.0)), db))
        # ponytail: non-constant exponents (x^y) need the generalized
        # log-derivative rule; add when needed.
        raise NotImplementedError(
            "Differentiation of non-constant exponents is not supported yet"
        )
    if isinstance(node, Sin):
        return Mul((Cos(node.arg), diff_node(node.arg, var)))
    if isinstance(node, Cos):
        return Mul((Number(-1.0), Sin(node.arg), diff_node(node.arg, var)))
    if isinstance(node, Ln):
        return Div(diff_node(node.arg, var), node.arg)
    if isinstance(node, Exp):
        return Mul((Exp(node.arg), diff_node(node.arg, var)))
    raise TypeError(f"Unknown node type: {type(node)!r}")


# --- integration (§4.2, "Level 1") --------------------------------------


def depends_on(node: Node, var: str) -> bool:
    """Whether `node` mentions `var` anywhere in its subtree."""
    if isinstance(node, Number):
        return False
    if isinstance(node, Symbol):
        return node.name == var
    if isinstance(node, Quantity):
        return node.name == var
    if isinstance(node, Add):
        return any(depends_on(t, var) for t in node.terms)
    if isinstance(node, Mul):
        return any(depends_on(f, var) for f in node.factors)
    if isinstance(node, (Sub, Div, Pow)):
        left = node.left if isinstance(node, (Sub, Div)) else node.base
        right = node.right if isinstance(node, (Sub, Div)) else node.exponent
        return depends_on(left, var) or depends_on(right, var)
    if isinstance(node, (Sin, Cos, Ln, Exp)):
        return depends_on(node.arg, var)
    raise TypeError(f"Unknown node type: {type(node)!r}")


def linear_coeff(node: Node, var: str) -> tuple[float, float] | None:
    """Detects `a*var + b`, returning `(a, b)`; `None` if not affine in `var`."""
    if isinstance(node, Number):
        return (0.0, node.value)
    if isinstance(node, Symbol) and node.name == var:
        return (1.0, 0.0)
    if isinstance(node, Quantity) and node.name == var:
        return (1.0, 0.0)
    if isinstance(node, (Symbol, Quantity)):
        return None
    if isinstance(node, Add):
        a = b = 0.0
        for t in node.terms:
            coeffs = linear_coeff(t, var)
            if coeffs is None:
                return None
            a += coeffs[0]
            b += coeffs[1]
        return (a, b)
    if isinstance(node, Sub):
        xa_xb = linear_coeff(node.left, var)
        ya_yb = linear_coeff(node.right, var)
        if xa_xb is None or ya_yb is None:
            return None
        return (xa_xb[0] - ya_yb[0], xa_xb[1] - ya_yb[1])
    if isinstance(node, Mul):
        coeff = 1.0
        lin: tuple[float, float] | None = None
        for f in node.factors:
            if depends_on(f, var):
                if lin is not None:
                    return None
                lin = linear_coeff(f, var)
                if lin is None:
                    return None
            elif isinstance(f, Number):
                coeff *= f.value
            else:
                return None
        la, lb = lin if lin is not None else (0.0, 1.0)
        return (coeff * la, coeff * lb)
    return None


def _arg_form(u: Node, var: str) -> tuple[str, float] | None:
    """Classifies `u`'s dependence on `var` for pattern-table lookup.

    Returns `("var", 0)`, `("linear", a)`, or `("const", 0)`; `None` if
    `u` doesn't match any pattern §4.2's table can integrate.
    """
    if (isinstance(u, Symbol) and u.name == var) or (
        isinstance(u, Quantity) and u.name == var
    ):
        return ("var", 0.0)
    if not depends_on(u, var):
        return ("const", 0.0)
    coeffs = linear_coeff(u, var)
    if coeffs is not None and coeffs[0] != 0.0:
        return ("linear", coeffs[0])
    return None


def integrate_sin(u: Node, var: str) -> Node:
    """`∫sin(u)dx` via the pattern table (§4.2)."""
    neg_cos = Mul((Number(-1.0), Cos(u)))
    form = _arg_form(u, var)
    if form is None:
        raise NotImplementedError(
            "Integration of sin(u) needs u linear in the integration variable"
        )
    kind, a = form
    if kind == "var":
        return neg_cos
    if kind == "linear":
        return Div(neg_cos, Number(a))
    return Mul((Sin(u), Symbol(var)))


def integrate_cos(u: Node, var: str) -> Node:
    """`∫cos(u)dx` via the pattern table (§4.2)."""
    sin_u = Sin(u)
    form = _arg_form(u, var)
    if form is None:
        raise NotImplementedError(
            "Integration of cos(u) needs u linear in the integration variable"
        )
    kind, a = form
    if kind == "var":
        return sin_u
    if kind == "linear":
        return Div(sin_u, Number(a))
    return Mul((Cos(u), Symbol(var)))


def integrate_exp(u: Node, var: str) -> Node:
    """`∫exp(u)dx` via the pattern table (§4.2)."""
    exp_u = Exp(u)
    form = _arg_form(u, var)
    if form is None:
        raise NotImplementedError(
            "Integration of exp(u) needs u linear in the integration variable"
        )
    kind, a = form
    if kind == "var":
        return exp_u
    if kind == "linear":
        return Div(exp_u, Number(a))
    return Mul((exp_u, Symbol(var)))


def integrate_ln(u: Node, var: str) -> Node:
    """`∫ln(u)dx` — only the `u == var` case is in the Level-1 table."""
    form = _arg_form(u, var)
    if form is not None and form[0] == "var":
        return Sub(Mul((u, Ln(u))), u)
    if form is not None and form[0] == "const":
        return Mul((Ln(u), Symbol(var)))
    raise NotImplementedError(
        "Integration of ln(u) only supports u = var or a var-independent constant"
    )


def integrate_pow(base: Node, exponent: Node, var: str) -> Node:
    """Power rule: `∫x^n dx = x^(n+1)/(n+1)`, plus the `n=-1` (ln) case."""
    if not isinstance(exponent, Number):
        raise NotImplementedError(
            "Integration of non-constant exponents is not supported yet"
        )
    n = exponent.value
    form = _arg_form(base, var)
    if form is None:
        raise NotImplementedError(
            "Integration of base^n needs base linear in the integration variable"
        )
    kind, a = form
    if kind == "var":
        if n == -1.0:
            return Ln(base)
        return Div(Pow(base, Number(n + 1.0)), Number(n + 1.0))
    if kind == "linear":
        if n == -1.0:
            return Div(Ln(base), Number(a))
        return Div(Pow(base, Number(n + 1.0)), Number(a * (n + 1.0)))
    return Mul((Pow(base, Number(n)), Symbol(var)))


def _antiderivative_of_outer(f: Node, u: Node) -> Node | None:
    """Returns the antiderivative of outer function `f` evaluated at `u`.

    `f` is `F(u)` for a Sin/Cos/Exp outer function (chain-rule inverse).
    """
    if isinstance(f, Sin):
        return Mul((Number(-1.0), Cos(u)))
    if isinstance(f, Cos):
        return Sin(u)
    if isinstance(f, Exp):
        return Exp(u)
    return None


def _inner_arg(f: Node) -> Node | None:
    if isinstance(f, (Sin, Cos, Exp)):
        return f.arg
    return None


def _try_u_substitution(
    p: Node, q: Node, var: str, coeff: float = 1.0
) -> tuple[Node, float] | None:
    """Basic u-substitution (§4.2): matches `p == g'(x)` against `q == F(g(x))`.

    Returns `(antiderivative, remaining_coeff)`: `remaining_coeff` is 1.0 when
    `coeff` was absorbed into the match (`coeff * p == g'(x)`, e.g. `2x`
    stripped to `coeff=2, p=x`), otherwise it's `coeff` itself, still to be
    multiplied in by the caller.
    """
    u = _inner_arg(q)
    if u is None:
        return None
    du = simplify(diff_node(u, var))
    if du == simplify(Mul((Number(coeff), p))):
        return _antiderivative_of_outer(q, u), 1.0
    if du == simplify(p):
        return _antiderivative_of_outer(q, u), coeff
    return None


def integrate_mul(factors: tuple[Node, ...], var: str) -> Node:
    """`∫(product)dx`: constant pullout, single-factor rule, u-substitution."""
    const_factors = [f for f in factors if not depends_on(f, var)]
    non_const = [f for f in factors if depends_on(f, var)]

    def const_coeff(fs: list[Node]) -> float | None:
        c = 1.0
        for f in fs:
            if not isinstance(f, Number):
                return None
            c *= f.value
        return c

    if len(non_const) == 0:
        return Mul((Mul(tuple(factors)), Symbol(var)))
    if len(non_const) == 1:
        inner = integrate_node(non_const[0], var)
        c = const_coeff(const_factors)
        if c is not None:
            return Mul((Number(c), inner))
        return Mul((*const_factors, inner))
    if len(non_const) == 2:
        c = const_coeff(const_factors)
        coeff = 1.0 if c is None else c
        for p, q in (
            (non_const[0], non_const[1]),
            (non_const[1], non_const[0]),
        ):
            result = _try_u_substitution(p, q, var, coeff)
            if result is not None:
                antideriv, remaining = result
                if remaining == 1.0:
                    return antideriv
                return Mul((Number(remaining), antideriv))
        raise NotImplementedError(
            "No u-substitution pattern matched this product"
        )
    raise NotImplementedError(
        "Integration of products with more than two non-constant factors is not supported yet"
    )


def integrate_div(a: Node, b: Node, var: str) -> Node:
    """`∫(a/b)dx`: constant-denominator pullout, plus `1/linear(x) → ln`."""
    if not depends_on(b, var):
        return Div(integrate_node(a, var), b)
    if isinstance(a, Number) and a.value == 1.0:
        form = _arg_form(b, var)
        if form is not None and form[0] == "var":
            return Ln(b)
        if form is not None and form[0] == "linear":
            return Div(Ln(b), Number(form[1]))
    raise NotImplementedError(
        "Integration of this quotient is not supported yet"
    )


def integrate_node(node: Node, var: str) -> Node:
    """Indefinite integration (§4.2, "Level 1").

    Pattern-table lookup, linear chain rule, and a narrow g'(x)*F(g(x))
    u-substitution. Raises `NotImplementedError` outside that pattern
    set — no general solver.
    """
    if isinstance(node, Number):
        return Mul((Number(node.value), Symbol(var)))
    if isinstance(node, Symbol):
        if node.name == var:
            return Div(Pow(node, Number(2.0)), Number(2.0))
        return Mul((node, Symbol(var)))
    if isinstance(node, Quantity):
        if node.name == var:
            return Div(Pow(node, Number(2.0)), Number(2.0))
        return Mul((node, Symbol(var)))
    if isinstance(node, Add):
        return Add(tuple(integrate_node(t, var) for t in node.terms))
    if isinstance(node, Sub):
        return Sub(
            integrate_node(node.left, var), integrate_node(node.right, var)
        )
    if isinstance(node, Mul):
        return integrate_mul(node.factors, var)
    if isinstance(node, Div):
        return integrate_div(node.left, node.right, var)
    if isinstance(node, Pow):
        return integrate_pow(node.base, node.exponent, var)
    if isinstance(node, Sin):
        return integrate_sin(node.arg, var)
    if isinstance(node, Cos):
        return integrate_cos(node.arg, var)
    if isinstance(node, Ln):
        return integrate_ln(node.arg, var)
    if isinstance(node, Exp):
        return integrate_exp(node.arg, var)
    raise TypeError(f"Unknown node type: {type(node)!r}")


# --- simplification (§3.1) ----------------------------------------------


def flatten_add(terms: list[Node]) -> list[Node]:
    """Recursively unwraps nested `Add` terms into one flat list."""
    out: list[Node] = []
    for t in terms:
        if isinstance(t, Add):
            out.extend(flatten_add(list(t.terms)))
        else:
            out.append(t)
    return out


def flatten_mul(factors: list[Node]) -> list[Node]:
    """Recursively unwraps nested `Mul` factors into one flat list."""
    out: list[Node] = []
    for f in factors:
        if isinstance(f, Mul):
            out.extend(flatten_mul(list(f.factors)))
        else:
            out.append(f)
    return out


# ponytail: repr-based sort, not alphabetical-by-symbol; deterministic and
# good enough for canonicalization/dedup, upgrade if a specific order is
# needed. Mirrors symbolic.rs's Debug-repr `sort_key`.
def sort_key(n: Node) -> str:
    """Deterministic canonicalization key for term/factor ordering."""
    return repr(n)


def simplify_add(terms: list[Node]) -> Node:
    """Flattens, constant-folds, and collects equal terms of a sum."""
    flat = flatten_add(terms)
    const_sum = 0.0
    rest: list[Node] = []
    for t in flat:
        if isinstance(t, Number):
            const_sum += t.value
        else:
            rest.append(t)
    collected: list[list] = []
    for t in rest:
        for entry in collected:
            if entry[0] == t:
                entry[1] += 1.0
                break
        else:
            collected.append([t, 1.0])
    out_terms: list[Node] = [
        t if count == 1.0 else Mul((Number(count), t))
        for t, count in collected
    ]
    if const_sum != 0.0 or not out_terms:
        out_terms.append(Number(const_sum))
    out_terms.sort(key=sort_key)
    if len(out_terms) == 1:
        return out_terms[0]
    return Add(tuple(out_terms))


def simplify_sub(a: Node, b: Node) -> Node:
    """Applies the inverse (`x-x=0`) and identity (`x-0=x`) laws."""
    if a == b:
        return Number(0.0)
    if isinstance(b, Number) and b.value == 0.0:
        return a
    if isinstance(a, Number) and isinstance(b, Number):
        return Number(a.value - b.value)
    return Sub(a, b)


def simplify_mul(factors: list[Node]) -> Node:
    """Flattens, constant-folds, and collects equal factors of a product."""
    flat = flatten_mul(factors)
    const_prod = 1.0
    rest: list[Node] = []
    for f in flat:
        if isinstance(f, Number):
            const_prod *= f.value
        else:
            rest.append(f)
    if const_prod == 0.0:
        return Number(0.0)
    collected: list[list] = []
    for f in rest:
        for entry in collected:
            if entry[0] == f:
                entry[1] += 1.0
                break
        else:
            collected.append([f, 1.0])
    out_factors: list[Node] = [
        f if count == 1.0 else Pow(f, Number(count)) for f, count in collected
    ]
    if const_prod != 1.0 or not out_factors:
        out_factors.append(Number(const_prod))
    out_factors.sort(key=sort_key)
    if len(out_factors) == 1:
        return out_factors[0]
    return Mul(tuple(out_factors))


def simplify_div(a: Node, b: Node) -> Node:
    """Applies the inverse (`x/x=1`) and identity (`x/1=x`) laws."""
    if a == b:
        return Number(1.0)
    if isinstance(b, Number) and b.value == 1.0:
        return a
    if isinstance(a, Number) and isinstance(b, Number) and b.value != 0.0:
        return Number(a.value / b.value)
    return Div(a, b)


def simplify_pow(base: Node, exponent: Node) -> Node:
    """Applies constant-folding and the `x^0=1`/`x^1=x`/`1^x=1` laws."""
    if isinstance(base, Number) and isinstance(exponent, Number):
        return Number(base.value**exponent.value)
    if isinstance(exponent, Number) and exponent.value == 1.0:
        return base
    if isinstance(exponent, Number) and exponent.value == 0.0:
        return Number(1.0)
    if isinstance(base, Number) and base.value == 1.0:
        return Number(1.0)
    return Pow(base, exponent)


def simplify(node: Node) -> Node:
    """Recursively applies the §3.1 algebraic simplification laws."""
    if isinstance(node, (Number, Symbol, Quantity)):
        return node
    if isinstance(node, Add):
        return simplify_add([simplify(t) for t in node.terms])
    if isinstance(node, Sub):
        return simplify_sub(simplify(node.left), simplify(node.right))
    if isinstance(node, Mul):
        return simplify_mul([simplify(f) for f in node.factors])
    if isinstance(node, Div):
        return simplify_div(simplify(node.left), simplify(node.right))
    if isinstance(node, Pow):
        return simplify_pow(simplify(node.base), simplify(node.exponent))
    if isinstance(node, Sin):
        return Sin(simplify(node.arg))
    if isinstance(node, Cos):
        return Cos(simplify(node.arg))
    if isinstance(node, Ln):
        return Ln(simplify(node.arg))
    if isinstance(node, Exp):
        return Exp(simplify(node.arg))
    raise TypeError(f"Unknown node type: {type(node)!r}")


# --- Expr wrapper --------------------------------------------------------


class PyExpr:
    """Pure-Python mirror of `physure._core.Expr`'s public surface."""

    __slots__ = ("node",)

    def __init__(self, node: Node) -> None:
        self.node = node

    @staticmethod
    def number(v: float) -> PyExpr:
        """Builds a numeric-literal expression."""
        return PyExpr(Number(float(v)))

    @staticmethod
    def symbol(s: str) -> PyExpr:
        """Builds a free-variable expression."""
        return PyExpr(Symbol(s))

    @staticmethod
    def quantity(name: str, unit: CompoundUnit) -> PyExpr:
        """Builds a named, unit-carrying variable expression."""
        return PyExpr(Quantity(name, unit))

    @staticmethod
    def sin(e: PyExpr) -> PyExpr:
        """Builds `sin(e)`."""
        return PyExpr(Sin(e.node))

    @staticmethod
    def cos(e: PyExpr) -> PyExpr:
        """Builds `cos(e)`."""
        return PyExpr(Cos(e.node))

    @staticmethod
    def ln(e: PyExpr) -> PyExpr:
        """Builds `ln(e)`."""
        return PyExpr(Ln(e.node))

    @staticmethod
    def exp(e: PyExpr) -> PyExpr:
        """Builds `exp(e)`."""
        return PyExpr(Exp(e.node))

    def __add__(self, other: PyExpr) -> PyExpr:
        """Adds two expressions, raising on incompatible units."""
        check_add_compat(self.node, other.node)
        return PyExpr(Add(tuple(flatten_add([self.node, other.node]))))

    def __sub__(self, other: PyExpr) -> PyExpr:
        """Subtracts two expressions, raising on incompatible units."""
        check_add_compat(self.node, other.node)
        return PyExpr(Sub(self.node, other.node))

    def __mul__(self, other: PyExpr) -> PyExpr:
        """Multiplies two expressions."""
        return PyExpr(Mul(tuple(flatten_mul([self.node, other.node]))))

    def __truediv__(self, other: PyExpr) -> PyExpr:
        """Divides two expressions."""
        return PyExpr(Div(self.node, other.node))

    def __pow__(self, other: PyExpr) -> PyExpr:
        """Raises this expression to the power of `other`."""
        return PyExpr(Pow(self.node, other.node))

    def simplify(self) -> PyExpr:
        """Returns an algebraically simplified copy of this expression."""
        return PyExpr(simplify(self.node))

    def diff(self, var: str, n: int = 1) -> PyExpr:
        """Returns the `n`-th derivative with respect to `var`, simplified."""
        cur = self.node
        for _ in range(n):
            cur = diff_node(cur, var)
        return PyExpr(simplify(cur))

    def integrate(self, var: str) -> PyExpr:
        """Returns an indefinite integral w.r.t. `var` (§4.2, "Level 1")."""
        return PyExpr(simplify(integrate_node(self.node, var)))

    def unit(self) -> CompoundUnit | None:
        """Returns this expression's inferred unit, or `None` if unit-less."""
        return infer_unit(self.node)

    def __repr__(self) -> str:
        """Returns the debug repr of the underlying node."""
        return repr(self.node)

    def __eq__(self, other: object) -> bool:
        """Compares structural equality of the underlying node trees."""
        if not isinstance(other, PyExpr):
            return NotImplemented
        return self.node == other.node

    def __hash__(self) -> int:
        """Hashes by the node's debug repr."""
        return hash(repr(self.node))


from physure._core import Expr
