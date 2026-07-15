"""Allows ``python -m physure`` to start the unit calculator."""

import sys

from physure.repl import main

if __name__ == "__main__":
    sys.exit(main())
