#!/bin/sh
# Atlas CLI installer
# Usage: curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh
set -e

ATLAS_REPO="codeatlasdev/atlas"
INSTALL_DIR="${ATLAS_INSTALL_DIR:-$HOME/.atlas/bin}"
CHANNEL="${ATLAS_CHANNEL:-stable}"

# Colors
BOLD="\033[1m"
DIM="\033[2m"
BLUE="\033[1;34m"
GREEN="\033[1;32m"
YELLOW="\033[1;33m"
RED="\033[1;31m"
RESET="\033[0m"

info() { printf "${BLUE}\u25cf${RESET} %s\n" "$1"; }
success() { printf "${GREEN}\u2713${RESET} %s\n" "$1"; }
warn() { printf "${YELLOW}\u25cf${RESET} %s\n" "$1"; }
error() { printf "${RED}\u2717${RESET} %s\n" "$1"; exit 1; }

# Detect platform
detect_platform() {
    ARCH=$(uname -m)
    OS=$(uname -s)

    case "$OS" in
        Darwin)
            case "$ARCH" in
                arm64) PLATFORM="aarch64-apple-darwin" ;;
                x86_64) PLATFORM="x86_64-apple-darwin" ;;
                *) error "Unsupported architecture: $ARCH" ;;
            esac
            ;;
        Linux)
            case "$ARCH" in
                x86_64) PLATFORM="x86_64-unknown-linux-gnu" ;;
                aarch64) PLATFORM="aarch64-unknown-linux-gnu" ;;
                *) error "Unsupported architecture: $ARCH" ;;
            esac
            ;;
        *) error "Unsupported OS: $OS" ;;
    esac
}

# Get latest version from GitHub
get_latest_version() {
    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/${ATLAS_REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": "v\([^"]*\)".*/\1/')
    elif command -v wget >/dev/null 2>&1; then
        VERSION=$(wget -qO- "https://api.github.com/repos/${ATLAS_REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": "v\([^"]*\)".*/\1/')
    else
        error "curl or wget is required"
    fi

    if [ -z "$VERSION" ]; then
        error "Could not determine latest version"
    fi
}

# Download and install
install() {
    TARBALL="atlas-${VERSION}-${PLATFORM}.tar.gz"
    URL="https://github.com/${ATLAS_REPO}/releases/download/v${VERSION}/${TARBALL}"

    info "Downloading atlas v${VERSION} (${PLATFORM})..."

    mkdir -p "$INSTALL_DIR"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$URL" | tar xz -C "$INSTALL_DIR"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "$URL" | tar xz -C "$INSTALL_DIR"
    fi

    if [ ! -f "$INSTALL_DIR/atlas" ]; then
        error "Installation failed. Binary not found after extraction."
    fi

    chmod +x "$INSTALL_DIR/atlas"
    success "Installed atlas v${VERSION} to ${INSTALL_DIR}/atlas"
}

# Verify code signature (macOS)
verify_signature() {
    if [ "$(uname -s)" = "Darwin" ] && command -v codesign >/dev/null 2>&1; then
        if codesign --verify "$INSTALL_DIR/atlas" 2>/dev/null; then
            success "Code signature verified"
        else
            warn "Binary is not code-signed (ad-hoc or unsigned)"
        fi
    fi
}

# Setup PATH
setup_path() {
    ATLAS_BIN="$INSTALL_DIR"

    case ":$PATH:" in
        *":$ATLAS_BIN:"*) return ;;
    esac

    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        zsh) RC_FILE="$HOME/.zshrc" ;;
        bash)
            if [ -f "$HOME/.bash_profile" ]; then
                RC_FILE="$HOME/.bash_profile"
            else
                RC_FILE="$HOME/.bashrc"
            fi
            ;;
        fish) RC_FILE="$HOME/.config/fish/config.fish" ;;
        *) RC_FILE="" ;;
    esac

    if [ -n "$RC_FILE" ]; then
        if ! grep -q ".atlas/bin" "$RC_FILE" 2>/dev/null; then
            printf "\n# Atlas CLI\nexport PATH=\"\$HOME/.atlas/bin:\$PATH\"\n" >> "$RC_FILE"
            info "Added \$HOME/.atlas/bin to PATH in $RC_FILE"
        fi
    fi
}

# Main
main() {
    printf "\n"
    printf "  ${BOLD}Atlas CLI Installer${RESET}\n"
    printf "  ${DIM}https://atlas.codeatlas.com.br${RESET}\n"
    printf "\n"

    detect_platform
    get_latest_version
    install
    verify_signature
    setup_path

    printf "\n"
    success "Installation complete!"
    printf "\n"
    printf "  ${DIM}Restart your shell or run:${RESET}\n"
    printf "  export PATH=\"\$HOME/.atlas/bin:\$PATH\"\n"
    printf "\n"
    printf "  ${DIM}Get started:${RESET}\n"
    printf "  atlas dev\n"
    printf "\n"
}

main
