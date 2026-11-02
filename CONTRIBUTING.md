# Contribuir

Gracias por ayudar. Este proyecto tiene una regla principal: **las decisiones viven en un
solo lugar**.

## Empezar

```bash
git clone https://github.com/TuxArchi/whisper-desktop
cd whisper-desktop
cargo test --workspace
cargo run -p wd-cli -- --detect-only
```

Si no tienes GPU o estás en una VM, todo funciona igual: la lógica se prueba con fixtures.

## Dónde tocar cada cosa

| Quiero… | Archivo |
|---|---|
| Añadir soporte para una GPU | `crates/wd-core/src/detect/pci.rs` + un caso en `tests/cases/` |
| Cambiar cuándo se usa ROCm | `crates/wd-core/src/decide/rules.rs` |
| Cambiar el modelo recomendado | `crates/wd-core/src/decide/rules.rs` y `docs/HARDWARE-MATRIX.md` |
| Añadir un texto nuevo | `crates/wd-core/src/i18n.rs` y `src/i18n/*.json` |
| Añadir un backend o modelo | `catalog/catalog.json` |

**Nunca** dupliques una regla en el frontend. Si la UI necesita decidir algo, se añade al
`InstallPlan` y se consume.

## Pull requests

1. Un cambio, un PR.
2. `cargo fmt --all` y `cargo clippy --workspace --all-targets -- -D warnings` deben pasar.
3. Si tocas reglas de decisión, añade o actualiza un caso en `tests/cases/`.
4. Si tocas la detección, adjunta el `hardware.json` de la máquina donde lo probaste:

```bash
whisper-desktop --detect-only --json > hardware.json
```

## Estilo

- Rust 2021, sin `unwrap()` en rutas de producción (sí en tests).
- Los `Reason` usan códigos, nunca frases en español dentro del core.
- Comentarios en inglés en el código; documentación de usuario en español por ahora.

## Licencia

Al enviar un PR aceptas que tu contribución se publique bajo MIT.
