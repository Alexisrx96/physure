"""Unit-aware terminal calculator speaking MNML.

Three entry modes, in order of precedence:

- ``python -m physure "500 N / 2 m^2 => kPa"`` — evaluate and exit.
- ``python -m physure < notes.mnml`` — evaluate piped statements.
- ``python -m physure`` — interactive REPL.

Also available as the ``physure repl`` CLI subcommand. Syntax is the
MeasureNote meta-language implemented in :mod:`physure.ext.grammar`.
"""

from __future__ import annotations

import sys
from typing import Any

_BANNER = (
    "physure — unit-aware calculator. "
    "Try `500 N / 2 m^2 => kPa`; exit with Ctrl-D."
)


def _print_results(results: list[Any]) -> None:
    for result in results:
        if result is not None:
            print(result)


def _run_source(source: str) -> int:
    from physure.ext.grammar import GrammarInterpreter

    try:
        _print_results(GrammarInterpreter().run(source))
    except Exception as e:  # CLI boundary: report, don't traceback
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


def _repl() -> None:
    import contextlib
    import threading

    with contextlib.suppress(ImportError):
        # ponytail: side-effecting import, enables line editing + history
        # on stdin; the name itself is never referenced.
        import readline  # noqa: F401  # pyright: ignore[reportUnusedImport]

    from physure.ext.grammar import GrammarInterpreter

    # Build the unit system while the user types their first line; join
    # before evaluating so the main thread never races the build.
    def _warm() -> None:
        with contextlib.suppress(Exception):
            from physure.application.context import get_current_system

            get_current_system()

    warm = threading.Thread(target=_warm, daemon=True)
    warm.start()

    interp = GrammarInterpreter()
    print(_BANNER)
    while True:
        try:
            line = input("mk> ")
        except EOFError:
            print()
            return
        except KeyboardInterrupt:
            print()
            continue
        if line.strip() in ("exit", "quit"):
            return
        warm.join()
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
    _repl()
    return 0


if __name__ == "__main__":
    sys.exit(main())
