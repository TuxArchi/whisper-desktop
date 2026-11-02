#!/usr/bin/env bash
# Builds the Arch package from an already compiled binary.
#
# Usage: make-arch-package.sh <binary-dir> <version> <out-dir>
# Requires makepkg (Arch or an archlinux container).
#
# The package is intentionally prebuilt: "install by double-clicking" means the
# user must not need a Rust toolchain, and compiling whisper.cpp is the
# installer's job anyway.

set -euo pipefail

bindir="${1:?uso: make-arch-package.sh <binary-dir> <version> <out-dir>}"
version="${2:?falta la versión}"
out="${3:-release}"

root="$(cd "$(dirname "$0")/.." && pwd)"
# Absolute: the script later changes directory, and a relative "release/" would
# resolve somewhere else (it broke the CI arch job).
out="$(mkdir -p "$out" && cd "$out" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

stage="$work/stage"
mkdir -p "$stage"

cp "$bindir/whisper-desktop" "$stage/"
cp "$root/LICENSE" "$root/THIRD-PARTY.md" "$stage/"
cp "$root/packaging/whisper-desktop.desktop" "$stage/"
cp "$root/packaging/icons/256x256/whisper-desktop.png" "$stage/"

cp "$root/packaging/aur/PKGBUILD" "$work/PKGBUILD"
sed -i "s/^pkgver=.*/pkgver=$version/" "$work/PKGBUILD"

tar -czf "$work/whisper-desktop-$version-x86_64.tar.gz" -C "$stage" .

mkdir -p "$out"
(
    cd "$work"
    makepkg -f --nodeps
    cp ./*.pkg.tar.zst "$out/"
)

echo "listo: $(ls "$out"/whisper-desktop-*.pkg.tar.zst | tail -1)"
