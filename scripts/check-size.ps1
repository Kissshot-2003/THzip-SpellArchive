# 体积门禁：超过阈值直接失败，防止体积悄悄涨回去。
#
# 阈值比当前实测值留 8% 余量。指标口径见 README「体积」一节：
# 「exe <= 12 MiB」在不砍功能的前提下不可达，已改为「安装后主程序 <= 27 MiB」
# 与「CLI <= 8 MiB」。
#
# 2026-11 上调过一次（原 GUI 23 / CLI 6 MiB）：RAR 后端从 unrar-ng（UnRAR C++）
# 换成 unrar-rs（纯 Rust），换来条目流式解码——内存占用与条目大小无关，
# 不再有 16 MiB 硬上限与临时文件中转。代价是 +3.8 MiB（GUI）/ +2.4 MiB（CLI）：
# unrar-rs 拖进了 RAR5 的纠错（reedsolomon）、并行解码（rayon）与 CRC 加速
# （crc-turbo），都是真实能力，不打算砍。详见 docs/legal/unrar-note.md。
$ErrorActionPreference = "Stop"

$targets = @(
    @{ Path = "target/release/thzip-spellarchive.exe"; LimitMB = 27.0; Name = "GUI" },
    @{ Path = "target/release/thzip.exe";             LimitMB = 8.0;  Name = "CLI" }
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
