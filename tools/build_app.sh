#!/usr/bin/env bash
# Build the panel app (app/) for the panel (aarch64) with Buildroot's Rust
# toolchain (BR2_PACKAGE_HOST_RUSTC, prebuilt) and its cross linker.
# Run as root in Linux/WSL after tools/build_image.sh has built host-rustc.
#
# Usage: tools/build_app.sh            -> $WORK/target/aarch64-unknown-linux-gnu/release/nspanel-app
# Run on the panel: copy the binary over SSH and start it (it takes over the display).
set -euo pipefail

BROUT="${BROUT:-/root/nspanel/br-out}"
WORK="${WORK:-/root/nspanel/app-build}"
export PATH="$BROUT/host/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
export CARGO_HOME="${CARGO_HOME:-/root/nspanel/cargo-home}"
export CARGO_TARGET_DIR="$WORK/target"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$BROUT/host/bin/aarch64-linux-gcc"
# Native libraries (fontconfig) come from Buildroot's staging tree; its
# pkg-config wrapper already points at the target sysroot.
export PKG_CONFIG="$BROUT/host/bin/pkg-config"
export PKG_CONFIG_ALLOW_CROSS=1
# C/C++ parts (wake word engine) with Buildroot's cross compilers (they use the sysroot).
export CC_aarch64_unknown_linux_gnu="$BROUT/host/bin/aarch64-linux-gcc"
export CXX_aarch64_unknown_linux_gnu="$BROUT/host/bin/aarch64-linux-g++"
export AR_aarch64_unknown_linux_gnu="$BROUT/host/bin/aarch64-linux-ar"

REPO="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$WORK/src"
# Copy the sources out of the (Windows) repository folder; keep the target dir.
rm -rf "$WORK/src"
cp -r "$REPO/app" "$WORK/src"
rm -rf "$WORK/src/target"

cd "$WORK/src"
cargo build --release --target aarch64-unknown-linux-gnu
# Keep the resolved dependency versions in the repository.
cp Cargo.lock "$REPO/app/Cargo.lock"
ls -la "$CARGO_TARGET_DIR/aarch64-unknown-linux-gnu/release/nspanel-app"
