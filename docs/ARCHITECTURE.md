# Arquitectura

## Dos modos, un binario

```
WhisperDesktop
   │
   ├─ ¿manifest de instalación válido? ──no──► WIZARD
   │                                          detectar · recomendar · descargar · verificar
   └──────────────────────────────sí─────────► STUDIO
                                              transcribir · traducir · ambas
                                              Ajustes → Reparar / Desinstalar
```

El wizard no es un programa aparte: es un conjunto de rutas del frontend más comandos IPC.
Así solo hay un ejecutable, un icono, un desinstalador y una firma.

## Separación core / GUI

```
crates/wd-core   → detección, decisión, catálogo, i18n, cli
crates/wd-cli    → binario `whisper-desktop` (diagnóstico)
src-tauri        → shell Tauri; traduce IPC a llamadas de wd-core
src/             → React; solo presenta datos y recoge preferencias
```

**Regla:** ninguna decisión de backend o de modelo se escribe en el frontend. El frontend
recibe un `InstallPlan` con `reasons` ya calculados y los traduce por código.

Motivo: el bug más caro de este proyecto sería tener la regla "AMD en Windows nunca usa
ROCm" implementada dos veces con resultados distintos.

## Contrato de datos

`HardwareReport` y `InstallPlan` son `Serialize + Deserialize`. Consecuencias:

- Se pueden guardar en un archivo y reproducir en otra máquina.
- Se pueden usar como fixtures de test (ver `tests/cases/`).
- El diagnóstico que adjunta un usuario en un issue es exactamente lo que el programa vio.
- Si el formato cambia, se sube `schema` y se migra explícitamente.

## Decisión

`crates/wd-core/src/decide/rules.rs` es el único lugar con reglas. Devuelve motivos
traducibles (`Reason { code, args }`), no frases. La traducción vive en `i18n.rs` (CLI) y en
`src/i18n/*.json` (GUI).

## Verificación

El wizard no termina cuando acaba la descarga: ejecuta un clip corto, parsea el log de
`whisper-cli` (`using Vulkan0 backend`) y mide el RTF. Si el backend real no coincide con el
planificado, prueba el siguiente de `plan.fallbacks`.

## Catálogo remoto

La app no contiene URLs de binarios. Descarga `catalog.json` firmado con minisign, lo
verifica con la clave pública embebida y usa sus URLs y hashes. Cambiar de versión de
whisper.cpp no requiere publicar una versión nueva de la app.
