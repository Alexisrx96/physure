from collections.abc import Callable

from mypy.nodes import StrExpr
from mypy.plugin import FunctionContext, Plugin
from mypy.types import Instance, LiteralType
from mypy.types import Type as MypyType


class MeasureKitPlugin(Plugin):
    """Narrows Q_(value, "unit") return types to Literal unit strings."""

    def get_function_hook(
        self, fullname: str
    ) -> Callable[[FunctionContext], MypyType] | None:
        """Returns the Q_ hook for measurekit factory callables."""
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

        # Quantity(Generic[Value, Unc, Unit])
        if (
            unit_str
            and isinstance(ret_type, Instance)
            and len(ret_type.args) >= 3
        ):
            str_type = ctx.api.named_generic_type("builtins.str", [])
            literal_unit = LiteralType(value=unit_str, fallback=str_type)
            new_args = list(ret_type.args)
            new_args[2] = literal_unit
            return ret_type.copy_modified(args=new_args)

        return ret_type


def plugin(_version: str) -> type[MeasureKitPlugin]:
    """Mypy plugin entry point."""
    return MeasureKitPlugin
