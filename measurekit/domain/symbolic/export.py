from typing import Any

from measurekit.domain.symbolic.graph import (
    LeafNode,
    LiteralNode,
    OpNode,
    SymbolicNode,
)


def _translate_leaf(node: LeafNode, sympy: Any) -> Any:
    """Translates a LeafNode to a SymPy Symbol."""
    return sympy.Symbol(node.symbol)


def _translate_literal(node: LiteralNode, sympy: Any) -> Any:
    """Translates a LiteralNode to a SymPy number or raw value."""
    if isinstance(node.value, (int, float)):
        return sympy.core.numbers.Number(node.value)
    return node.value


def _translate_op(node: OpNode, args: list, sympy: Any) -> Any:
    """Translates an OpNode to its SymPy equivalent."""
    op = node.op_name
    if op == "add":
        return sympy.Add(*args)
    if op == "sub":
        return args[0] - args[1] if len(args) == 2 else sympy.Add(*args)
    if op == "mul":
        return sympy.Mul(*args)
    if op == "truediv" and len(args) == 2:
        return args[0] / args[1]
    if op == "pow" and len(args) == 2:
        return args[0] ** args[1]
    if op == "sin":
        return sympy.sin(args[0])
    if op == "cos":
        return sympy.cos(args[0])
    func = getattr(sympy, op, None)
    if func:
        return func(*args)
    return None


class SympyTranslator:
    """Translates a SymbolicNode graph into a SymPy expression.

    SymPy is imported lazily inside methods.
    """

    @classmethod
    def translate(cls, node: SymbolicNode) -> Any:
        """Recursively converts a node tree to a SymPy expression."""
        import sympy

        if isinstance(node, LeafNode):
            return _translate_leaf(node, sympy)

        if isinstance(node, LiteralNode):
            return _translate_literal(node, sympy)

        if isinstance(node, OpNode):
            args = [cls.translate(arg) for arg in node.args]
            result = _translate_op(node, args, sympy)
            if result is not None:
                return result

        raise ValueError(f"Unsupported node type or operation: {node}")
