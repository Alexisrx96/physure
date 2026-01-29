# Build and Upload

## Prerequisites

Ensure you have the build tools installed:
pip install build twine maturin

## 1. Publish Core (Rust)

The core must be published first as it is a dependency.

cd measurekit_core
maturin publish
cd ..

## 2. Publish Main Package (Python)

To build the package (uses hatchling backend):
python -m build

To check the package artifacts:
twine check dist/\*

To upload to PyPI:
twine upload dist/\*
