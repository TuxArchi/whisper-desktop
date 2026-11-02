#!/usr/bin/env bash
# Builds the RPM package from an already compiled binary.
#
# Usage: make-rpm.sh <binary-dir> <version> <out-dir>
#
# rpmbuild ships only on RPM distributions, so the default path runs Fedora in
# a container; that keeps the result identical to what CI would produce.

set -euo pipefail

bindir="${1:?uso: make-rpm.sh <binary-dir> <version> <out-dir>}"
version="${2:?falta la versión}"
out="${3:-release}"

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$(mkdir -p "$out" && cd "$out" && pwd)"

work="$(mktemp -d)"
trap 'rm -rf "$work" 2>/dev/null || true' EXIT

mkdir -p "$work/sources" "$work/rpmbuild"
cp "$bindir/whisper-desktop" "$work/sources/"
cp "$root/packaging/whisper-desktop.desktop" "$work/sources/"
cp "$root/packaging/icons/256x256/whisper-desktop.png" "$work/sources/"
cp "$root/LICENSE" "$root/THIRD-PARTY.md" "$work/sources/"
cp "$root/packaging/rpm/whisper-desktop.spec" "$work/"

# Single line on purpose: nested quoting inside `bash -c` plus line
# continuations is a reliable way to produce broken scripts.
container_cmd='dnf -y -q install rpm-build >/dev/null && rpmbuild -bb --define "_topdir /work/rpmbuild" --define "_sourcedir /work/sources" --define "version_override $VER" /work/whisper-desktop.spec >/dev/null && chown -R "$UID_HOST:$GID_HOST" /work/rpmbuild'

if command -v rpmbuild >/dev/null 2>&1; then
    rpmbuild -bb \
        --define "_topdir $work/rpmbuild" \
        --define "_sourcedir $work/sources" \
        --define "version_override $version" \
        "$work/whisper-desktop.spec" >/dev/null
else
    docker run --rm \
        -v "$work":/work \
        -e VER="$version" \
        -e UID_HOST="$(id -u)" \
        -e GID_HOST="$(id -g)" \
        fedora:latest \
        bash -c "$container_cmd" >/dev/null
fi

found="$(find "$work/rpmbuild/RPMS" -name '*.rpm' -print -quit)"
[ -n "$found" ] || { echo "rpmbuild no generó ningún paquete" >&2; exit 1; }
cp "$found" "$out/"
echo "listo: $out/$(basename "$found")"
