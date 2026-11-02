# Builds one whisper.cpp backend on Windows and packs it as a release artifact.
#
# Usage: build-backend.ps1 <cpu|vulkan|cuda> <version> <outdir>
# Run from inside the whisper.cpp checkout.

param(
    [Parameter(Mandatory = $true)][string]$Backend,
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Out = "dist"
)

$ErrorActionPreference = "Stop"

$flags = switch ($Backend) {
    "cpu"    { @() }
    "vulkan" { @("-DGGML_VULKAN=ON") }
    "cuda"   { @("-DGGML_CUDA=ON", "-DCMAKE_CUDA_ARCHITECTURES=61;75;86;89") }
    default  { throw "backend desconocido: $Backend" }
}

Write-Host "== whisper.cpp $Version · backend $Backend =="

cmake -B build -G "Visual Studio 17 2022" -A x64 `
    -DBUILD_SHARED_LIBS=ON `
    -DWHISPER_BUILD_TESTS=OFF `
    -DWHISPER_BUILD_EXAMPLES=ON `
    @flags
if ($LASTEXITCODE -ne 0) { throw "cmake configure falló" }

cmake --build build --config Release -j
if ($LASTEXITCODE -ne 0) { throw "cmake build falló" }

$stage = Join-Path $Out "stage-$Backend"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path $stage -Force | Out-Null

# MSVC drops everything in build\bin\Release: the CLI, the benchmark and the
# DLLs it needs (ggml*.dll, whisper.dll, and for CUDA the cublas DLLs).
Copy-Item -Path "build\bin\Release\*" -Destination $stage -Recurse -Force

Copy-Item "LICENSE" (Join-Path $stage "LICENSE.whisper.cpp")
if (Test-Path "README.md") {
    Copy-Item "README.md" (Join-Path $stage "README.whisper.cpp.md")
}

$name = "whisper-$Version-windows-x64-$Backend"
New-Item -ItemType Directory -Path $Out -Force | Out-Null
$zip = Join-Path $Out "$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal
Remove-Item -Recurse -Force $stage

Write-Host "listo: $zip"
