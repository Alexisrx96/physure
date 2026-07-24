"""Symbolic mathematics for Physure."""

from typing import List
from physure._core import Interpreter
from physure.domain.measurement.quantity import Quantity

from physure.domain.symbolic import (
    Equation,
    SymbolicExpression,
    SymbolicQuantity,
)

class PhyFunction:
    """Represents a physical or mathematical function registered in the Physure context.
    Delegates function definition and execution statefully to the Rust engine.
    """

    def __init__(self, interpreter: Interpreter, name: str, body: str):
        self.interpreter = interpreter
        self.name = name
        self.body = body
        self.interpreter.evaluate(body)

    @classmethod
    def _from_existing(cls, interpreter: Interpreter, name: str):
        self = cls.__new__(cls)
        self.interpreter = interpreter
        self.name = name
        self.body = ""
        return self

    def get_params(self) -> List[str]:
        return self.interpreter.get_fn_params(self.name) or []

    def __call__(self, *args) -> "Quantity":
        if len(args) == 1 and isinstance(args[0], PhyFunction):
            return self.compose(args[0])
            
        formatted_args = []
        for arg in args:
            if isinstance(arg, Quantity):
                unit_str = str(arg.unit)
                # Clean unicode symbols for the PHS engine lexer
                unit_str = unit_str.replace("·", " * ")
                unit_str = unit_str.replace("⁻", "^-")
                unit_str = unit_str.replace("⁰", "^0")
                unit_str = unit_str.replace("¹", "^1")
                unit_str = unit_str.replace("²", "^2")
                unit_str = unit_str.replace("³", "^3")
                unit_str = unit_str.replace("⁴", "^4")
                unit_str = unit_str.replace("⁵", "^5")
                unit_str = unit_str.replace("⁶", "^6")
                unit_str = unit_str.replace("⁷", "^7")
                unit_str = unit_str.replace("⁸", "^8")
                unit_str = unit_str.replace("⁹", "^9")
                unit_str = unit_str.replace("^-^-", "^-")
                unit_str = unit_str.replace("^^", "^")
                formatted_args.append(f"{arg.magnitude} {unit_str}")
            else:
                formatted_args.append(str(arg))
        
        call_str = f"{self.name}({', '.join(formatted_args)})"
        results = self.interpreter.evaluate(call_str)
        if not results:
            return None
        return results[-1]

    def deriv(self, var: str) -> "PhyFunction":
        params = self.get_params()
        if not params:
            raise ValueError("Cannot differentiate a function with no parameters.")
        
        params_joined = ", ".join(params)
        call_expr = f"{self.name}({params_joined})"
        deriv_result = self.interpreter.deriv(call_expr, var)
        
        new_name = f"d_{self.name}_d_{var}"
        new_body = f"{new_name}({params_joined}) = {deriv_result}"
        
        return PhyFunction(self.interpreter, new_name, new_body)

    def integral(self, var: str) -> "PhyFunction":
        params = self.get_params()
        if not params:
            raise ValueError("Cannot integrate a function with no parameters.")
        
        params_joined = ", ".join(params)
        call_expr = f"{self.name}({params_joined})"
        integral_result = self.interpreter.integral(call_expr, var)
        
        new_name = f"int_{self.name}_d_{var}"
        new_body = f"{new_name}({params_joined}) = {integral_result}"
        
        return PhyFunction(self.interpreter, new_name, new_body)

    def solve(self, var: str) -> "PhyFunction":
        params = self.get_params()
        if not params:
            raise ValueError("Cannot solve a function with no parameters.")
        
        params_joined = ", ".join(params)
        call_expr = f"{self.name}({params_joined})"
        target_name = "target"
        
        solve_result = self.interpreter.solve(f"{call_expr} = {target_name}", var)
        
        new_params = [target_name] + [p for p in params if p != var]
        new_params_joined = ", ".join(new_params)
        
        new_name = f"solve_{self.name}_for_{var}"
        new_body = f"{new_name}({new_params_joined}) = {solve_result}"
        
        return PhyFunction(self.interpreter, new_name, new_body)

    def __add__(self, other):
        if not isinstance(other, PhyFunction):
            raise TypeError("Can only add another PhyFunction")
        return self._binary_op(other, "+", "add")

    def __sub__(self, other):
        if not isinstance(other, PhyFunction):
            raise TypeError("Can only subtract another PhyFunction")
        return self._binary_op(other, "-", "sub")

    def __mul__(self, other):
        if not isinstance(other, PhyFunction):
            raise TypeError("Can only multiply another PhyFunction")
        return self._binary_op(other, "*", "mul")

    def __truediv__(self, other):
        if not isinstance(other, PhyFunction):
            raise TypeError("Can only divide another PhyFunction")
        return self._binary_op(other, "/", "div")

    def _binary_op(self, other, op_symbol, op_name):
        if self.interpreter is not other.interpreter:
            raise ValueError("Functions must share the same Interpreter context")
        
        params1 = self.get_params()
        params2 = other.get_params()
        
        combined = list(params1)
        for p in params2:
            if p not in combined:
                combined.append(p)
                
        combined_params_joined = ", ".join(combined)
        new_name = f"{op_name}_{self.name}_{other.name}"
        body = f"{new_name}({combined_params_joined}) = {self.name}({', '.join(params1)}) {op_symbol} {other.name}({', '.join(params2)})"
        
        return PhyFunction(self.interpreter, new_name, body)

    def compose(self, other: "PhyFunction") -> "PhyFunction":
        if self.interpreter is not other.interpreter:
            raise ValueError("Functions must share the same Interpreter context")
            
        params_f = self.get_params()
        params_g = other.get_params()
        
        if not params_f:
            raise ValueError("Outer function must have at least one parameter.")
            
        combined = list(params_g)
        for p in params_f[1:]:
            if p not in combined:
                combined.append(p)
                
        combined_params_joined = ", ".join(combined)
        
        call_g = f"{other.name}({', '.join(params_g)})"
        call_f_args = [call_g] + list(params_f[1:])
        call_f = f"{self.name}({', '.join(call_f_args)})"
        
        new_name = f"compose_{self.name}_{other.name}"
        body = f"{new_name}({combined_params_joined}) = {call_f}"
        
        return PhyFunction(self.interpreter, new_name, body)


__all__ = ["Equation", "SymbolicExpression", "SymbolicQuantity", "PhyFunction"]
