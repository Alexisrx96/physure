from typing import Any

from measurekit.domain.symbolic.graph import (
    LeafNode,
    LiteralNode,
    OpNode,
    SymbolicNode,
)


class SympyTranslator:
    """Translates a SymbolicNode graph into a SymPy expression.
    SymPy is imported lazily inside methods.
    """

    @classmethod
    def translate(cls, node: SymbolicNode) -> Any:
        import sympy

        if isinstance(node, LeafNode):
            return sympy.Symbol(node.symbol)

        if isinstance(node, LiteralNode):
            return (
                sympy.core.numbers.Number(node.value)
                if isinstance(node.value, (int, float))
                else node.value
            )

        if isinstance(node, OpNode):
            args = [cls.translate(arg) for arg in node.args]

            if node.op_name == "add":
                return sympy.Add(*args)
            if node.op_name == "sub":
                # Sympy subtraction is Add(a, -b)
                if len(args) == 2:
                    return args[0] - args[1]
                return sympy.Add(*args)  # Should not happen for sub
            if node.op_name == "mul":
                return sympy.Mul(*args)
            if node.op_name == "truediv":
                if len(args) == 2:
                    return args[0] / args[1]
            elif node.op_name == "pow":
                if len(args) == 2:
                    return args[0] ** args[1]
            elif node.op_name == "sin":
                return sympy.sin(args[0])
            elif node.op_name == "cos":
                return sympy.cos(args[0])
            # Add more as needed

            # Fallback for generic ops
            func = getattr(sympy, node.op_name, None)
            if func:
                return func(*args)

        raise ValueError(f"Unsupported node type or operation: {node}")
