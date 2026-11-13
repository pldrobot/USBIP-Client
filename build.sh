#!/usr/bin/env bash
# Builds the Linux binary and .deb package: dist/usbip-client, dist/usbip-client_<ver>_amd64.deb
# Requires: Rust, and the GTK/appindicator dev headers listed below.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

if ! command -v cargo >/dev/null; then
    echo "cargo not found — install Rust first: https://rustup.rs" >&2
    exit 1
fi

if ! pkg-config --exists gtk+-3.0 ayatana-appindicator3-0.1 2>/dev/null; then
    echo "Missing build deps. On Debian/Ubuntu:" >&2
    echo "  sudo apt install build-essential pkg-config libgtk-3-dev libayatana-appindicator3-dev" >&2
    exit 1
fi

cargo build --release

if ! command -v cargo-deb >/dev/null; then
    echo "Installing cargo-deb ..."
    cargo install cargo-deb --locked
fi
# To build an .rpm instead: cargo install cargo-generate-rpm && cargo generate-rpm
cargo deb --no-build

mkdir -p dist
cp target/release/usbip-client dist/
cp target/debian/*.deb dist/

echo "Built:"
ls -la dist/
