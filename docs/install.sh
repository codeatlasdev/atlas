#!/bin/bash
set -euo pipefail

REPO="codeatlasdev/atlas"
INSTALL_DIR="${ATLAS_INSTALL_DIR:-$HOME/.atlas/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

info() { echo -e "${CYAN}→${NC} $1"; }
ok() { echo -e "${GREEN}✓${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; exit 1; }

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)  PLATFORM="linux" ;;
  darwin) PLATFORM="darwin" ;;
  *)      fail "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH="x64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *)             fail "Unsupported architecture: $ARCH" ;;
esac

TARGET="atlas-${PLATFORM}-${ARCH}"
info "Detected ${PLATFORM}-${ARCH}"

# Get latest release
info "Fetching latest release..."
RELEASE_URL=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep "browser_download_url.*${TARGET}" \
  | head -1 \
  | cut -d '"' -f 4)

if [ -z "$RELEASE_URL" ]; then
  fail "No binary found for ${TARGET}. Check https://github.com/${REPO}/releases"
fi

# Download
info "Downloading ${TARGET}..."
mkdir -p "$INSTALL_DIR"
curl -fsSL "$RELEASE_URL" -o "${INSTALL_DIR}/atlas"
chmod +x "${INSTALL_DIR}/atlas"

ok "Installed to ${INSTALL_DIR}/atlas"

# Add to PATH
SHELL_NAME=$(basename "$SHELL")
PROFILE=""
case "$SHELL_NAME" in
  bash) PROFILE="$HOME/.bashrc" ;;
  zsh)  PROFILE="$HOME/.zshrc" ;;
  fish) PROFILE="$HOME/.config/fish/config.fish" ;;
esac

if [ -n "$PROFILE" ] && ! grep -q "$INSTALL_DIR" "$PROFILE" 2>/dev/null; then
  if [ "$SHELL_NAME" = "fish" ]; then
    echo "set -gx PATH $INSTALL_DIR \$PATH" >> "$PROFILE"
  else
    echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$PROFILE"
  fi
  ok "Added to PATH in $PROFILE"
  info "Run: source $PROFILE"
else
  ok "Already in PATH"
fi

echo ""
echo -e "${GREEN}Atlas installed!${NC} Run: atlas --help"
