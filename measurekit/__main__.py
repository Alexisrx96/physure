"""Allows ``python -m measurekit`` to start the unit calculator."""

import sys

from measurekit.repl import main

if __name__ == "__main__":
    sys.exit(main())
