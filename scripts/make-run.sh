#!/usr/bin/env bash
# Builds the universal Linux installer: a shell stub with a tar.gz payload
# appended. The stub installs per-user (no root) and asks the app itself to do
# the desktop integration, so there is only one implementation of it.
#
# Usage: make-run.sh <stage-dir> <version> <out-dir>
# The stage directory must contain the binary, the icon and the licences.

set -euo pipefail

stage="${1:?uso: make-run.sh <stage-dir> <version> <out-dir>}"
version="${2:?falta la versión}"
out="${3:-release}"

[ -x "$stage/whisper-desktop" ] || {
  echo "falta el binario en $stage/whisper-desktop" >&2
  exit 1
}

mkdir -p "$out"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

payload="$work/payload.tar.gz"
tar -C "$stage" -czf "$payload" .

header="$work/header.sh"
cat > "$header" <<'HEADER'
#!/usr/bin/env bash
# Whisper Desktop installer for Linux.
# Installs per user (no root), creates the menu entry and a CLI symlink.
set -euo pipefail

APP_NAME="Whisper Desktop"
BIN_NAME="whisper-desktop"
VERSION="__VERSION__"
PAYLOAD_LINE=__PAYLOAD_LINE__

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
DEST="$DATA_HOME/whisper-desktop/app"

die() { printf 'error: %s\n' "$*" >&2; exit 1; }

check_dependencies() {
    if ! command -v pkg-config >/dev/null 2>&1; then
        die "falta pkg-config; instálalo con el gestor de paquetes de tu distribución"
    fi

    if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
        return 0
    fi

    printf '\nFalta webkit2gtk-4.1, que es lo único que la aplicación necesita del sistema.\n\n'
    printf '  Ubuntu / Debian : sudo apt install libwebkit2gtk-4.1-0\n'
    printf '  Fedora          : sudo dnf install webkit2gtk4.1\n'
    printf '  openSUSE        : sudo zypper install libwebkit2gtk-4_1-0\n'
    printf '  Arch            : sudo pacman -S webkit2gtk-4.1\n\n'
    printf 'Instálalo y vuelve a ejecutar este archivo.\n'
    exit 1
}

install_app() {
    check_dependencies

    printf 'Instalando %s %s en %s\n' "$APP_NAME" "$VERSION" "$DEST"
    mkdir -p "$DEST"

    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    tail -n +"$PAYLOAD_LINE" "$0" | tar -xz -C "$work"
    cp -a "$work"/. "$DEST"/
    chmod +x "$DEST/$BIN_NAME"

    # The application knows how to create its own menu entry, icon and
    # symlink, and how to remove them again.
    "$DEST/$BIN_NAME" --integrate || printf 'aviso: no se pudo completar la integración\n'

    printf '\nListo. Abre "%s" desde el menú de aplicaciones.\n' "$APP_NAME"
    printf 'Desinstalar: %s --uninstall\n' "$0"
}

uninstall_app() {
    if [ -x "$DEST/$BIN_NAME" ]; then
        "$DEST/$BIN_NAME" --unintegrate || true
    fi
    rm -rf "$DEST"
    printf 'Desinstalado. Los modelos y los datos se conservan en %s\n' \
        "$DATA_HOME/whisper-desktop"
}

case "${1:-}" in
    --check)
        check_dependencies
        printf 'Todo listo para instalar %s %s\n' "$APP_NAME" "$VERSION"
        ;;
    --uninstall)
        uninstall_app
        ;;
    --upgrade|--install)
        install_app
        ;;
    --help|-h)
        printf 'Uso: %s [--check|--uninstall|--upgrade]\n' "$0"
        ;;
    *)
        install_app
        ;;
esac

# Everything below this line is the tar.gz payload. Without this exit, bash
# keeps reading and tries to execute the binary as shell commands.
exit 0
HEADER

line_count=$(wc -l < "$header")
payload_line=$((line_count + 1))
sed -i "s|__VERSION__|$version|; s|__PAYLOAD_LINE__|$payload_line|" "$header"

target="$out/WhisperDesktop-$version-x64.run"
cat "$header" "$payload" > "$target"
chmod +x "$target"

printf 'listo: %s (%s)\n' "$target" "$(du -h "$target" | cut -f1)"
