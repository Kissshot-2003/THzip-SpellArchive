# THzip-SpellArchive

东方 Project 同人主题归档管理器。当前为**zip / 7z / rar / tar 家族读写（RAR 只读）+ 单文件流读取**的阶段：

- `thzip-core`：零 UI 依赖的核心（类型、trait、安全解压、限额、任务线程池、zip/7z/rar/tar 读取与 zip/7z/tar 写入、单文件流读取）
- `thzip-app`：egui 三栏界面（主题、i18n、自绘进度条、虚拟化列表、打开/预检/解压/测试/创建/设置）
- `thzip-cli`：复用 core 的命令行入口（zip / 7z / rar / tar 的 list / test / extract / pack）

## 运行

```powershell
cargo run -p thzip-app                           # 启动 GUI
cargo run -p thzip-app -- <归档路径>              # 启动并直接打开归档
cargo run -p thzip-app -- --extract-here <归档>   # 打开并解压到当前文件夹
cargo run -p thzip-app -- --extract-to <归档>     # 打开并解压到单独文件夹
cargo run -p thzip-app -- --add <路径...>         # 打开“添加到 zip”对话框
cargo run -p thzip-cli -- sniff <文件>            # 格式嗅探
cargo run -p thzip-cli -- list <归档>            # 列出条目（zip / 7z / tar 家族）
cargo run -p thzip-cli -- test <归档>            # CRC 完整性测试
cargo run -p thzip-cli -- extract <归档> <目录>   # 安全解压（--force 覆盖）
cargo run -p thzip-cli -- pack <输出> <输入...>      # 格式由扩展名决定；7z 可加 --volume-size <MiB>
# sniff / list / test / extract / pack 都支持 --json，输出单行机读结果
cargo test --workspace                           # 145 个单元 / 集成 / 归档格式用例
cargo bench -p thzip-core                        # 格式嗅探 / 10k 列表 / 解压 / 7z 打包基准
powershell -File scripts/package.ps1             # release + 便携目录（有 iscc 时出安装包）
```

## 性能基线（本机，`cargo bench --quick`）

| 场景 | 耗时 |
|---|---|
| `classify_600b`（17 组样本） | ~5 µs |
| 打开并列出 10k 条目 zip（≤ 8 MiB 包） | ~8 ms |
| 仅解析 10k 条目中央目录（文件 seek） | ~100 ms |
| 解压 16 MiB zip 到计数 sink | ~1.7 ms |
| 打包 4 MiB 随机数据为 7z（LZMA2 L6） | ~950 ms |

数字只用于前后对比。8 MiB 以内的 zip 会整包读入内存，避免逐条目 seek；更大的包仍走文件流。
此前 10k 列表约 340 ms，优化后约 8 ms。

## 便携模式

exe 同目录存在 `portable.flag` 或 `themes/` 时自动进入便携模式：配置、日志、单实例 handoff 都写到
`<exe 目录>\config\`，不再使用 `%APPDATA%`。首次便携启动会从 `%APPDATA%\THzip-SpellArchive` 迁移一次设置，
之后以 `config\prefs.toml` 为准。Inno 安装版带 `unins000.exe`，不会被误判为便携，仍使用 `%APPDATA%`。

## 当前状态

| 模块 | 状态 |
|---|---|
| 格式嗅探（magic bytes） | 可用 |
| 路径净化 / 显示名防护 / 限额计数 / 原子覆盖 / junction 防护 | 可用，含测试 |
| zip 读 | **可用**：列目录、预检、安全解压、CRC 校验、AES 解压；Store / Deflate / Deflate64 / BZip2 / Zstd / PPMd / LZMA / XZ；不支持方法预检提示；同名自动 `~1` 改名 |
| zip 写 | **可用**：Store / Deflate / BZip2 / Zstd、AES-256、目录递归、归档名净化、重名拒绝、追加入现有包、原子替换、zip64 大文件 |
| 7z 读 | **可用**：LZMA/LZMA2/PPMd/BZip2/Deflate/Zstd/LZ4、固实包、单条提取、AES-256 头加密与数据加密、`.7z.001` 分卷拼接 |
| 7z 写 | **可用**：LZMA2（级别 / 字典可调）/ COPY、AES-256 头加密、Windows 属性、分卷创建（`.7z.001`）、原子替换；未加密包支持追加（解包重建） |
| RAR 读 | **可用**：列表、预检、安全解压、CRC 校验、密码、固实包、多卷；基于 `unrar-ng`（UnRAR 许可）；大条目临时文件中转 |
| tar 家族读 | **可用**：tar / tar.gz / tar.bz2 / tar.xz / tar.zst / tar.lz4；符号链接跳过；无扩展名的压缩包会探测内层 tar |
| 单文件流读 | **可用**：`.gz` / `.bz2` / `.xz` / `.zst` / `.lz4`，自动去掉压缩扩展名作为文件名 |
| tar 家族写 | **可用**：tar / tar.gz / tar.bz2 / tar.xz / tar.zst / tar.lz4，原子替换；追加为解包重建 |
| CLI | `sniff` / `list` / `test` / `extract`（含 --force/--rename/--number）、`pack`（zip 可选 --method bzip2/zstd；7z 可选 --7z-level/--7z-dict；--append 支持 zip / 7z / tar 家族）；全部支持 `--json` 机读输出 |
| 任务线程池 | **可用**：2 条后台 worker、`crossbeam-channel` 事件、进度节流、取消、panic 兜底 |
| GUI | **可用**：多标签页、打开任意支持格式、后台预检、解压/测试/取消、创建 zip/7z/tar（zip 可选 BZip2/Zstd 与归档注释、7z 可选级别/字典）、追加 zip / 7z / tar 家族、AES-256、打包任务模板、后台主题包导入与导出、预览（双击或工具栏按钮；文本 / PNG / JPEG / GIF / BMP）、多选与表头排序、删除选中条目、重命名/移动/批量改名模板与正则替换改名（目录自动带上子项）、冲突跳过/覆盖/改名/自动编号；中英文全量界面（含对话框、状态栏、任务与右键菜单） |
| 桌面集成 | **可用**：设置持久化（schema 5，含排序键、窗口大小与打包模板）、便携模式、关闭到托盘开关、多文件拖拽多标签页、单实例转交、本地化托盘菜单、资源管理器右键菜单（打开/解压到当前文件夹/解压到单独文件夹/添加到 zip，启动自检 exe 路径）、主题热重载、常用快捷键（Ctrl+O/E/P/T/F、Delete、F2、F5） |
| 日志 | `%APPDATA%\THzip-SpellArchive\thzip-spellarchive.log`（便携模式为 `<exe>\config\`；级别/模块 + `key=value` 结构化字段，5 MiB 轮转）+ panic hook |
| 主题引擎 | 可扫描 `themes/*/theme.toml`，按需载入 PNG/SVG 纹理与配色、九宫格背景、主题粒子；缺资源回退内置主题 |
| 字体 | 内嵌 Noto Sans SC 子集（OFL，约 2.3 MiB）；生僻字回退系统 CJK 字体 |
| Windows 资源 | `build.rs`：exe 图标、版本信息、DPI/长路径 manifest；release 隐藏控制台 |

## 主题与素材

`themes/{reimu,marisa,sanae,cirno}/` 是从 `../assets/` 复制并按 `theme.toml` 组织好的四个主题：
头像、功能图标、底图 / 填充进度条。修改主题只需改 `theme.toml` 的路径与配色，
不需要改 Rust 代码。资源 Key 与回退规则见 `docs/theme_spec.md`。

素材授权：CC BY-NC-SA 4.0（禁止商用），代码 MIT；边界见 `NOTICE.md`。

## 打包

`scripts/package.ps1` 从 `Cargo.toml` 读取版本并执行 release 构建，把 GUI 与 CLI 两个 exe、themes、
README/CHANGELOG/LICENSE/NOTICE、third_party 许可证复制到 `dist/THzip-SpellArchive/`，同时生成 `SHA256SUMS.txt`。
版本号不一致会直接失败。若 PATH 或常见安装目录中有 Inno Setup 的 `iscc`，还会生成
`dist/THzip-SpellArchive-<version>-setup.exe`（`packaging/THzip-SpellArchive.iss`，安装到用户目录，无需管理员），
并打印安装包 SHA-256。

## 交接与后续

接手开发请先读 **`HANDOVER.md`**：现状、完成度矩阵、已知问题、下一步接入点都在里面。

## 从 SpellArchive 迁移

本版把产品名改为 **THzip-SpellArchive**，首次启动会自动迁移：

- 把 `%APPDATA%\SpellArchive\prefs.toml` 复制到 `%APPDATA%\THzip-SpellArchive\prefs.toml`（主题、语言、目录、排序、窗口大小保留）。
- 如果检测到旧 `SpellArchive` 资源管理器右键菜单，会按新名重装并删除旧注册表键。
- exe 名、单实例键、日志文件名已更新；升级期间若旧版仍在运行，新版会把启动参数交给旧版，待旧版退出后再启动新版。
- 自定义主题包在旧安装目录时，请复制到新目录的 `themes/` 或重新导入。

## 设计文档

- `../SpellArchive 开发方案（Rust 版）v2.1.md`：产品设计与完整规范
- `ROADMAP.md`：阶段路线图与验收标准
- `CHANGELOG.md`：版本变更记录
