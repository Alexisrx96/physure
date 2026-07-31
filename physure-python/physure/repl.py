"""Unit-aware terminal calculator speaking PHS.

Three entry modes, in order of precedence:

- ``python -m physure "500 N / 2 m^2 => kPa"`` — evaluate and exit.
- ``python -m physure < notes.phs`` — evaluate piped statements.
- ``python -m physure`` — interactive REPL.

Also available as the ``physure repl`` CLI subcommand. Syntax is the
Physure Script (PHS) language, evaluated by the Rust engine in
``physure._core``; there is no Python implementation of the language.
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
            s = str(result)
            lines = [
                line
                for line in s.split("\n")
                if not line.startswith("[PLOT_IMAGE:")
            ]
            clean_str = "\n".join(lines).rstrip()
            if clean_str:
                print(clean_str)


def _interpreter() -> Any:
    """The PHS engine. Rust-only by design, so a missing extension is fatal here."""
    try:
        from physure._core import Interpreter
    except (
        ImportError
    ) as e:  # pragma: no cover - depends on how physure was installed
        raise SystemExit(
            "error: PHS needs the native engine. Install the compiled package "
            "(`pip install physure`) or build it with `maturin develop`."
        ) from e
    return Interpreter()


def _run_source(source: str) -> int:
    try:
        _print_results(_interpreter().evaluate(source))
    except Exception as e:  # CLI boundary: report, don't traceback
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 0


def _repl() -> None:
    import contextlib

    with contextlib.suppress(ImportError):
        # ponytail: side-effecting import, enables line editing + history
        # on stdin; the name itself is never referenced.
        import readline  # noqa: F401  # pyright: ignore[reportUnusedImport]

    interp = _interpreter()
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
        try:
            _print_results(interp.evaluate(line))
        except Exception as e:  # keep the session alive on any error
            print(f"error: {e}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    """Entry point: file path (.phs) > expression args > piped stdin > interactive REPL."""
    from pathlib import Path

    args = sys.argv[1:] if argv is None else argv
    if args:
        path_candidate = (
            Path(args[0]) if len(args) == 1 else Path(" ".join(args))
        )
        if path_candidate.is_file():
            try:
                source = path_candidate.read_text(encoding="utf-8")
            except OSError as e:
                print(
                    f"error reading file '{path_candidate}': {e}",
                    file=sys.stderr,
                )
                return 1
            return _run_source(source)

        if len(args) == 1 and args[0].endswith(".phs"):
            print(f"error: file '{args[0]}' not found.", file=sys.stderr)
            return 1

        return _run_source(" ".join(args))
    if not sys.stdin.isatty():
        return _run_source(sys.stdin.read())
    _repl()
    return 0


if __name__ == "__main__":
    sys.exit(main())
