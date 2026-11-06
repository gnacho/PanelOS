#!/usr/bin/env bash
# Fetch the exact upstream source versions this project builds against.
# Runs in Linux/WSL. Sources go to $SRC (default /root/nspanel/src).
set -euo pipefail

KERNEL_TAG="${KERNEL_TAG:-v7.3.1}"       # latest stable at the time; move to the 2026 longterm kernel when announced
BUILDROOT_TAG="${BUILDROOT_TAG:-2026.08.1}"
SRC="${SRC:-/root/nspanel/src}"

mkdir -p "$SRC"
cd "$SRC"

fetch() {  # dir url tag
  local dir=$1 url=$2 tag=$3
  if [ -d "$dir/.git" ] && [ "$(git -C "$dir" describe --tags 2>/dev/null)" = "$tag" ]; then
    echo "$dir: already at $tag"
    return
  fi
  if [ -d "$dir" ]; then
    echo "$dir: replacing $(git -C "$dir" describe --tags 2>/dev/null || echo unknown) with $tag"
    rm -rf "$dir"
  fi
  git clone --depth 1 --branch "$tag" "$url" "$dir" 2>&1 | tail -1
}

fetch linux https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git "$KERNEL_TAG"
fetch buildroot https://gitlab.com/buildroot.org/buildroot.git "$BUILDROOT_TAG"
echo "kernel: $(make -s -C linux kernelversion)  buildroot: $BUILDROOT_TAG"
