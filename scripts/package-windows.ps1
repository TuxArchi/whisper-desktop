# Builds the portable Windows ZIP.
#
# Usage: package-windows.ps1 [-WebView2Runtime <dir>]
#
# The ZIP is fully self-contained: launching WhisperDesktop.exe from any
# folder works, and all user data stays inside that folder.

param(
    [string]$WebView2Runtime = "",
    [string]$Out = "release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "== Compilando la aplicación =="
Push-Location $root
npm run build
if ($LASTEXITCODE -ne 0) { throw "falló el build del frontend" }

# --no-bundle: the portable ZIP replaces NSIS entirely.
npm run tauri build -- --no-bundle
if ($LASTEXITCODE -ne 0) { throw "falló el build de Tauri" }
Pop-Location

$exe = Join-Path $root "src-tauri\target\release\whisper-desktop.exe"
if (-not (Test-Path $exe)) { throw "no se encontró el ejecutable: $exe" }

$version = (Get-Content (Join-Path $root "package.json") | ConvertFrom-Json).version
$stage = Join-Path $root "$Out\WhisperDesktop-$version-x64"

if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path $stage -Force | Out-Null

# The app resolves everything from its own folder, so the name is cosmetic.
Copy-Item $exe (Join-Path $stage "WhisperDesktop.exe")

if ($WebView2Runtime -and (Test-Path $WebView2Runtime)) {
    Write-Host "== Incluyendo WebView2 Fixed Runtime =="
    Copy-Item $WebView2Runtime (Join-Path $stage "webview2") -Recurse -Force
} else {
    Write-Warning "Sin WebView2 embebido: se usará el runtime del sistema."
}

Copy-Item (Join-Path $root "packaging\LEEME.txt") $stage
Copy-Item (Join-Path $root "LICENSE") $stage
Copy-Item (Join-Path $root "THIRD-PARTY.md") $stage

New-Item -ItemType Directory -Path (Join-Path $stage "models") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $stage "data") -Force | Out-Null

New-Item -ItemType Directory -Path (Join-Path $root $Out) -Force | Out-Null
$zip = Join-Path $root "$Out\WhisperDesktop-$version-x64.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal

Write-Host "listo: $zip"
Get-Item $zip | Select-Object Name, @{n = "MB"; e = { [math]::Round($_.Length / 1MB, 1) } }
