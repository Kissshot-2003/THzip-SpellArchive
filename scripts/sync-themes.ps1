param(
    [string]$Assets = (Join-Path $PSScriptRoot '..\..\assets')
)

# Re-copy the pixel assets from ..\assets into every theme folder.
# theme.toml files are maintained in the repo and are NOT overwritten.
$ErrorActionPreference = 'Stop'
$themesRoot = Join-Path $PSScriptRoot '..\themes'

if (-not (Test-Path $Assets)) {
    throw "Assets folder not found: $Assets"
}

foreach ($skin in @('reimu', 'marisa', 'sanae', 'cirno')) {
    $dst = Join-Path $themesRoot "$skin\png"
    New-Item -ItemType Directory -Path $dst -Force | Out-Null

    Copy-Item (Join-Path $Assets 'icons\*.png') $dst -Force
    Copy-Item (Join-Path $Assets 'progress\progress-base.png') $dst -Force
    Copy-Item (Join-Path $Assets "progress\progress-fill-$skin.png") (Join-Path $dst 'progress-fill.png') -Force
    Copy-Item (Join-Path $Assets "avatars\$skin-256.png") (Join-Path $dst 'avatar.png') -Force
}

Write-Host "Synced PNG assets into $themesRoot"
