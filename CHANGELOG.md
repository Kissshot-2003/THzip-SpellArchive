# 更新日志

本文件格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- CI：stable 测试 / clippy / fmt，tag 触发可复现打包并上传 Release。
- 差分测试：与固定 SHA-256 的 7-Zip 26.04 双向比对列出、解压与 AES 密码。
- fuzz：4 个 cargo-fuzz 目标 + stable 确定性变异 smoke 测试。
- criterion 基准：格式嗅探、10k 条目 zip 列表、16 MiB 解压、4 MiB 7z 打包。
- 便携模式：exe 同目录存在 `portable.flag` 或 `themes/` 时，配置、日志、handoff 写入 `<exe>\config\`，首次启动从 `%APPDATA%` 迁移。
- CLI `--json`：`sniff` / `list` / `test` / `extract` / `pack` 输出单行 JSON。

### 变更

- 最低 Rust 版本提升到 1.93。
- `sevenz-rust2` 升级到 0.24.2。

### 修复

- 7z 目录曾被写成 K_ANTI 反条目；升级后目录正确，并写入 Windows 属性（目录 0x10 / 文件 0x20，只读追加 0x01）。

## [0.2.0] - 2026-11-15

### 新增

- zip 读写：Store / Deflate / Deflate64 / BZip2 / Zstd / PPMd / LZMA / XZ 读取，Store / Deflate / BZip2 / Zstd 写入，AES-256，zip64。
- 7z 读写：固实包、AES-256 头加密、LZMA2 级别与字典可调、`.7z.001` 分卷读取。
- tar 家族与单文件流：tar / gz / bz2 / xz / zst / lz4 读写（单文件流只读）。
- RAR 只读：列表、预检、解压、CRC 测试、密码、固实包、多卷、大条目临时文件中转。
- 安全解压：路径净化、显示名防护、限额熔断、原子覆盖、junction 防护、目录 mtime 还原。
- GUI：多标签页、后台预检、冲突跳过/覆盖/改名/自动编号、创建与追加、删除/重命名/移动/批量改名/正则改名。
- 桌面集成：托盘、单实例转交、资源管理器右键菜单、主题热重载、拖拽打开、快捷键、窗口大小持久化。
- 主题与文案：四套东方主题 + 素体回退、SVG 光栅化、九宫格进度条、主题粒子、内嵌 Noto Sans SC 子集、中英文全量 i18n。
- CLI：`sniff` / `list` / `test` / `extract` / `pack`。

### 变更

- 产品名由 SpellArchive 改为 THzip-SpellArchive；首次启动自动迁移 `%APPDATA%` 配置、单实例键、日志文件名与右键菜单。
- 设置 schema 升级到 4，新增排序键与窗口大小，缺失字段带默认值。
- 错误改为结构化 `FailReason`，由 UI 按当前语言翻译。

### 修复

- AES 条目 CRC 为 0 时不参与比较。
- 追加到加密 zip / 7z 时拒绝，避免新条目以明文写入。
- 固实/流式包跳过条目时正确 drain，避免后续条目错位。
- 缺卷时错误信息列出缺失卷号。
- 解压收尾按深度倒序恢复目录 mtime。
