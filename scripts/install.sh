#!/usr/bin/env bash
set -e

# PHS Native CLI & REPL Installer
# Usage: curl -fsSL https://physure.irvintorres.com/install.sh | bash

BOLD="$(tput bold 2>/dev/null || echo '')"
GREEN="$(tput setaf 2 2>/dev/null || echo '')"
CYAN="$(tput setaf 6 2>/dev/null || echo '')"
YELLOW="$(tput setaf 3 2>/dev/null || echo '')"
RESET="$(tput srgb 2>/dev/null || tput sgr0 2>/dev/null || echo '')"

echo -e "${BOLD}${CYAN}⚡ Installing Standalone PHS Native Executable...${RESET}\n"

INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"

BINARY_FOUND=0

# Option 1: Cargo build/install if cargo is available
if command -v cargo >/dev/null 2>&1; then
    echo "📦 Rust cargo detected. Building/installing phs..."
    cargo install --path /home/irvint/Projects/physure/physure-core --bin phs 2>/dev/null || cargo install physure --bin phs 2>/dev/null || true
fi

# Option 2: Copy existing binary if present
if [ -f "$HOME/.cargo/bin/phs" ]; then
    cp "$HOME/.cargo/bin/phs" "$INSTALL_DIR/phs"
    BINARY_FOUND=1
elif command -v phs >/dev/null 2>&1; then
    PHS_PATH="$(which phs)"
    cp "$PHS_PATH" "$INSTALL_DIR/phs"
    BINARY_FOUND=1
fi

if [ "$BINARY_FOUND" -ne 1 ]; then
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    ARCH="$(uname -m)"
    
    echo "📥 Downloading phs binary release for ${OS}-${ARCH}..."
    RELEASE_URL="https://github.com/Alexisrx96/physure/releases/latest/download/phs-${OS}-${ARCH}"
    
    if curl -fsSL "$RELEASE_URL" -o "$INSTALL_DIR/phs" 2>/dev/null; then
        chmod +x "$INSTALL_DIR/phs"
        BINARY_FOUND=1
    else
        echo -e "${YELLOW}Warning: Precompiled binary for ${OS}-${ARCH} not found.${RESET}"
    fi
fi

if [ "$BINARY_FOUND" -eq 1 ]; then
    chmod +x "$INSTALL_DIR/phs"
fi

# Add INSTALL_DIR to user PATH if not present
SHELL_NAME="$(basename "${SHELL:-bash}")"
PROFILE=""

case "$SHELL_NAME" in
    bash)
        if [ -f "$HOME/.bashrc" ]; then PROFILE="$HOME/.bashrc"; elif [ -f "$HOME/.bash_profile" ]; then PROFILE="$HOME/.bash_profile"; fi
        ;;
    zsh)
        PROFILE="$HOME/.zshrc"
        ;;
    fish)
        PROFILE="$HOME/.config/fish/config.fish"
        ;;
    *)
        if [ -f "$HOME/.profile" ]; then PROFILE="$HOME/.profile"; fi
        ;;
esac

PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
if [ "$SHELL_NAME" = "fish" ]; then
    PATH_LINE='set -gx PATH $HOME/.local/bin $PATH'
fi

if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    if [ -n "$PROFILE" ]; then
        if ! grep -q '\.local/bin' "$PROFILE" 2>/dev/null; then
            echo "" >> "$PROFILE"
            echo "# Added by PHS installer" >> "$PROFILE"
            echo "$PATH_LINE" >> "$PROFILE"
            echo "✨ Added $INSTALL_DIR to PATH in $PROFILE"
        fi
    fi
    export PATH="$HOME/.local/bin:$PATH"
fi

echo -e "\n${BOLD}${GREEN}🎉 PHS successfully installed!${RESET}"
echo -e "Try running: ${BOLD}phs${RESET} or ${BOLD}phs \"500 N / 2 m^2 => kPa\"${RESET}\n"
