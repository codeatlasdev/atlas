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
    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Darwin) OS="apple-darwin" ;;
        Linux) OS="unknown-linux-gnu" ;;
        *) error "Unsupported OS: $OS" ;;
    esac

    case "$ARCH" in
        arm64|aarch64) ARCH="aarch64" ;;
        x86_64|amd64) ARCH="x86_64" ;;
        *) error "Unsupported architecture: $ARCH" ;;
    esac

    PLATFORM="${ARCH}-${OS}"
}

# Get latest version from GitHub
get_latest_version() {
    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/${ATLAS_REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"v\(.*\)".*/\1/')
    elif command -v wget >/dev/null 2>&1; then
        VERSION=$(wget -qO- "https://api.github.com/repos/${ATLAS_REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"v\(.*\)".*/\1/')
    else
        error "Neither curl nor wget found. Please install one of them."
    fi

    if [ -z "$VERSION" ]; then
        error "Could not determine latest version. Check your internet connection."
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
    if [ -f "$INSTALL_DIR/atlas-daemon" ]; then
        chmod +x "$INSTALL_DIR/atlas-daemon"
    fi

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

    # Check if already in PATH
    case ":$PATH:" in
        *":$ATLAS_BIN:"*) return ;;
    esac

    # Detect shell and rc file
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

# Install launchd agent for daemon
install_daemon_service() {
    if [ ! -f "$INSTALL_DIR/atlas-daemon" ]; then
        return
    fi

    PLIST_DIR="$HOME/Library/LaunchAgents"
    PLIST_FILE="$PLIST_DIR/dev.codeatlas.daemon.plist"

    if [ -f "$PLIST_FILE" ]; then
        # Already installed, reload
        launchctl unload "$PLIST_FILE" 2>/dev/null || true
    fi

    mkdir -p "$PLIST_DIR"
    mkdir -p "$HOME/.atlas/logs"

    cat > "$PLIST_FILE" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.codeatlas.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALL_DIR}/atlas-daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${HOME}/.atlas/logs/daemon.log</string>
    <key>StandardErrorPath</key>
    <string>${HOME}/.atlas/logs/daemon.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>${HOME}</string>
    </dict>
</dict>
</plist>
EOF

    launchctl load "$PLIST_FILE" 2>/dev/null || true
    success "Daemon service installed (launchd)"
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

    # Only install daemon service on macOS
    if [ "$(uname -s)" = "Darwin" ]; then
        install_daemon_service
    fi

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
