from collections.abc import Generator
from contextlib import contextmanager
from contextvars import ContextVar, Token
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from measurekit.domain.symbolic.tracer import FormulaTracer

# The tracer is imported only inside functions or using TYPE_CHECKING
# to avoid circular imports.
_current_tracer: ContextVar[Optional["FormulaTracer"]] = ContextVar(
    "formula_tracer", default=None
)


def get_active_tracer() -> Optional["FormulaTracer"]:
    """Returns the currently active FormulaTracer if any."""
    # During torch.compile tracing, ContextVar often triggers "Unsupported method call".
    # We disable MeasureKit symbolic tracing inside Torch compilation to avoid
    # mixing tracing systems and side effects that confuse Dynamo.
    try:
        import torch
        if torch.compiler.is_compiling():
            return None
    except (ImportError, AttributeError):
        pass

    return _current_tracer.get()


def set_active_tracer(tracer: Optional["FormulaTracer"]) -> Token:
    """Sets the active FormulaTracer and returns a token for resetting."""
    return _current_tracer.set(tracer)


@contextmanager
def trace_formulas() -> Generator["FormulaTracer", None, None]:
    """Context manager to enable symbolic tracing.

    Example:
        with trace_formulas() as tracer:
            m = Q_(10, "kg", symbol="m")
            a = Q_(5, "m/s^2", symbol="a")
            f = m * a
            print(tracer.get_equation(f))
    """
    from measurekit.domain.symbolic.tracer import FormulaTracer

    tracer = FormulaTracer()
    token = set_active_tracer(tracer)
    try:
        yield tracer
    finally:
        _current_tracer.reset(token)
