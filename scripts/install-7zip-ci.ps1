param(
    [string]$InstallDir = (Join-Path $env:ProgramFiles "7-Zip")
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$url = "https://www.7-zip.org/a/7z2604-x64.msi"
$expectedSha256 = "0b01334a418654293513449f61d6bfc99e5196ca371e3c3dc961eda57cd535c6"
$exe = Join-Path $InstallDir "7z.exe"

if (-not (Test-Path -LiteralPath $exe)) {
    $msi = Join-Path $env:TEMP "7z2604-x64.msi"
    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $msi -TimeoutSec 120
    $actual = (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expectedSha256) {
        throw "7-Zip MSI checksum mismatch: expected $expectedSha256, got $actual"
    }
    $process = Start-Process msiexec.exe -ArgumentList @("/i", "`"$msi`"", "/qn", "/norestart") -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "7-Zip MSI install failed with exit code $($process.ExitCode)"
    }
}

if (-not (Test-Path -LiteralPath $exe)) {
    throw "7-Zip was installed but $exe is missing"
}

Write-Output $exe
