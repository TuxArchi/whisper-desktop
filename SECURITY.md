# Seguridad

## Reportar una vulnerabilidad

No abras un issue público. Escribe a `security@example.com` con:

- Versión de Whisper Desktop y sistema operativo.
- Descripción del problema y pasos para reproducirlo.
- Impacto estimado.

Respondemos en un máximo de 7 días.

## Modelo de amenaza

Whisper Desktop descarga y ejecuta binarios de terceros (backend de whisper.cpp, ffmpeg,
modelos). Esto es el núcleo de su superficie de ataque, y por eso:

| Control | Implementación |
|---|---|
| Transporte | HTTPS obligatorio, sin excepciones |
| Integridad | SHA-256 verificado **antes** de ejecutar cualquier binario |
| Autenticidad del catálogo | Firma minisign; clave pública embebida en el binario |
| Modelos | Se descargan de HuggingFace pero con hash fijado en el catálogo firmado |
| Privilegios | El ZIP y el `.run` no piden root; los paquetes lo piden vía diálogo gráfico |
| Actualizaciones | Manifiesto `latest.json` firmado (Tauri updater) |

Un catálogo con un hash que no coincida se rechaza y se muestra un error explícito. Nunca
se ejecuta un binario descargado sin verificación.

## Cadena de suministro (CI)

- Actions fijadas por SHA de commit, no por tag.
- `permissions` mínimos por workflow; escritura solo en el job de release.
- Sin secretos disponibles en PRs de forks.
- Hashes de artefactos generados en CI, nunca a mano.

## Alcance

Quedan fuera del alcance los fallos que requieran acceso físico al equipo, ni los que
dependan de un driver de GPU de terceros con vulnerabilidades propias.
