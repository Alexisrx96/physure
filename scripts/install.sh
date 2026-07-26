#!/usr/bin/env bash
set -e

# phs (PhysureScript CLI) Installer — Linux & macOS
# Usage: curl -fsSL https://physure.irvintorres.com/install.sh | bash
# Install from a specific branch instead of the latest release:
#   PHS_BRANCH=main curl -fsSL https://physure.irvintorres.com/install.sh | bash

REPO="Alexisrx96/physure"
INSTALL_DIR="$HOME/.local/bin"
PHS_BRANCH="${PHS_BRANCH:-}"

BOLD="$(tput bold 2>/dev/null || echo '')"
GREEN="$(tput setaf 2 2>/dev/null || echo '')"
CYAN="$(tput setaf 6 2>/dev/null || echo '')"
RESET="$(tput sgr0 2>/dev/null || echo '')"

echo "${BOLD}${CYAN}⚡ Installing phs (PhysureScript CLI)...${RESET}"

mkdir -p "$INSTALL_DIR"

install_from_source() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "Rust/cargo not found. Install it from https://rustup.rs then re-run this script." >&2
        exit 1
    fi
    if [ -n "$PHS_BRANCH" ]; then
        echo "Building from source (branch: $PHS_BRANCH)..."
        cargo install --git "https://github.com/$REPO" --branch "$PHS_BRANCH" physure-cli --bin phs --locked --force
        cargo install --git "https://github.com/$REPO" --branch "$PHS_BRANCH" physure-lsp --locked --force \
            || echo "Warning: failed to build physure-lsp (VS Code language server); continuing without it." >&2
    else
        echo "Building from source (default branch)..."
        cargo install --git "https://github.com/$REPO" physure-cli --bin phs --locked --force
        cargo install --git "https://github.com/$REPO" physure-lsp --locked --force \
            || echo "Warning: failed to build physure-lsp (VS Code language server); continuing without it." >&2
    fi
    cp "$HOME/.cargo/bin/phs" "$INSTALL_DIR/phs"
    [ -f "$HOME/.cargo/bin/physure-lsp" ] && cp "$HOME/.cargo/bin/physure-lsp" "$INSTALL_DIR/physure-lsp"
}

installed=false
if [ -z "$PHS_BRANCH" ]; then
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64)  ASSET="phs-linux-x86_64.tar.gz" ;;
        Linux-aarch64) ASSET="phs-linux-aarch64.tar.gz" ;;
        Darwin-x86_64) ASSET="phs-macos-x86_64.tar.gz" ;;
        Darwin-arm64)  ASSET="phs-macos-aarch64.tar.gz" ;;
        *)             ASSET="" ;;
    esac

    if [ -n "$ASSET" ]; then
        TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases" 2>/dev/null | grep -m1 '"tag_name": *"core-v' | sed -E 's/.*"(core-v[^"]+)".*/\1/')"
        if [ -n "$TAG" ]; then
            TMPFILE="$(mktemp)"
            if curl -fsSL "https://github.com/$REPO/releases/download/$TAG/$ASSET" -o "$TMPFILE"; then
                tar -xzf "$TMPFILE" -C "$INSTALL_DIR"
                installed=true
            fi
            rm -f "$TMPFILE"
        fi
    fi
fi

if [ "$installed" = false ]; then
    if [ -z "$PHS_BRANCH" ]; then
        echo "No prebuilt binary for $(uname -s)-$(uname -m) — falling back to cargo."
    fi
    install_from_source
fi
chmod +x "$INSTALL_DIR/phs"
[ -f "$INSTALL_DIR/physure-lsp" ] && chmod +x "$INSTALL_DIR/physure-lsp"

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
