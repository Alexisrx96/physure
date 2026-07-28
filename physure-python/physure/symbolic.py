"""Symbolic mathematics for Physure."""

from physure._core import Interpreter, Quantity as CoreQuantity
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

    def get_params(self) -> list[str]:
        """Return the ordered parameter names of this function."""
        if hasattr(self, "_custom_params") and self._custom_params:
            return self._custom_params
        return self.interpreter.get_fn_params(self.name) or []

    def __call__(self, *args, **kwargs) -> "Quantity":
        """Evaluate the function with positional or keyword arguments via the Rust engine."""
        if hasattr(self, "_py_callable") and self._py_callable is not None:
            return self._py_callable(*args, **kwargs)

        if len(args) == 1 and isinstance(args[0], PhyFunction):
            return self.compose(args[0])

        params = self.get_params()
        formatted_args = []

        if kwargs:
            for p in params:
                if p in kwargs:
                    arg = kwargs[p]
                    if isinstance(arg, Quantity):
                        formatted_args.append(f"{arg.magnitude} {clean_unit_str(arg.unit)}")
                    else:
                        formatted_args.append(str(arg))
                else:
                    raise ValueError(f"Missing parameter '{p}' for function '{self.name}'")
        else:
            for arg in args:
                if isinstance(arg, Quantity):
                    formatted_args.append(f"{arg.magnitude} {clean_unit_str(arg.unit)}")
                else:
                    formatted_args.append(str(arg))

        call_str = f"{self.name}({', '.join(formatted_args)})"
        results = self.interpreter.evaluate(call_str)
        if not results:
            return None
        res = results[-1]
        if isinstance(res, CoreQuantity):
            return Quantity(res.magnitude, res.unit)
        return res

    def deriv(self, var: str) -> "PhyFunction":
        """Return a new PhyFunction that is the derivative with respect to `var`."""
        params = self.get_params()
        if not params:
            raise ValueError(
                "Cannot differentiate a function with no parameters."
            )

        params_joined = ", ".join(params)
        call_expr = f"{self.name}({params_joined})"
        deriv_result = self.interpreter.deriv(call_expr, var)

        new_name = f"d_{self.name}_d_{var}"
        new_body = f"{new_name}({params_joined}) = {deriv_result}"

        return PhyFunction(self.interpreter, new_name, new_body)

    def integral(self, var: str) -> "PhyFunction":
        """Return a new PhyFunction that is the antiderivative with respect to `var`."""
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
        """Return a new PhyFunction solving the original equation for `var`."""
        params = self.get_params()
        if not params:
            raise ValueError("Cannot solve a function with no parameters.")

        params_joined = ", ".join(params)
        call_expr = f"{self.name}({params_joined})"
        target_name = "target"

        solve_result = self.interpreter.solve(
            f"{call_expr} = {target_name}", var
        )

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
            raise ValueError(
                "Functions must share the same Interpreter context"
            )

        params1 = self.get_params()
        params2 = other.get_params()

        combined = list(params1)
        for p in params2:
            if p not in combined:
                combined.append(p)

        combined_params_joined = ", ".join(combined)
        new_name = f"{op_name}_{self.name}_{other.name}"
        call_self = f"{self.name}({', '.join(params1)})"
        call_other = f"{other.name}({', '.join(params2)})"
        body = f"{new_name}({combined_params_joined}) = {call_self} {op_symbol} {call_other}"

        return PhyFunction(self.interpreter, new_name, body)

    def compose(self, other: "PhyFunction") -> "PhyFunction":
        """Return a new PhyFunction representing self(other(...))."""
        if self.interpreter is not other.interpreter:
            raise ValueError(
                "Functions must share the same Interpreter context"
            )

        params_f = self.get_params()
        params_g = other.get_params()

        if not params_f:
            raise ValueError(
                "Outer function must have at least one parameter."
            )

        combined = list(params_g)
        for p in params_f[1:]:
            if p not in combined:
                combined.append(p)

        combined_params_joined = ", ".join(combined)

        call_g = f"{other.name}({', '.join(params_g)})"
        call_f_args = [call_g, *list(params_f[1:])]
        call_f = f"{self.name}({', '.join(call_f_args)})"

        new_name = f"compose_{self.name}_{other.name}"
        body = f"{new_name}({combined_params_joined}) = {call_f}"

        return PhyFunction(self.interpreter, new_name, body)


def clean_unit_str(unit_str: str) -> str:
    s = str(unit_str)
    s = s.replace("·", " * ")
    s = s.replace("⁻", "^-")
    s = s.replace("⁰", "^0")
    s = s.replace("¹", "^1")
    s = s.replace("²", "^2")
    s = s.replace("³", "^3")
    s = s.replace("⁴", "^4")
    s = s.replace("⁵", "^5")
    s = s.replace("⁶", "^6")
    s = s.replace("⁷", "^7")
    s = s.replace("⁸", "^8")
    s = s.replace("⁹", "^9")
    s = s.replace("^-^-", "^-")
    s = s.replace("^^", "^")
    return s


class PhyEquation:
    """Represents a physical equation (e.g. "V = R * I").
    
    Callable by default: invoking the equation with known keyword arguments 
    dynamically solves and evaluates the remaining unknown variable.
    """

    def __init__(self, expression: str, interpreter: Interpreter = None):
        if interpreter is None:
            interpreter = Interpreter()
        self.interpreter = interpreter
        self.expression = expression.strip().strip('"')

    def solve(self, var: str) -> "PhyEquation":
        """Solve this equation for variable `var`, returning a new PhyEquation."""
        solved_expr = self.interpreter.solve(self.expression, var)
        if "=" not in solved_expr:
            solved_expr = f"{var} = {solved_expr}"
        return PhyEquation(solved_expr, interpreter=self.interpreter)

    def __call__(self, **kwargs) -> Quantity:
        """Call this equation with keyword arguments to solve the unknown on the fly."""
        import re
        tokens = re.findall(r'\b[A-Za-z_][A-Za-z0-9_]*\b', self.expression)
        known_keys = set(kwargs.keys())
        builtins_set = {"sin", "cos", "tan", "sqrt", "log", "exp", "pi", "abs", "ln"}
        missing = [t for t in tokens if t not in known_keys and t not in builtins_set]
        
        args_str = ", ".join(f"{k} = ({v.magnitude} {clean_unit_str(v.unit)})" if isinstance(v, Quantity) else f"{k} = {v}" for k, v in kwargs.items())
        
        if len(missing) == 1:
            target_var = missing[0]
            script = f"""use solve from calc
eq_temp = "{self.expression}"
solve_fn = solve(eq_temp, "{target_var}")
solve_fn({args_str})
"""
        else:
            script = f"""use solve from calc
eq_temp = "{self.expression}"
solve_fn = solve(eq_temp)
solve_fn({args_str})
"""
        from physure._core import evaluate_phs_native
        results = evaluate_phs_native(script)
        if not results:
            return None
        res = results[-1]
        if isinstance(res, CoreQuantity):
            return Quantity(res.magnitude, res.unit)
        return res

    @property
    def lhs(self) -> str:
        """Left-hand side of the equation."""
        if "=" in self.expression:
            return self.expression.split("=", 1)[0].strip()
        return self.expression.strip()

    @property
    def rhs(self) -> str:
        """Right-hand side of the equation."""
        if "=" in self.expression:
            return self.expression.split("=", 1)[1].strip()
        return "0"

    def _format_operand(self, other: Any) -> str:
        if isinstance(other, PhyEquation):
            return f"({other.expression})"
        if isinstance(other, Quantity):
            return f"({other.magnitude} {clean_unit_str(other.unit)})"
        return str(other)

    def __add__(self, other: Any) -> "PhyEquation":
        if isinstance(other, PhyEquation):
            return PhyEquation(f"({self.lhs}) + ({other.lhs}) = ({self.rhs}) + ({other.rhs})", interpreter=self.interpreter)
        op_str = self._format_operand(other)
        return PhyEquation(f"({self.lhs}) + {op_str} = ({self.rhs}) + {op_str}", interpreter=self.interpreter)

    def __radd__(self, other: Any) -> "PhyEquation":
        op_str = self._format_operand(other)
        return PhyEquation(f"{op_str} + ({self.lhs}) = {op_str} + ({self.rhs})", interpreter=self.interpreter)

    def __sub__(self, other: Any) -> "PhyEquation":
        if isinstance(other, PhyEquation):
            return PhyEquation(f"({self.lhs}) - ({other.lhs}) = ({self.rhs}) - ({other.rhs})", interpreter=self.interpreter)
        op_str = self._format_operand(other)
        return PhyEquation(f"({self.lhs}) - {op_str} = ({self.rhs}) - {op_str}", interpreter=self.interpreter)

    def __rsub__(self, other: Any) -> "PhyEquation":
        op_str = self._format_operand(other)
        return PhyEquation(f"{op_str} - ({self.lhs}) = {op_str} - ({self.rhs})", interpreter=self.interpreter)

    def __mul__(self, other: Any) -> "PhyEquation":
        if isinstance(other, PhyEquation):
            return PhyEquation(f"({self.lhs}) * ({other.lhs}) = ({self.rhs}) * ({other.rhs})", interpreter=self.interpreter)
        op_str = self._format_operand(other)
        return PhyEquation(f"({self.lhs}) * {op_str} = ({self.rhs}) * {op_str}", interpreter=self.interpreter)

    def __rmul__(self, other: Any) -> "PhyEquation":
        op_str = self._format_operand(other)
        return PhyEquation(f"{op_str} * ({self.lhs}) = {op_str} * ({self.rhs})", interpreter=self.interpreter)

    def __truediv__(self, other: Any) -> "PhyEquation":
        if isinstance(other, PhyEquation):
            return PhyEquation(f"({self.lhs}) / ({other.lhs}) = ({self.rhs}) / ({other.rhs})", interpreter=self.interpreter)
        op_str = self._format_operand(other)
        return PhyEquation(f"({self.lhs}) / {op_str} = ({self.rhs}) / {op_str}", interpreter=self.interpreter)

    def __rtruediv__(self, other: Any) -> "PhyEquation":
        op_str = self._format_operand(other)
        return PhyEquation(f"{op_str} / ({self.lhs}) = {op_str} / ({self.rhs})", interpreter=self.interpreter)

    def __pow__(self, power: Any) -> "PhyEquation":
        op_str = self._format_operand(power)
        return PhyEquation(f"({self.lhs}) ^ {op_str} = ({self.rhs}) ^ {op_str}", interpreter=self.interpreter)

    def substitute(self, target: str, expr_or_eq: Any) -> "PhyEquation":
        """Substitute symbol `target` in this equation with another expression or equation RHS."""
        if isinstance(expr_or_eq, PhyEquation):
            sub_val = f"({expr_or_eq.rhs})"
        else:
            sub_val = f"({expr_or_eq})"
        import re
        new_lhs = re.sub(rf'\b{re.escape(target)}\b', sub_val, self.lhs)
        new_rhs = re.sub(rf'\b{re.escape(target)}\b', sub_val, self.rhs)
        return PhyEquation(f"{new_lhs} = {new_rhs}", interpreter=self.interpreter)

    def __repr__(self):
        return f"PhyEquation(\"{self.expression}\")"


def phy_function(func=None, *, name=None, params=None, interpreter=None):
    """Decorator to convert a Python function into a physical PhyFunction."""
    def decorator(fn):
        fn_name = name or fn.__name__
        import inspect
        fn_params = params or list(inspect.signature(fn).parameters.keys())
        pf = PhyFunction._from_existing(interpreter or Interpreter(), fn_name)
        pf._py_callable = fn
        pf._custom_params = fn_params
        return pf

    if func is not None:
        return decorator(func)
    return decorator


__all__ = ["Equation", "PhyEquation", "PhyFunction", "phy_function", "SymbolicExpression", "SymbolicQuantity"]
