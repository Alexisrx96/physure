#!/usr/bin/env bash
set -e

# phs (PhysureScript CLI) Installer — Linux & macOS
# Usage: curl -fsSL https://physure.irvintorres.com/install.sh | bash

BOLD="$(tput bold 2>/dev/null || echo '')"
GREEN="$(tput setaf 2 2>/dev/null || echo '')"
CYAN="$(tput setaf 6 2>/dev/null || echo '')"
RESET="$(tput sgr0 2>/dev/null || echo '')"

echo "${BOLD}${CYAN}⚡ Installing phs (PhysureScript CLI)...${RESET}"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust/cargo not found. Install it from https://rustup.rs then re-run this script." >&2
    exit 1
fi

# physure-cli (the phs binary) isn't published to crates.io, so install
# straight from the repo — works identically on Linux and macOS.
cargo install --git https://github.com/Alexisrx96/physure physure-cli --bin phs --locked --force

INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"
cp "$HOME/.cargo/bin/phs" "$INSTALL_DIR/phs"
chmod +x "$INSTALL_DIR/phs"

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
    if [ -n "$PROFILE" ] && ! grep -q '\.local/bin' "$PROFILE" 2>/dev/null; then
        echo "" >> "$PROFILE"
        echo "# Added by PHS installer" >> "$PROFILE"
        echo "$PATH_LINE" >> "$PROFILE"
        echo "✨ Added $INSTALL_DIR to PATH in $PROFILE (restart your shell)"
    fi
    export PATH="$HOME/.local/bin:$PATH"
fi

echo -e "\n${BOLD}${GREEN}🎉 phs successfully installed!${RESET}"
echo -e "Try running: ${BOLD}phs${RESET} or ${BOLD}phs \"500 N / 2 m^2 => kPa\"${RESET}\n"
