from typing import Callable, Optional, Type, Any
from mypy.plugin import Plugin, FunctionContext
from mypy.types import Type as MypyType, Instance, LiteralType
from mypy.nodes import CallExpr, StrExpr

class MeasureKitPlugin(Plugin):
    def get_function_hook(self, fullname: str) -> Optional[Callable[[FunctionContext], MypyType]]:
        if fullname in (
            "measurekit.domain.measurement.quantity.Q_", 
            "measurekit.Q_", 
            "measurekit.domain.measurement.quantity.Quantity.from_input",
            # Hooks for testing environments
            "mk_fake.Q_",
        ):
             return self.q_factory_hook
        return None

    def q_factory_hook(self, ctx: FunctionContext) -> MypyType:
        """Infer a return type that includes the unit literal if possible."""
        ret_type = ctx.default_return_type
        
        # Check if we can find a unit string in the arguments: Q_(value, unit_string, ...)
        unit_str = None
        if len(ctx.args) >= 2 and len(ctx.args[1]) > 0:
            unit_arg = ctx.args[1][0]
            if isinstance(unit_arg, StrExpr):
                unit_str = unit_arg.value
        
        if unit_str and isinstance(ret_type, Instance):
            # Quantity(Generic[Value, Unc, Unit])
            if len(ret_type.args) >= 3:
                str_type = ctx.api.named_generic_type("builtins.str", [])
                literal_unit = LiteralType(
                    value=unit_str,
                    fallback=str_type
                )
                new_args = list(ret_type.args)
                new_args[2] = literal_unit
                return ret_type.copy_modified(args=new_args)

        return ret_type

def plugin(_version: str) -> Type[MeasureKitPlugin]:
    return MeasureKitPlugin
