param(
    # 待签名的文件。省略则签 dist 目录里的全部 exe 与安装包。
    [string[]]$Path = @(),
    # 证书文件（.pfx）。也可用环境变量 THZIP_SIGN_PFX 传入。
    [string]$Pfx = $env:THZIP_SIGN_PFX,
    # 证书密码。也可用环境变量 THZIP_SIGN_PASSWORD 传入。
    [string]$Password = $env:THZIP_SIGN_PASSWORD,
    # RFC 3161 时间戳服务器。留空则不签名（自签名证书无戳会失去时间戳价值）。
    [string]$Timestamp = "http://timestamp.digicert.com",
    # 证书指纹。给了就走 Windows 证书存储，不用 PFX 文件。
    [string]$Thumbprint = $env:THZIP_SIGN_THUMBPRINT
)

# 代码签名：没有证书就无法签，这是唯一真正的外部阻塞。
# 本脚本把「有证书」和「没证书」两条路都铺好：
#   - 本地：./scripts/sign.ps1 -Pfx D:\cert.pfx -Password xxx
#   - CI ：把 PFX + 密码存进 GitHub Secrets，由 release.yml 调用
#   - 都没证书时脚本明确报告并以非零码退出，调用方据此决定跳过还是中止。

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $root
try {
    $signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
    $signtoolPath = if ($null -ne $signtool) { $signtool.Source } else { $null }
    if ([string]::IsNullOrEmpty($signtoolPath)) {
        $candidates = @(
            (Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe"),
            (Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin\10.0.22621.0\x64\signtool.exe"),
            (Join-Path $env:ProgramFiles "Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe")
        )
        foreach ($candidate in $candidates) {
            if (Test-Path -LiteralPath $candidate) {
                $signtoolPath = $candidate
                break
            }
        }
    }
    if ([string]::IsNullOrEmpty($signtoolPath)) {
        throw "signtool.exe not found. Install the Windows SDK, or run this on windows-latest."
    }
    Write-Host "signtool: $signtoolPath"

    # 签名方式二选一：证书存储里的指纹，或 PFX 文件。
    $useThumbprint = -not [string]::IsNullOrWhiteSpace($Thumbprint)
    $usePfx = -not [string]::IsNullOrWhiteSpace($Pfx)
    if (-not $useThumbprint -and -not $usePfx) {
        throw "no certificate: set -Pfx (or THZIP_SIGN_PFX), or -Thumbprint (or THZIP_SIGN_THUMBPRINT)."
    }
    if ($useThumbprint -and $usePfx) {
        throw "give either a thumbprint or a PFX, not both."
    }

    # 收集待签名文件。
    if ($Path.Count -eq 0) {
        $stage = Join-Path $root "dist\THzip-SpellArchive"
        $candidates = @()
        if (Test-Path $stage) {
            $candidates += Get-ChildItem $stage -Filter *.exe -File
        }
        $candidates += Get-ChildItem (Join-Path $root "dist") -Filter *.exe -File -ErrorAction SilentlyContinue
        if ($candidates.Count -eq 0) {
            throw "nothing to sign: pass -Path explicitly, or build dist/ first."
        }
        $Path = $candidates | Select-Object -ExpandProperty FullName
    }

    $signed = 0
    foreach ($item in $Path) {
        if (-not (Test-Path -LiteralPath $item)) {
            Write-Warning "skip missing file: $item"
            continue
        }
        $signArgs = @("sign", "/fd", "SHA256", "/td", "SHA256", "/tr", $Timestamp, "/v")
        if ($useThumbprint) {
            $signArgs += @("/sha1", $Thumbprint)
        } else {
            $signArgs += @("/f", $Pfx)
            if (-not [string]::IsNullOrEmpty($Password)) {
                $signArgs += @("/p", $Password)
            }
        }
        $signArgs += $item

        Write-Host "Signing $item ..."
        # 同上：signtool 成功时也会往 stderr 写摘要，临时放宽错误策略。
        $savedEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        & $signtoolPath @signArgs
        $signExit = $LASTEXITCODE
        $ErrorActionPreference = $savedEap
        if ($signExit -ne 0) {
            throw "signtool failed for $item (exit $signExit)"
        }
        $signed++
    }

    # 签完立刻验一遍：签名失败但脚本没报错的情况，靠 verify 兜住。
    #
    # `/pa` 用的是「默认策略」，只认系统信任链里的证书。自签名证书（本地演练、
    # 内部分发）会在这里报「not trusted」——这是预期行为，不是签名失败。
    #
    # PowerShell 5.1 会把原生命令写往 stderr 的内容当成错误记录，配合
    # $ErrorActionPreference = "Stop" 会让脚本在 verify 阶段直接抛出。
    # 这里临时放宽，靠 $LASTEXITCODE 判断成败。
    $savedEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    foreach ($item in $Path) {
        if (-not (Test-Path -LiteralPath $item)) { continue }
        $verifyOutput = & $signtoolPath "verify" "/pa" "/v" $item 2>&1
        $verifyExit = $LASTEXITCODE
        if ($verifyExit -eq 0) {
            Write-Host "  verified (trusted): $item"
            continue
        }
        $text = ($verifyOutput | Out-String)
        if ($text -match "not trusted by the trust provider" -or $text -match "terminated in a root") {
            # 签名在，但证书不在系统信任链里。自签名证书属于这种情况。
            Write-Warning "  signed but NOT trusted by the system: $item"
            Write-Warning "  Expected for self-signed certificates. Public releases need a CA-issued certificate."
        } else {
            $ErrorActionPreference = $savedEap
            throw "signature verification failed for $item`n$text"
        }
    }
    $ErrorActionPreference = $savedEap

    Write-Host "Signed $signed file(s)."
} finally {
    Pop-Location
}
