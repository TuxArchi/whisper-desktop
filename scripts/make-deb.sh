#!/usr/bin/env bash
# Builds the Debian package from an already compiled binary.
#
# Usage: make-deb.sh <binary-dir> <version> <out-dir>
#
# Uses dpkg-deb when available; otherwise runs the build inside a Debian
# container so the package is byte-compatible with what CI produces.

set -euo pipefail

bindir="${1:?uso: make-deb.sh <binary-dir> <version> <out-dir>}"
version="${2:?falta la versión}"
out="${3:-release}"

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$(mkdir -p "$out" && cd "$out" && pwd)"

work="$(mktemp -d)"
trap 'rm -rf "$work" 2>/dev/null || true' EXIT

tree="$work/pkg"
mkdir -p "$tree/DEBIAN" \
    "$tree/usr/bin" \
    "$tree/usr/share/applications" \
    "$tree/usr/share/icons/hicolor/256x256/apps" \
    "$tree/usr/share/doc/whisper-desktop"

install -m755 "$bindir/whisper-desktop" "$tree/usr/bin/whisper-desktop"
install -m644 "$root/packaging/whisper-desktop.desktop" \
    "$tree/usr/share/applications/whisper-desktop.desktop"
install -m644 "$root/packaging/icons/256x256/whisper-desktop.png" \
    "$tree/usr/share/icons/hicolor/256x256/apps/whisper-desktop.png"
install -m644 "$root/LICENSE" "$tree/usr/share/doc/whisper-desktop/copyright"
install -m644 "$root/THIRD-PARTY.md" "$tree/usr/share/doc/whisper-desktop/THIRD-PARTY.md"

cat > "$tree/DEBIAN/control" <<CONTROL
Package: whisper-desktop
Version: $version
Section: sound
Priority: optional
Architecture: amd64
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, librsvg2-2
Installed-Size: $(du -sk "$tree" | cut -f1)
Maintainer: TuxArchi <TuxArchi@users.noreply.github.com>
Homepage: https://github.com/TuxArchi/whisper-desktop
Description: Transcribe y traduce audio en local
 Whisper Desktop detects your GPU, picks the best whisper.cpp backend
 (CUDA, Vulkan, ROCm or CPU) and model, and leaves the app ready to
 transcribe and translate audio without Python or an internet connection.
CONTROL

name="WhisperDesktop_${version}_amd64.deb"

if command -v dpkg-deb >/dev/null 2>&1; then
    dpkg-deb --root-owner-group --build "$tree" "$out/$name" >/dev/null
else
    docker run --rm -v "$work":/work -e UID_HOST="$(id -u)" -e GID_HOST="$(id -g)" \
        debian:12 bash -c 'dpkg-deb --root-owner-group --build /work/pkg /work/out.deb >/dev/null &&
            chown "$UID_HOST:$GID_HOST" /work/out.deb'
    mv "$work/out.deb" "$out/$name"
fi

echo "listo: $out/$name"
