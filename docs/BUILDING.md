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
| Windows (ZIP portable) | `npm run tauri build -- --no-bundle` + `scripts/package-windows.ps1` |
| Linux `.deb` / `.rpm` (CI) | `npm run tauri build -- --bundles deb,rpm` |
| Linux `.deb` / `.rpm` (local) | `scripts/make-deb.sh` · `scripts/make-rpm.sh` |
| Arch `.pkg.tar.zst` | `scripts/make-arch-package.sh src-tauri/target/release 0.1.0 release` |
| `.run` universal | `scripts/make-run.sh <stage> <version> release` |

En CI, `.deb` y `.rpm` los genera el bundler de Tauri sobre Ubuntu 22.04: calcula
las dependencias con `dpkg-shlibdeps` y fija glibc 2.35. Los scripts
`make-deb.sh` / `make-rpm.sh` existen para empaquetar un binario ya compilado
desde una máquina que no es Debian ni Fedora (usan contenedores si falta la
herramienta nativa).

> El binario hereda la glibc del sistema donde se compiló. Compilado en Arch
> (glibc 2.44) exige **glibc ≥ 2.39**; compilado en Ubuntu 22.04 baja a 2.35,
> que cubre Ubuntu 22.04+, Debian 12+ y Fedora 36+.)

El `.run` y el paquete de Arch necesitan un binario ya compilado:

```bash
cargo build --release --manifest-path src-tauri/Cargo.toml
mkdir -p stage
cp src-tauri/target/release/whisper-desktop LICENSE THIRD-PARTY.md stage/
cp packaging/icons/256x256/whisper-desktop.png stage/
bash scripts/make-run.sh stage 0.1.0 release
```

Ambos se prueban en el workflow con un `$HOME` temporal, sin tocar el sistema.

## Añadir un caso de hardware

Copia un `HardwareReport` real y añádelo a `tests/cases/` con el bloque `expect`. Los tests
lo recogen automáticamente.
