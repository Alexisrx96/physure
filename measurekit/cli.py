# measurekit/cli.py
import argparse
import sys

from measurekit.scripts.generate_types import generate


def main():
    """Entry point for the MeasureKit CLI."""
    parser = argparse.ArgumentParser(description="MeasureKit CLI Tool")
    subparsers = parser.add_subparsers(dest="command")

    # sync-types command
    _ = subparsers.add_parser(
        "sync-types", help="Generate type hints for available units."
    )

    args = parser.parse_args()

    if args.command == "sync-types":
        print("Synchronizing types for MeasureKit units...")
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
