#!/usr/bin/env bash
# rls installer: detects OS/arch, downloads the matching release binary and
# verifies it against the SHA256SUMS file published alongside every release
# before installing it. Run with: curl -fsSL .../install.sh | bash
set -euo pipefail

REPO="aleee-dev1/rls"
APP="rls"
INSTALL_DIR="$HOME/.local/bin"
BASE_URL="https://github.com/$REPO/releases/latest/download"

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
        echo "Unsupported platform: $OS $ARCH" >&2
        exit 1
        ;;
esac

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p "$INSTALL_DIR"

echo "Downloading $APP for $OS $ARCH..."
curl -fL --retry 3 "$BASE_URL/$ASSET" -o "$TMP_DIR/$APP"

echo "Verifying checksum..."
curl -fL --retry 3 "$BASE_URL/SHA256SUMS" -o "$TMP_DIR/SHA256SUMS"

# Extract the expected hash for this asset from SHA256SUMS, then compare
# against the downloaded file's actual sha256. Fails on any mismatch.
EXPECTED="$(awk -v asset="$ASSET" '$2 == asset { print $1 }' "$TMP_DIR/SHA256SUMS")"
if [ -z "$EXPECTED" ]; then
    echo "Checksum for $ASSET not found in SHA256SUMS — refusing to install." >&2
    exit 1
fi

ACTUAL="$(sha256sum "$TMP_DIR/$APP" | awk '{ print $1 }')"
if [ "$EXPECTED" != "$ACTUAL" ]; then
    echo "Checksum mismatch for $ASSET!" >&2
    echo "  expected: $EXPECTED" >&2
    echo "  actual:   $ACTUAL" >&2
    echo "The download may be corrupted or tampered with — refusing to install." >&2
    exit 1
fi

mv "$TMP_DIR/$APP" "$INSTALL_DIR/$APP"
chmod +x "$INSTALL_DIR/$APP"

echo "Installed $APP to $INSTALL_DIR/$APP"