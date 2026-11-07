# SpellArchive

东方 Project 同人主题归档管理器。当前为**zip / 7z / rar / tar 家族读写（RAR 只读）+ 单文件流读取**的阶段：

- `spell-core`：零 UI 依赖的核心（类型、trait、安全解压、限额、任务线程池、zip/7z/rar/tar 读取与 zip/7z/tar 写入、单文件流读取）
- `spell-app`：egui 三栏界面（主题、i18n、自绘进度条、虚拟化列表、打开/预检/解压/测试/创建/设置）
- `spell-cli`：复用 core 的命令行入口（zip / 7z / rar / tar 的 list / test / extract / pack）

## 运行

```powershell
cargo run -p spell-app                           # 启动 GUI
cargo run -p spell-app -- <归档路径>              # 启动并直接打开归档
cargo run -p spell-cli -- sniff <文件>            # 格式嗅探
cargo run -p spell-cli -- list <归档>            # 列出条目（zip / 7z / tar 家族）
cargo run -p spell-cli -- test <归档>            # CRC 完整性测试
cargo run -p spell-cli -- extract <归档> <目录>   # 安全解压（--force 覆盖）
cargo run -p spell-cli -- pack <输出> <输入...>      # 格式由扩展名决定（zip/7z/tar 家族）
cargo test --workspace                           # 87 个单元 / 集成 / 归档格式用例
powershell -File scripts/package.ps1             # release + 便携目录（有 iscc 时出安装包）
```

## 当前状态

| 模块 | 状态 |
|---|---|
| 格式嗅探（magic bytes） | 可用 |
| 路径净化 / 显示名防护 / 限额计数 / 原子覆盖 / junction 防护 | 可用，含测试 |
| zip 读 | **可用**：列目录、预检、安全解压、CRC 校验、AES 解压；Store / Deflate / Deflate64 / BZip2 / Zstd / PPMd / LZMA / XZ；不支持方法预检提示；同名自动 `~1` 改名 |
| zip 写 | **可用**：Store / Deflate / BZip2 / Zstd、AES-256、目录递归、归档名净化、重名拒绝、追加入现有包、原子替换、zip64 大文件 |
| 7z 读 | **可用**：LZMA/LZMA2/PPMd/BZip2/Deflate/Zstd/LZ4、固实包、单条提取、AES-256 头加密与数据加密、`.7z.001` 分卷拼接 |
| 7z 写 | **可用**：LZMA2 / COPY、AES-256 头加密、原子替换 |
| RAR 读 | **可用**：列表、预检、安全解压、CRC 校验、密码、固实包；基于 `unrar-ng`（UnRAR 许可）；单条上限 512 MiB |
| tar 家族读 | **可用**：tar / tar.gz / tar.bz2 / tar.xz / tar.zst / tar.lz4；符号链接跳过；无扩展名的压缩包会探测内层 tar |
| 单文件流读 | **可用**：`.gz` / `.bz2` / `.xz` / `.zst` / `.lz4`，自动去掉压缩扩展名作为文件名 |
| tar 家族写 | **可用**：tar / tar.gz / tar.bz2 / tar.xz / tar.zst / tar.lz4，原子替换 |
| CLI | `sniff` / `list` / `test` / `extract`（含 --force/--rename/--number）、`pack`（zip 可选 --method bzip2/zstd + 7z + tar 家族） |
| 任务线程池 | **可用**：2 条后台 worker、`crossbeam-channel` 事件、进度节流、取消、panic 兜底 |
| GUI | **可用**：多标签页、打开任意支持格式、后台预检、解压/测试/取消、创建 zip/7z/tar（zip 可选 BZip2/Zstd）、追加 zip、AES-256、后台主题包导入、多选与表头排序、冲突跳过/覆盖/改名/自动编号 |
| 桌面集成 | **可用**：设置持久化（schema 3，含排序键）、关闭到托盘开关、多文件拖拽多标签页、单实例转交、本地化托盘菜单、资源管理器右键菜单（启动自检 exe 路径）、主题热重载 |
| 日志 | `%APPDATA%\SpellArchive\spellarchive.log`（级别/模块，5 MiB 轮转）+ panic hook |
| 主题引擎 | 可扫描 `themes/*/theme.toml`，按需载入 PNG/SVG 纹理与配色、九宫格背景、主题粒子；缺资源回退内置主题 |
| 字体 | 自动加载系统 CJK 字体（simhei / Deng，fontdb 兜底） |
| Windows 资源 | `build.rs`：exe 图标、版本信息、DPI/长路径 manifest；release 隐藏控制台 |

## 主题与素材

`themes/{reimu,marisa,sanae,cirno}/` 是从 `../assets/` 复制并按 `theme.toml` 组织好的四个主题：
头像、功能图标、底图 / 填充进度条。修改主题只需改 `theme.toml` 的路径与配色，
不需要改 Rust 代码。资源 Key 与回退规则见 `docs/theme_spec.md`。

素材授权：CC BY-NC-SA 4.0（禁止商用），代码 MIT；边界见 `NOTICE.md`。

## 打包

`scripts/package.ps1` 会执行 release 构建，并把 exe、themes、README/LICENSE/NOTICE、third_party 许可证复制到
`dist/SpellArchive/`。若 PATH 中有 Inno Setup 的 `iscc`，还会生成
`dist/SpellArchive-0.1.0-setup.exe`（`packaging/SpellArchive.iss`，安装到用户目录，无需管理员）。

## 交接与后续

接手开发请先读 **`HANDOVER.md`**：现状、完成度矩阵、已知问题、下一步接入点都在里面。

## 设计文档

`../SpellArchive 开发方案（Rust 版）v2.1.md`
