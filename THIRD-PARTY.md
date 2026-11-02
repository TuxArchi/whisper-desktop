# Terceros

Whisper Desktop se apoya en software de terceros. Este documento explica qué
distribuimos, bajo qué licencia y qué obligaciones cumple cada pieza. Todos los
avisos viajan con la aplicación y están accesibles desde *Ajustes → Acerca de*.

> **Proyecto independiente.** No está afiliado a OpenAI, a AMD, a NVIDIA ni a la
> aplicación WhisperDesktop de Const-me. «Whisper» es el nombre del modelo de
> OpenAI y se usa aquí de forma descriptiva.

## Resumen

| Componente | Licencia | ¿Lo distribuimos? | Obligación |
|---|---|---|---|
| Nuestro código | MIT | Sí (binario y fuentes) | Aviso de copyright |
| whisper.cpp | MIT | Sí (motor, dentro de cada backend) | Incluir el texto |
| Modelos GGML | MIT | **No**: se descargan de HuggingFace | Atribución |
| ffmpeg | LGPL 2.1+ (build con texto v3) | Sí (ejecutable aparte) | Texto de licencia **y** oferta de fuente |
| webkit2gtk | LGPL | **No**: dependencia del sistema | Ninguna por nuestra parte |
| WebView2 | Términos de Microsoft | No (se usa el runtime del sistema) | Aviso si algún día se empaqueta |
| Dependencias Rust y npm | MIT / Apache-2.0 / BSD / ISC | Sí (dentro del binario) | Ver `NOTICES.md` |

## whisper.cpp

- **Licencia:** MIT — Copyright (c) 2023-2026 The ggml authors.
- **Uso:** motor de inferencia (CPU, CUDA, Vulkan, ROCm).
- **Dónde está el aviso:** `LICENSE.whisper.cpp` dentro de cada backend
  descargado, y `packaging/licenses/MIT-whisper.cpp.txt` en el repositorio.
- **Origen:** <https://github.com/ggml-org/whisper.cpp>

## Modelos (OpenAI Whisper)

- **Licencia:** MIT.
- **Uso:** pesos GGML (`tiny` … `large-v3`).
- **No los redistribuimos:** la aplicación los descarga desde HuggingFace con el
  hash SHA-256 fijado en el catálogo firmado.
- **Origen:** <https://github.com/openai/whisper> ·
  <https://huggingface.co/ggerganov/whisper.cpp>

## ffmpeg

- **Licencia:** LGPL 2.1 o posterior. La compilación que distribuimos incluye el
  texto de la **versión 3**, que es el que hay que reproducir.
- **Uso:** decodificación y normalización de audio a 16 kHz mono.
- **Obligaciones y cómo se cumplen:**
  1. Texto de la licencia → `packaging/licenses/LGPL-3.0-ffmpeg.txt`, también
     junto al binario en `bin/` tras la instalación.
  2. Código fuente de esa compilación exacta → `packaging/licenses/FFMPEG-SOURCE.txt`.
  3. Posibilidad de sustituirlo → se distribuye como **ejecutable independiente**,
     no enlazado con la aplicación (mera agregación).
- **Build:** BtbN `FFmpeg-Builds`, variante **LGPL** (sin componentes GPL como
  x264 o x265).
- **Origen:** <https://ffmpeg.org/> · <https://github.com/BtbN/FFmpeg-Builds>

## Dependencias de compilación

Las ~110 dependencias de Rust y npm, con su licencia y el texto completo, están
en [`NOTICES.md`](NOTICES.md). Se regenera automáticamente con:

```bash
node scripts/make-notices.mjs
```

La mayoría son MIT o Apache-2.0 de doble licencia; se reproducen los textos una
sola vez por licencia.

## Marcas

«OpenAI» y «Whisper» son marcas de OpenAI. «NVIDIA», «CUDA», «AMD», «Radeon»,
«Intel» y «Vulkan» pertenecen a sus respectivos titulares. Este proyecto los
menciona únicamente para describir compatibilidad.
