"""Pre-configured unit systems."""

from measurekit.application.startup import create_system

# Create the Imperial system instance by loading its configuration.
imperial = create_system("imperial.conf")

# Create the International (SI) system instance.
international = create_system("international.conf")
