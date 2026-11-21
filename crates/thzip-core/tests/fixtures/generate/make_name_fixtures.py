"""生成「同人包友好模式」测试夹具。

产出：
- cp932-names.zip / gbk-names.zip / mixed-names.zip：无 UTF-8 标志位的本地码名字
- split-missing.7z.001 / .002（缺 .003）
- sums/：SHA256SUMS.txt 正例与两条篡改例

手工构造 ZIP（Store 方法）以便写入 CP932 / GBK 原始字节且不置 bit 11；
zip crate 写出的名字总是 UTF-8，无法覆盖这些场景。

用法：python make_name_fixtures.py
"""

import hashlib
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def make_zip(path: Path, entries):
    """entries: [(name_bytes, data_bytes), ...]"""
    local = bytearray()
    central = bytearray()
    offset = 0
    for name, data in entries:
        crc = zlib.crc32(data) & 0xFFFFFFFF
        local += struct.pack(
            "<IHHHHHIIIHH",
            0x04034B50, 20, 0, 0, 0, 0,
            crc, len(data), len(data), len(name), 0,
        )
        local += name + data
        central += struct.pack(
            "<IHHHHHHIIIHHHHHII",
            0x02014B50, 20, 20, 0, 0, 0, 0,
            crc, len(data), len(data), len(name),
            0, 0, 0, 0, 0, offset,
        )
        central += name
        offset += 30 + len(name) + len(data)

    eocd = struct.pack(
        "<IHHHHIIH",
        0x06054B50, 0, 0, len(entries), len(entries),
        len(central), offset, 0,
    )
    path.write_bytes(local + central + eocd)


def main():
    jp_name = "テスト.txt".encode("cp932")
    jp_nested = "フォルダ/ファイル.txt".encode("cp932")
    cn_name = "测试.txt".encode("gbk")
    utf8_name = "utf8-名前.txt".encode("utf-8")

    make_zip(ROOT / "cp932-names.zip", [(jp_name, b"jp"), (jp_nested, b"nested")])
    make_zip(ROOT / "gbk-names.zip", [(cn_name, b"cn")])
    make_zip(ROOT / "mixed-names.zip", [(utf8_name, b"u"), (jp_name, b"s")])

    # 缺卷：.001/.002/.004 存在，.003 缺失（detect 从最小到最大查连续性）。
    for suffix, payload in (
        ("001", b"x" * 16),
        ("002", b"y" * 16),
        ("004", b"z" * 16),
    ):
        (ROOT / f"split-missing.7z.{suffix}").write_bytes(payload)

    # 校验和样例：与 cp932-names.zip 内的两个条目对应。
    sums = ROOT / "sums"
    sums.mkdir(exist_ok=True)
    hash_jp = hashlib.sha256(b"jp").hexdigest()
    hash_nested = hashlib.sha256(b"nested").hexdigest()
    good = f"{hash_jp}  テスト.txt\n{hash_nested}  フォルダ/ファイル.txt\n"
    (sums / "SHA256SUMS.txt").write_text(good, encoding="utf-8")
    (sums / "tampered-mismatch.txt").write_text(
        good.replace(hash_jp, "0" * 64), encoding="utf-8"
    )
    (sums / "tampered-extra.txt").write_text(
        good + f"{hash_jp}  多出のファイル.txt\n", encoding="utf-8"
    )
    print(f"fixtures written to {ROOT}")


if __name__ == "__main__":
    main()
