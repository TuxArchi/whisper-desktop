# AMD y ROCm

Este documento explica por qué Whisper Desktop **no instala PyTorch** y por qué ROCm se
ofrece solo en casos concretos.

## El problema

Durante años, usar Whisper con una GPU AMD en Python ha requerido:

1. Instalar el stack ROCm completo (`amdgpu-install`, 5–15 GB).
2. Instalar una build de PyTorch compilada contra esa versión exacta de ROCm.
3. Parchear archivos de código porque la GPU no está en la lista oficial.
4. Exportar `HSA_OVERRIDE_GFX_VERSION` para hacer pasar una arquitectura no soportada por
   otra compatible (p. ej. `gfx1031` → `10.3.0`).
5. Rezar para que la actualización del driver no rompa todo.

Cada paso puede fallar por separado, y los errores son opacos (`HSA_STATUS_ERROR`).

## Lo que hacemos

**whisper.cpp no usa PyTorch.** Los binarios ROCm incorporan el runtime necesario, así que
no hace falta instalar ROCm en el sistema ni tocar ningún archivo.

| Escenario | Solución |
|---|---|
| AMD en Windows, cualquier generación | Binario **Vulkan**. Cero configuración |
| AMD RDNA4 / Ryzen AI en Windows | Binario **ROCm** (runtime embebido) |
| AMD RDNA3/4 en Linux | Binario **ROCm** si `/dev/kfd` es accesible |
| AMD RDNA1/2, Vega, GCN en Linux | **Vulkan** |
| Cualquier caso dudoso | **Vulkan**, y la verificación confirma si funciona |

## Permisos en Linux

ROCm necesita acceso a `/dev/kfd` y `/dev/dri/renderD*`. Si el usuario no está en los
grupos `render` y `video`, el planificador cae a Vulkan y muestra:

```
ROCm no accesible (/dev/kfd). Añade tu usuario a render y video, y reinicia sesión
```

```bash
sudo usermod -aG render,video "$USER"
```

## Modo avanzado (futuro)

Si en el futuro se añade un módulo Python opcional, la tabla de compatibilidad forzada será:

| gfx detectado | `HSA_OVERRIDE_GFX_VERSION` |
|---|---|
| gfx1030 / 1031 / 1032 (RDNA2) | `10.3.0` |
| gfx1010 / 1011 / 1012 (RDNA1) | `10.1.0` |
| gfx1102 (RX 7600) | `11.0.0` |
| gfx1150 / 1151 (Ryzen AI) | nativo |

Consulta la matriz oficial antes de prometer soporte:
<https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/compatibility/compatibilityrad/windows/windows_compatibility.html>
