# Compilar

## Requisitos

| | Versión |
|---|---|
| Rust | 1.75+ (`rustup`) |
| Node.js | 20+ |
| pnpm | 9+ |

### Linux (adicional, para la GUI)

```bash
# Ubuntu / Debian
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel

# Arch
sudo pacman -S webkit2gtk-4.1 base-devel curl wget file openssl \
  libappindicator-gtk3 librsvg
```

Windows no necesita nada extra (WebView2 viene con el sistema; para compilar basta con
Visual Studio Build Tools y el SDK de Windows).

## Núcleo (sin GUI)

```bash
cargo test --workspace
cargo run -p wd-cli -- --detect-only
cargo run -p wd-cli -- --detect-only --json
cargo run -p wd-cli -- --detect-only --fixture tests/cases/rx6700xt-windows.json
```

Esta es la única parte necesaria para trabajar en la lógica de decisión y corre en cualquier
CI sin GPU.

## GUI (fase 2)

```bash
pnpm install
pnpm tauri dev
```

## Empaquetado

| Objetivo | Comando |
|---|---|
| Windows (ZIP portable) | `pnpm tauri build` + script de empaquetado |
| Linux `.deb` / `.rpm` | `pnpm tauri build --bundles deb,rpm` |
| Arch `.pkg.tar.zst` | `cd packaging/aur && makepkg -s` |
| `.run` universal | `scripts/build-run.sh` |

## Añadir un caso de hardware

Copia un `HardwareReport` real y añádelo a `tests/cases/` con el bloque `expect`. Los tests
lo recogen automáticamente.
