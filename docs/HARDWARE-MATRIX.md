# Matriz de hardware

Cómo se decide el backend y el modelo. Esta tabla es la fuente de verdad; el código en
`crates/wd-core/src/decide/rules.rs` la implementa.

## Backend por GPU

| SO | GPU | Backend | Fallback |
|---|---|---|---|
| Windows | NVIDIA, driver ≥ 500 | `cuda` | Vulkan → CPU |
| Windows | AMD RDNA4 (gfx1200/1201) | `rocm` | Vulkan → CPU |
| Windows | AMD Ryzen AI (gfx1150/1151) | `rocm` | Vulkan → CPU |
| Windows | AMD RDNA1–3, Vega, GCN | `vulkan` | CPU |
| Windows | Intel Arc / iGPU | `vulkan` | CPU |
| Linux | NVIDIA | `cuda` | Vulkan → CPU |
| Linux | AMD RDNA3/4, CDNA con `/dev/kfd` | `rocm` | Vulkan → CPU |
| Linux | AMD RDNA1–2, Vega | `vulkan` | CPU |
| Linux | Intel | `vulkan` | CPU |
| cualquiera | sin GPU | `cpu` | — |

### Por qué AMD en Windows casi nunca usa ROCm

La matriz oficial de AMD para Windows cubre únicamente `gfx1100/1101/1200/1201` y
`gfx1150/1151`, y solo Windows 11. Las RX 6000 (`gfx103x`) no están soportadas en Windows
(ver [`AMD.md`](AMD.md)). Ofrecer ROCm fuera de esa lista produce instalaciones que fallan
en el primer arranque.

Vulkan funciona en *todas* las GPU AMD con driver reciente y rinde muy bien en Whisper
(casos reportados de 7,5–8× tiempo real en RX 9070 XT con modelos grandes).

## Modelo por hardware

| VRAM / RAM | Modelo | Variante |
|---|---|---|
| CPU, < 16 GB RAM | `base` | `q5_0` |
| CPU, ≥ 16 GB RAM + AVX2 | `small` | `q5_0` |
| GPU < 4 GB | `base` | `q5_0` |
| GPU 4–6 GB | `small` | `fp16` |
| GPU 6–8 GB | `large-v3-turbo` | `q5_0` |
| GPU 8–12 GB | `large-v3-turbo` | `fp16` |
| GPU 12–16 GB | `large-v3` | `q5_0` |
| GPU ≥ 16 GB | `large-v3` | `fp16` |

## Cómo añadir una GPU

1. Añade un caso en `tests/cases/` con el `HardwareReport` de la máquina y el `expect`.
2. Si la familia no se detecta bien, ajusta `family_from_name` en
   `crates/wd-core/src/detect/pci.rs` y añade un test unitario.
3. `cargo test -p wd-core`.

No añadas una tabla gigante de ids PCI: se desactualiza y produce decisiones silenciosamente
incorrectas. Las familias son estables; los ids no.
