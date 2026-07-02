"""Unit-aware terminal calculator speaking MNML.

Three entry modes, in order of precedence:

- ``python -m measurekit "500 N / 2 m^2 => kPa"`` — evaluate and exit.
- ``python -m measurekit < notes.mnml`` — evaluate piped statements.
- ``python -m measurekit`` — interactive REPL.

Also available as the ``measurekit repl`` CLI subcommand. Syntax is the
MeasureNote meta-language implemented in :mod:`measurekit.ext.grammar`.
"""

from __future__ import annotations

import sys
from typing import Any

_BANNER = (
    "measurekit — unit-aware calculator. "
    "Try `500 N / 2 m^2 => kPa`; exit with Ctrl-D."
)


def _print_results(results: list[Any]) -> None:
    for result in results:
        if result is not None:
            print(result)


def _run_source(source: str) -> int:
    from measurekit.ext.grammar import GrammarInterpreter

    try:
        _print_results(GrammarInterpreter().run(source))
    except Exception as e:  # CLI boundary: report, don't traceback
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


def _repl() -> int:
    import contextlib

    with contextlib.suppress(ImportError):
        import readline  # noqa: F401  # line editing + history

    from measurekit.ext.grammar import GrammarInterpreter

    interp = GrammarInterpreter()
    print(_BANNER)
    while True:
        try:
            line = input("mk> ")
        except EOFError:
            print()
            return 0
        except KeyboardInterrupt:
            print()
            continue
        if line.strip() in ("exit", "quit"):
            return 0
        try:
            _print_results(interp.run(line))
        except Exception as e:  # keep the session alive on any error
            print(f"error: {e}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    """Entry point: expression args > piped stdin > interactive REPL."""
    args = sys.argv[1:] if argv is None else argv
    if args:
        return _run_source(" ".join(args))
    if not sys.stdin.isatty():
        return _run_source(sys.stdin.read())
    return _repl()


if __name__ == "__main__":
    sys.exit(main())
