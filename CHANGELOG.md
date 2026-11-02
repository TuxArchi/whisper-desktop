# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y
[SemVer](https://semver.org/lang/es/).

## [0.1.5] - 2026-11-02

### Corregido

- **SIGILL al transcribir**: los motores se compilaban con `-march=native` en el
  runner de CI (AVX-512) y abortaban en CPUs sin esa extensión. Ahora se
  compilan 14 variantes y se elige en tiempo de ejecución
  (`GGML_NATIVE=OFF`, `GGML_CPU_ALL_VARIANTS`, `GGML_BACKEND_DL`).
- Elegir un modelo distinto del recomendado instalaba siempre el recomendado
  (closure obsoleto en el asistente).

### Añadido

- Reparador de motor: re-descarga el backend conservando modelos y configuración.
- Workflow `republish-catalog.yml` para regenerar el catálogo sin recompilar.

## [0.1.3] - 2026-11-02

### Añadido

- `packaging/licenses/` con la LGPL v3 de ffmpeg, la oferta de código fuente y el
  MIT de whisper.cpp.
- `NOTICES.md` generado (`scripts/make-notices.mjs`): 110 dependencias, 37 textos
  únicos.
- Las licencias viajan en `.deb`, `.rpm`, `.pkg.tar.zst`, `.run` y el ZIP.
- Enlace de Ko-fi en la web, el README y `.github/FUNDING.yml`.

### Corregido

- `THIRD-PARTY.md` apuntaba a archivos que no existían.

## [0.1.2] - 2026-11-02

### Corregido

- Windows: la VRAM de una tarjeta de 8 GB se reportaba como 4 GB. `AdapterRAM`
  satura en 4095 MB y tapaba el valor de `nvidia-smi`; ahora manda `nvidia-smi`
  y el valor saturado se descarta. Nuevo campo `vramSource`.
- Windows: al arrancar se abrían varias ventanas de PowerShell. Ahora toda la
  detección va en una sola invocación y todos los procesos hijos se lanzan con
  `CREATE_NO_WINDOW` (incluidos ffmpeg y whisper-cli durante la transcripción).

### Añadido

- Selector de modelo en la pantalla de recomendación, con tamaño, VRAM estimada
  y aviso si el modelo no cabe en la GPU.

## [No publicado]

### Corregido (encontrados instalando de verdad)

- `build-backend.sh` empaquetaba solo `build/bin`: las `.so` viven en
  `build/src` y `build/ggml/src`, así que el motor fallaba al arrancar con
  «cannot open shared object file».
- La verificación no reconocía un build de CPU: whisper.cpp imprime
  «no GPU found», no «using CPU backend».
- Los comandos no incluían `-of`, así que whisper escribía junto al audio de
  trabajo con el nombre del WAV en vez de en la carpeta de salida.
- El directorio de salida no se creaba antes de transcribir.
- El manifest incluía enlaces simbólicos y comparaba el tamaño del enlace con
  el del archivo de destino, por lo que la instalación se daba por rota.
- Los tests de «mover datos» escribían en el `location.txt` real del usuario;
  ahora la ruta del puntero es un parámetro.
- `make-catalog.mjs` no se ejecutaba desde rutas con espacios.
- El plan no se adaptaba a lo publicado: si el catálogo no tenía el backend o
  el modelo recomendados, fallaba en vez de usar la mejor alternativa.

### Añadido

- `.deb` y `.rpm` generados desde el binario compilado, con scripts que usan
  contenedores cuando la herramienta nativa no está disponible.
- Verificados instalándolos en limpio: Ubuntu 24.04 (apt resuelve dependencias)
  y Fedora 44 (dnf), y ejecutando `whisper-desktop --version` en ambos.


- Instalador universal `.run` para Linux: sin root, instala en `~/.local/share`,
  crea menú, icono y comando, y se desinstala con `--uninstall`.
- Paquete de Arch `.pkg.tar.zst` generado desde el binario del release.
- El binario acepta `--detect-only`, `--integrate`, `--unintegrate` y `--version`
  sin abrir ventana.
- SIGPIPE restaurado: `--detect-only | head` ya no provoca un panic.


- Catálogo remoto firmado con Ed25519 (`scripts/trust.mjs`, `crates/wd-core/src/trust.rs`).
  La app descarga `catalog.json` + `.sig`, verifica la firma, lo cachea y revalida la
  caché; si no hay red usa la copia embebida, que también está firmada.
- El catálogo se publica en GitHub Pages (`docs/catalog.json`) para tener una URL estable
  y se adjunta al release de backends.


- Workspace Rust con `wd-core` (detección + planificación) y `wd-cli`.
- Detección de hardware en Linux (`/sys`, `/proc`, `lspci`, `nvidia-smi`, `rocminfo`,
  `vulkaninfo`) y en Windows (PowerShell/CIM, registro, `nvidia-smi`).
- Matriz de decisión backend/modelo con motivos traducibles (es/en).
- 12 casos de prueba que cubren NVIDIA, AMD RDNA1–4, Ryzen AI, Intel, CPU y VM.
- `whisper-desktop --detect-only [--json] [--fixture]`.
- Catálogo remoto con validación (HTTPS, SHA-256, tamaño, versión mínima de app).
- Configuración de usuario con carga parcial tolerante a campos ausentes.
- Manifest de instalación con comprobación de integridad y rutas seguras.
- Rutas portable (Windows) y XDG (Linux) con detección de carpeta de solo lectura
  y de ejecución desde la vista previa del ZIP.
- Verificación posterior: parseo del backend real desde el log de whisper.cpp,
  cálculo de RTF y selección del siguiente fallback.
- Descarga verificada con reanudación por rangos y borrado del archivo si el
  SHA-256 no coincide.
- Extracción de `.zip` y `.tar.gz` con protección contra *zip-slip*.
- Instalador completo: motor → ffmpeg → modelo → configuración → manifest,
  escribiendo el manifest solo al final.
- GUI Tauri + React: asistente de bienvenida, detección, recomendación, progreso
  de instalación por pasos y estado del catálogo.
- Pipeline de backends: scripts de compilación para Linux y Windows, workflow
  `build-backends.yml`, generador de catálogo con hashes reales (probado con
  `node --test`) y firma minisign.
- `docs/RELEASE.md` con el procedimiento completo y las trampas conocidas
  (`$ORIGIN` en el RPATH, repack de ffmpeg, hashes de modelos vía API de HF).
- Verificación real: WAV sintético de 10 s, ejecución de `whisper-cli`, parseo del
  backend que cargó de verdad, RTF y recorrido automático de los fallbacks.
- `verify_installation` y pantalla final que confirma el backend y la velocidad.
- Iconos de la aplicación (SVG + PNG en 32/128/256/512).
- `package-windows.ps1`: ZIP portable con `LEEME.txt`, datos junto al ejecutable y
  soporte opcional de WebView2 *Fixed Runtime* (`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`).
- `release.yml`: ZIP de Windows + `.deb` y `.rpm`, con `SHA256SUMS.txt`.
- Integración con el sistema: accesos directos (Menú Inicio/Escritorio), entrada
  `.desktop` con `%F` y MIME de audio/vídeo, icono, symlink en `~/.local/bin`,
  PATH de usuario y registro "Abrir con" en `HKCU` — todo reversible.
- La generación de los scripts de PowerShell es independiente de la plataforma y
  está cubierta por tests; la integración de Linux se prueba de verdad creando y
  eliminando los archivos en un `$HOME` temporal.
- Comandos `integrate_system` / `unintegrate_system` y botones en Ajustes.
- Studio: arrastrar y soltar, selector de tarea (transcribir / traducir / ambas),
  idioma, modelo, carpeta de salida, formatos, patrón de nombre con vista previa
  real, cola con progreso en vivo, cancelación e historial.
- Núcleo del Studio: planificación de salidas con colisiones, construcción de los
  comandos de whisper.cpp, parseo de progreso (incluye `\r`), historial JSONL y
  ejecución con streaming y cancelación.
- "Abrir con": lectura de argumentos al arrancar y **instancia única** que reenvía
  los archivos a la ventana ya abierta (`open-files`), en lugar de lanzar una
  segunda copia.
- Lotes por carpeta: escaneo recursivo en Rust (`scan_folder`), omisión de carpetas
  ocultas y symlinks, y espejo de la estructura de subcarpetas en la salida.
- Mover datos: copia con verificación SHA-256 archivo por archivo, comprobación de
  espacio libre, rollback completo si algo falla, opción de conservar copia y
  puntero de ubicación fuera del directorio de datos.
- Shell Tauri compilado, enlazado y ejecutado en Arch Linux (Wayland y X11): el
  wizard y el Studio se validaron con hardware real (RX 6650 XT → Vulkan).
- CI en Ubuntu y Windows (fmt, clippy, tests, smoke test de detección), job de
  clippy para el crate Tauri y job de scripts Node.

[No publicado]: https://github.com/TuxArchi/whisper-desktop/compare/main...HEAD
