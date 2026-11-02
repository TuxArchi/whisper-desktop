# Terceros

Whisper Desktop se apoya en software de terceros. Este documento recoge sus licencias y
obligaciones, que se distribuyen junto al binario y se muestran en *Ajustes → Acerca de*.

## whisper.cpp

- **Licencia:** MIT
- **Uso:** motor de inferencia (CPU, CUDA, Vulkan, ROCm)
- **Obligación:** incluir el aviso de copyright y el texto de la licencia.
- **Origen:** <https://github.com/ggml-org/whisper.cpp>

## OpenAI Whisper (modelos)

- **Licencia:** MIT
- **Uso:** pesos de los modelos GGML (tiny … large-v3).
- **Obligación:** atribución.
- **Origen:** <https://github.com/openai/whisper> · <https://huggingface.co/ggml-org/whisper.cpp>

## ffmpeg

- **Licencia:** LGPL 2.1 o posterior (**build sin componentes GPL**: sin x264/x265).
- **Uso:** decodificación y normalización a 16 kHz mono PCM.
- **Obligaciones:**
  1. Incluir el texto de la LGPL (`packaging/licenses/LGPL-ffmpeg.txt`).
  2. Incluir `FFMPEG-SOURCE.txt` con la URL del código fuente exacto de la versión usada.
  3. Permitir la sustitución del binario (se distribuye como ejecutable separado, no
     enlazado estáticamente a nuestra aplicación).
- **Origen:** <https://ffmpeg.org/> · builds: <https://github.com/BtbN/FFmpeg-Builds>

## Microsoft WebView2 Runtime (solo Windows)

- **Licencia:** términos de distribución de WebView2 (redistribuible en modo *Fixed Version*).
- **Uso:** motor de la interfaz.
- **Obligación:** incluir el aviso de Microsoft que acompaña al paquete del runtime.

## Librerías Rust y JavaScript

Las dependencias y sus licencias se pueden listar con:

```bash
cargo deny list
pnpm licenses list
```

Se revisan en CI con `cargo deny` y `pnpm audit`.
