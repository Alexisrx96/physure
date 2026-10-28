import sys
from contextvars import ContextVar, Token
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from physure.domain.symbolic.tracer import FormulaTracer

# The tracer is imported only under TYPE_CHECKING: its module imports this one.
_current_tracer: ContextVar["FormulaTracer | None"] = ContextVar(
    "formula_tracer", default=None
)


def get_active_tracer() -> "FormulaTracer | None":
    """Returns the currently active FormulaTracer if any."""
    # During torch.compile tracing, ContextVar often triggers "Unsupported method call".
    # We disable Physure symbolic tracing inside Torch compilation to avoid
    # mixing tracing systems and side effects that confuse Dynamo.
    # sys.modules guard: a bare `import torch` would load all of torch (~2s)
    # for users who never touch it.
    if "torch" in sys.modules:
        try:
            import torch

            if torch.compiler.is_compiling():
                return None
        except (ImportError, AttributeError):
            pass

    return _current_tracer.get()


def set_active_tracer(tracer: "FormulaTracer | None") -> Token:
    """Sets the active FormulaTracer and returns a token for resetting."""
    return _current_tracer.set(tracer)


def reset_active_tracer(token: Token) -> None:
    """Restores the tracer that was active before `set_active_tracer`."""
    _current_tracer.reset(token)
