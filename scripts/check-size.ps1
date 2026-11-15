# 体积门禁：超过阈值直接失败，防止体积悄悄涨回去。
#
# 阈值比当前实测值留 8% 余量。指标口径见 README「体积」一节：
# 「exe <= 12 MiB」在不砍功能的前提下不可达，已改为「安装后主程序 <= 23 MiB」
# 与「压缩分发包 <= 10 MiB」。
$ErrorActionPreference = "Stop"

$targets = @(
    @{ Path = "target/release/thzip-spellarchive.exe"; LimitMB = 23.0; Name = "GUI" },
    @{ Path = "target/release/thzip.exe";             LimitMB = 6.0;  Name = "CLI" }
)

$failed = $false
foreach ($item in $targets) {
    if (-not (Test-Path $item.Path)) {
        Write-Host "跳过 $($item.Name)：$($item.Path) 不存在"
        continue
    }
    $sizeMB = (Get-Item $item.Path).Length / 1MB
    $status = if ($sizeMB -le $item.LimitMB) { "OK" } else { "FAIL" }
    Write-Host ("{0,-4} {1,-4} {2,7:N2} MiB / 上限 {3,5:N2} MiB" -f $status, $item.Name, $sizeMB, $item.LimitMB)
    if ($sizeMB -gt $item.LimitMB) {
        $failed = $true
    }
}

if ($failed) {
    throw "体积超过门禁阈值。要么优化，要么在 scripts/check-size.ps1 里显式上调并说明理由。"
}
