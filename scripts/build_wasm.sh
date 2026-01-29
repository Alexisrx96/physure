
#!/bin/bash
set -e

echo "Starting WASM build for MeasureKit Core..."

# Check requirements
if ! command -v rustup &> /dev/null; then
    echo "Error: rustup is not installed."
    exit 1
fi

if ! command -v maturin &> /dev/null; then
    echo "Installing maturin..."
    pip install maturin
fi

# Need emcc
if ! command -v emcc &> /dev/null; then
    echo "Error: emcc (Emscripten compiler) not found."
    echo "Please activate Emscripten SDK environment:"
    echo "  source /path/to/emsdk/emsdk_env.sh"
    exit 1
fi

# Add target
echo "Adding wasm32-unknown-emscripten target..."
rustup target add wasm32-unknown-emscripten

# Build
echo "Building wheel..."
cd measurekit_core
maturin build --release --target wasm32-unknown-emscripten -o ../dist

echo "Build complete. Wheel saved to dist/"
