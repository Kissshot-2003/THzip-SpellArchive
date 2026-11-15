param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $root
try {
    $cargoToml = Get-Content (Join-Path $root "Cargo.toml") -Raw
    $match = [regex]::Match($cargoToml, '(?m)^\s*version\s*=\s*"([^"]+)"')
    if (-not $match.Success) {
        throw "cannot read workspace version from Cargo.toml"
    }
    $cargoVersion = $match.Groups[1].Value

    if ([string]::IsNullOrWhiteSpace($Version)) {
        $Version = $cargoVersion
    } elseif ($Version -ne $cargoVersion) {
        throw "version mismatch: -Version $Version but Cargo.toml is $cargoVersion"
    }

    Write-Host "Building THzip-SpellArchive $Version ..."
    # PowerShell 5.1 会把原生命令写往 stderr 的内容当成错误记录，而 cargo 的编译进度全在
    # stderr 上，配合 $ErrorActionPreference = "Stop" 会让脚本在编译中途直接抛出。
    # 这里临时放宽，靠 $LASTEXITCODE 判断成败。
    $savedEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    cargo build -p thzip-app -p thzip-cli --release
    $buildExit = $LASTEXITCODE
    $ErrorActionPreference = $savedEap
    if ($buildExit -ne 0) {
        throw "release build failed"
    }

    $stage = Join-Path $root "dist\THzip-SpellArchive"
    if (Test-Path $stage) {
        Remove-Item -Recurse -Force $stage
    }
    New-Item -ItemType Directory -Path $stage -Force | Out-Null

    Copy-Item (Join-Path $root "target\release\thzip-spellarchive.exe") $stage
    Copy-Item (Join-Path $root "target\release\thzip.exe") $stage
    Copy-Item (Join-Path $root "themes") $stage -Recurse
    Copy-Item (Join-Path $root "README.md") $stage
    Copy-Item (Join-Path $root "CHANGELOG.md") $stage
    Copy-Item (Join-Path $root "LICENSE") $stage
    Copy-Item (Join-Path $root "NOTICE.md") $stage
    Copy-Item (Join-Path $root "third_party") $stage -Recurse

    $built = (Get-Item (Join-Path $stage "thzip-spellarchive.exe")).VersionInfo.FileVersion
    if ($built -ne $Version) {
        throw "built exe version $built does not match $Version"
    }

    $sums = foreach ($file in Get-ChildItem $stage -Recurse -File | Sort-Object FullName) {
        $relative = $file.FullName.Substring($stage.Length + 1).Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $relative"
    }
    $sumsPath = Join-Path $stage "SHA256SUMS.txt"
    [IO.File]::WriteAllLines($sumsPath, $sums, (New-Object System.Text.UTF8Encoding($false)))

    Write-Host "Staged: $stage"
    Write-Host "Version: $Version"

    $isccCommand = Get-Command iscc -ErrorAction SilentlyContinue
    $iscc = if ($null -ne $isccCommand) { $isccCommand.Source } else { $null }
    if ([string]::IsNullOrEmpty($iscc)) {
        $candidates = @(
            (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
            (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe"),
            (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe")
        )
        foreach ($candidate in $candidates) {
            if (Test-Path -LiteralPath $candidate) {
                $iscc = $candidate
                break
            }
        }
    }
    if (-not [string]::IsNullOrEmpty($iscc)) {
        & $iscc "/DMyAppVersion=$Version" (Join-Path $root "packaging\THzip-SpellArchive.iss")
        if ($LASTEXITCODE -ne 0) {
            throw "Inno Setup failed"
        }
        $installer = Join-Path $root "dist\THzip-SpellArchive-$Version-setup.exe"
        $installerHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Host "Installer written to dist/THzip-SpellArchive-$Version-setup.exe"
        Write-Host "Installer SHA-256: $installerHash"
    } else {
        Write-Host "Inno Setup (iscc) not found; only the portable folder was staged."
    }
} finally {
    Pop-Location
}
