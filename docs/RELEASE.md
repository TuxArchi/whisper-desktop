# Publicar binarios (release de backends)

Los binarios del motor se publican **por separado** de la aplicación. Así se
puede actualizar whisper.cpp o ffmpeg sin que nadie tenga que descargar una
versión nueva de Whisper Desktop.

## Cómo funciona

```
build-backends.yml
  ├─ linux-cpu / linux-vulkan / linux-cuda      → dist/*.tar.gz
  ├─ windows-cpu / windows-vulkan / windows-cuda → dist/*.zip
  ├─ ffmpeg-lgpl (repack de BtbN, solo el binario)
  └─ catalog:
       ├─ make-catalog.mjs   (hashes reales, valida como la app)
       ├─ trust.mjs sign     (Ed25519 sobre catalog.json)
       ├─ gh release create  (tag backends-rX)
       └─ docs/catalog.json  (copia para GitHub Pages, URL estable)
```

`scripts/make-catalog.mjs` **hashea los archivos él mismo**. Nunca se escriben
hashes a mano: es el error que rompe instalaciones en silencio.

## Preparación (una vez)

### 1. Claves de firma

Se usa **Ed25519** con `node:crypto`: no hace falta instalar nada.

```bash
node scripts/trust.mjs keygen --out-dir ./keys
```

- `keys/catalog.pub` es **pública**: va en `crates/wd-core/src/trust.rs`
  (`CATALOG_PUBLIC_KEY`) y en el workflow.
- `keys/catalog.key` es **privada**: nunca se sube al repo.

```bash
base64 -w0 keys/catalog.key | gh secret set CATALOG_SIGNING_KEY
```

Cambiar de clave obliga a publicar una versión nueva de la app: la clave va
compilada dentro y es lo que impide que nadie sustituya los binarios.

### 2. Hashes de los modelos

Los modelos se descargan de HuggingFace, pero el hash va fijado en el catálogo.
El campo `lfs.oid` que expone la API **es** el SHA-256 del archivo, así que no
hace falta descargar 1,6 GB:

```bash
curl -s https://huggingface.co/api/models/ggml-org/whisper.cpp/tree/main \
  | jq -r '.[] | select(.lfs != null) | "\(.path)\t\(.lfs.oid)\t\(.lfs.size)"'
```

Copia el resultado en `scripts/models.json` (formato en `models.example.json`).

## Publicar

```bash
gh workflow run build-backends.yml -f whisper_version=1.8.4 -f channel=stable
```

O creando un tag:

```bash
git tag backends-r1 && git push origin backends-r1
```

Al terminar, el release contiene:

```
whisper-1.8.4-{linux,windows}-x64-{cpu,vulkan,cuda,rocm}.*
ffmpeg-lgpl-{win64,linux64}.*
catalog.json
catalog.json.minisig
```

## Verificar antes de anunciarlo

```bash
# 1. El catálogo es aceptable para la app
node scripts/make-catalog.mjs --artifacts dist --repo TuxArchi/whisper-desktop \
  --release backends-r1 --out /tmp/check.json

# 2. La firma es válida
node scripts/trust.mjs verify --pub "$(cat keys/catalog.pub)" \
  --in dist/catalog.json --sig dist/catalog.json.sig

# 3. Los binarios arrancan (en una máquina de cada tipo)
./whisper-cli --help
./whisper-cli -m ggml-tiny.bin -f prueba.wav
```

## Notas de compilación

| Backend | Flags | Notas |
|---|---|---|
| cpu | — | Portable, funciona en cualquier equipo |
| vulkan | `-DGGML_VULKAN=ON` | Requiere el SDK solo al **compilar**; en runtime usa el driver del usuario |
| cuda | `-DGGML_CUDA=ON` | Windows incluye las DLL de cuBLAS; el ZIP pesa ~400 MB |
| rocm | `-DGGML_HIP=ON -DAMDGPU_TARGETS=...` | Compilación pesada; se ejecuta a mano o en un contenedor ROCm |

`-DCMAKE_INSTALL_RPATH='$ORIGIN'` es obligatorio en Linux: sin él, el binario
busca las `.so` en el sistema en lugar de junto a sí mismo y falla en equipos
limpios.

## Si algo falla

| Síntoma | Causa habitual |
|---|---|
| `falta el artefacto` | Un job de build no subió su archivo o el nombre no sigue el contrato de `backends.manifest.json` |
| El catálogo no valida | Un archivo quedó vacío o la URL base no es HTTPS |
| La app rechaza el catálogo | La firma no corresponde a la clave pública embebida |
| `whisper-cli` no arranca en Linux | Falta `$ORIGIN` en el RPATH o faltó copiar `build/bin/.` completo |
