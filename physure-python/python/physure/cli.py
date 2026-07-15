# physure/cli.py
import argparse
import sys

from physure.scripts.generate_types import generate


def main():
    """Entry point for the Physure CLI."""
    parser = argparse.ArgumentParser(description="Physure CLI Tool")
    subparsers = parser.add_subparsers(dest="command")

    # sync-types command
    _ = subparsers.add_parser(
        "sync-types", help="Generate type hints for available units."
    )

    # repl command
    repl_parser = subparsers.add_parser(
        "repl", help="Interactive unit-aware calculator (MNML syntax)."
    )
    repl_parser.add_argument(
        "expression",
        nargs="*",
        help="Evaluate this expression and exit instead of starting a REPL.",
    )

    args = parser.parse_args()

    if args.command == "repl":
        from physure.repl import main as repl_main

        sys.exit(repl_main(args.expression))

    if args.command == "sync-types":
        print("Synchronizing types for Physure units...")
        try:
            generate()
            print(
                "Successfully generated _generated_types.py and registry.pyi"
            )
        except Exception as e:
            print(f"Error generating types: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
