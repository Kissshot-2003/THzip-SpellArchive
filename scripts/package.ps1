param(
    [string]$Version = "0.2.0"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $root
try {
    cargo build -p thzip-app --release
    if ($LASTEXITCODE -ne 0) {
        throw "release build failed"
    }

    $stage = Join-Path $root "dist\THzip-SpellArchive"
    if (Test-Path $stage) {
        Remove-Item -Recurse -Force $stage
    }
    New-Item -ItemType Directory -Path $stage -Force | Out-Null

    Copy-Item (Join-Path $root "target\release\thzip-spellarchive.exe") $stage
    Copy-Item (Join-Path $root "themes") $stage -Recurse
    Copy-Item (Join-Path $root "README.md") $stage
    Copy-Item (Join-Path $root "LICENSE") $stage
    Copy-Item (Join-Path $root "NOTICE.md") $stage
    Copy-Item (Join-Path $root "third_party") $stage -Recurse

    Write-Host "Staged: $stage"
    Write-Host "Version: $Version"

    $iscc = Get-Command iscc -ErrorAction SilentlyContinue
    if ($null -ne $iscc) {
        & $iscc.Source (Join-Path $root "packaging\THzip-SpellArchive.iss")
        if ($LASTEXITCODE -ne 0) {
            throw "Inno Setup failed"
        }
        Write-Host "Installer written to dist/THzip-SpellArchive-$Version-setup.exe"
    } else {
        Write-Host "Inno Setup (iscc) not found; only the portable folder was staged."
    }
} finally {
    Pop-Location
}
