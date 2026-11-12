#!/usr/bin/env bash
# Fetch the exact upstream source versions this project builds against.
# Runs in Linux/WSL. Sources go to $SRC (default /root/nspanel/src).
set -euo pipefail

KERNEL_TAG="${KERNEL_TAG:-v7.3.1}"       # latest stable at the time; move to the 2026 longterm kernel when announced
BUILDROOT_TAG="${BUILDROOT_TAG:-2026.08.1}"
# The commits these tags pointed to when they were tested: a moved tag stops the build.
KERNEL_COMMIT="${KERNEL_COMMIT:-79a907609d53d5cd645b42c11550d295d43abf99}"
BUILDROOT_COMMIT="${BUILDROOT_COMMIT:-7fdeadb3b9f12f78e5fccca914d7fef47090ab9a}"
SRC="${SRC:-/root/nspanel/src}"

mkdir -p "$SRC"
cd "$SRC"

check() {  # dir commit
  local head
  head=$(git -C "$1" rev-parse HEAD)
  if [ "$head" != "$2" ]; then
    echo "$1: at $head, expected $2 (tag moved or wrong source)" >&2
    exit 1
  fi
}

fetch() {  # dir url tag commit
  local dir=$1 url=$2 tag=$3 commit=$4
  if [ -d "$dir/.git" ] && [ "$(git -C "$dir" describe --tags 2>/dev/null)" = "$tag" ]; then
    echo "$dir: already at $tag"
    check "$dir" "$commit"
    return
  fi
  if [ -d "$dir" ]; then
    echo "$dir: replacing $(git -C "$dir" describe --tags 2>/dev/null || echo unknown) with $tag"
    rm -rf "$dir"
  fi
  git clone --depth 1 --branch "$tag" "$url" "$dir" 2>&1 | tail -1
  check "$dir" "$commit"
}

fetch linux https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git "$KERNEL_TAG" "$KERNEL_COMMIT"
fetch buildroot https://gitlab.com/buildroot.org/buildroot.git "$BUILDROOT_TAG" "$BUILDROOT_COMMIT"
echo "kernel: $(make -s -C linux kernelversion)  buildroot: $BUILDROOT_TAG"
