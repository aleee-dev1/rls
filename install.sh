#!/usr/bin/env bash
set -e

REPO="aleee-dev1/rls"
APP="rls"
INSTALL_DIR="$HOME/.local/bin"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS-$ARCH" in
    Linux-x86_64)
        ASSET="rls-linux-x86_64"
        ;;
    Linux-aarch64|Linux-arm64)
        ASSET="rls-linux-aarch64"
        ;;
    Darwin-x86_64)
        ASSET="rls-macos-x86_64"
        ;;
    Darwin-arm64)
        ASSET="rls-macos-aarch64"
        ;;
    *)
        echo "Unsupported platform: $OS $ARCH"
        exit 1
        ;;
esac

mkdir -p "$INSTALL_DIR"

URL="https://github.com/$REPO/releases/latest/download/$ASSET"

echo "Downloading $APP..."
curl -fL "$URL" -o "$INSTALL_DIR/$APP"

chmod +x "$INSTALL_DIR/$APP"

echo "Installed $APP to $INSTALL_DIR/$APP"
