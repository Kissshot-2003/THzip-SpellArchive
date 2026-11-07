# SpellArchive 交接文档

> 面向接手的开发者。读完这份文档应能：在当前机器跑起来、验证核心功能、看懂分层与数据流、
> 知道哪些已实现、哪些是占位、按路线继续开发。
>
> 最后更新：2026-11（zip 多方法解压、格式读写、桌面集成、日志、主题包导入、打包脚本完成）

---

## 0. 文档地图

| 文档 | 作用 |
|---|---|
| `HANDOVER.md`（本文件） | 现状、架构、测试、坑、下一步 |
| `README.md` | 对外介绍、快速开始、能力表 |
| `../SpellArchive 开发方案（Rust 版）v2.1.md` | 完整产品设计、安全规范、路线图、风险 |
| `docs/theme_spec.md` | `theme.toml` schema、资源 Key、回退规则 |
| `NOTICE.md` | 代码/素材/字体/同人授权边界 |
| `LICENSE` | MIT 全文 |

工程路径：`C:\Users\karl2\Desktop\SpellArchive\spell_archive`
素材目录：`C:\Users\karl2\Desktop\SpellArchive\assets`（与工程同级）

### 0.1 30 秒摘要

- **能做什么**：zip / 7z / tar 家族读写；zip 多方法解压（Deflate64/BZip2/Zstd/PPMd/LZMA）；
  单文件流读取；AES-256；固实 7z；分卷 7z；
  后台预检；多选/排序；创建/追加；密码；主题包导入；托盘、右键菜单、单实例、设置持久化。
- **怎么跑**：`cargo test --workspace`（65）；`cargo clippy --workspace --all-targets -- -D warnings`；
  `cargo run -p spell-app`；release 打包用 `scripts/package.ps1`。
- **分层红线**：`spell-core` 零 UI 依赖；所有写盘走 `safety`；UI 只通过 Reader/Writer 和 `TaskEvent`。
- **当前风险**：RAR 未实现；多文件拖拽只开第一个；zip XZ 与加密 LZMA 未支持；exe 未签名。
- **工作区状态**：最新提交 `4c360e0`，工作区有未提交改动（多选/排序、zip 多方法解压、文档），无远端。
- **下一步建议**：先提交当前批次，再决定 RAR、标签页/多文件、主题视觉增强或签名。

---

## 1. 项目现状

### 1.1 一句话

SpellArchive 是一个 Windows 桌面归档管理器，东方 Project 同人主题只是皮肤。
当前 **zip / 7z / tar 家族可读写，单文件流可读**；GUI 覆盖打开、后台预检、解压、测试、取消、
创建/追加、密码、主题包导入、多选与表头排序、设置持久化、拖拽打开与悬停提示、单实例转交、
关闭到托盘、右键菜单、主题热重载、日志/崩溃落盘。

### 1.2 能力矩阵

| 格式 | 读取 | 创建 | 追加 | 加密 | 备注 |
|---|---|---|---|---|---|
| zip | 是 | 是 | 是 | AES-256 | Store/Deflate 写入；Store/Deflate/Deflate64/BZip2/Zstd/PPMd/LZMA 读取；zip64；同名 `~1` |
| 7z | 是 | 是 | 否 | AES-256 头加密 | 固实包；LZMA2/COPY 写入 |
| tar | 是 | 是 | 否 | 否 | PAX / GNU longname 由 tar crate 处理 |
| tar.gz / tgz | 是 | 是 | 否 | 否 | 无 `.tar` 扩展名时探测内层 ustar |
| tar.bz2 / tbz2 / tbz | 是 | 是 | 否 | 否 | 同上 |
| tar.xz / txz | 是 | 是 | 否 | 否 | 同上 |
| tar.zst / tzst | 是 | 是 | 否 | 否 | 同上 |
| tar.lz4 / tlz4 | 是 | 是 | 否 | 否 | 同上 |
| gz / bz2 / xz / zst / lz4 | 是 | 否 | 否 | 否 | 单文件流，打开时先读一遍求大小 |
| rar | 否 | 否 | 否 | 否 | 仅 magic bytes 识别 |
| 分卷 `.7z.001` | 是 | 否 | 否 | 同主包 | 读取时按字节拼接成逻辑流；从任意卷号打开都可 |

### 1.3 代码规模与提交状态

- Rust 约 11570 行（含测试）。
- 最新提交：`4c360e0 feat: background planning, theme import, logging, volume join and packaging`
- 初始提交：`36842a2 feat: 初始化 SpellArchive Rust 工程骨架与主题系统`
- 当前工作区有未提交改动（多选/排序、zip 多方法解压、文档），没有配置远端。
- 接手第一件事：`git status` → 跑测试 → 决定是否提交当前批次。

---

## 2. 快速开始

### 2.1 环境

- Windows 10/11 x64
- Rust stable（当前验证 1.99.1；`rust-version = 1.85`，edition 2024）
- MSVC Build Tools（`zstd` 需要 C 编译；其余主要是 Rust）
- 可选：系统 `7z`、`tar` 仅用于手工构造测试包

### 2.2 命令

```powershell
cd C:\Users\karl2\Desktop\SpellArchive\spell_archive

cargo test --workspace                 # 63 个测试
cargo clippy --workspace --all-targets -- -D warnings

cargo run -p spell-app                 # GUI
cargo run -p spell-app -- <归档>        # 启动并直接打开

cargo run -p spell-cli -- sniff <文件>
cargo run -p spell-cli -- list <归档>
cargo run -p spell-cli -- test <归档>
cargo run -p spell-cli -- extract <归档> <目标目录> [--force] [--password <pw>]
cargo run -p spell-cli -- pack <输出> <输入...> [--store] [--force] [--append] [--password <pw>]
```

`pack` 的输出扩展名决定容器：

- `.zip`
- `.7z`
- `.tar`
- `.tar.gz` / `.tgz`
- `.tar.bz2` / `.tbz2` / `.tbz`
- `.tar.xz` / `.txz`
- `.tar.zst` / `.tzst`
- `.tar.lz4` / `.tlz4`

`--append` 只支持 zip；`--password` 只支持 zip / 7z；`--store` 对 zip 是 Store、对 7z 是 COPY，
tar 家族忽略。

### 2.3 环境验收

1. GUI 能开；左上角是灵梦头像，工具栏有图标。
2. 主题下拉有 5 项：灵梦 / 魔理沙 / 早苗 / 琪露诺 / 素体。
3. 打开 zip、7z、tar.gz、单文件 `.gz`，条目真实显示。
4. 解压：选目录 → 预检弹窗 → 开始 → 进度/速度正确；运行中可取消。
5. 创建：添加文件夹 → 输出 `.zip` / `.7z` / `.tar.gz`，任务完成后用 `spell list` 读回。
6. 加密：zip / 7z 勾选 AES-256 创建；无密码打开 7z 包头会提示密码。
7. 设置：切主题/语言后退出重开，设置应保持（`%APPDATA%\SpellArchive\prefs.toml`）。
8. 拖拽：把归档拖进窗口应直接打开；拖目录应切换左侧浏览目录。
9. 归档表点“全选”后解压只处理选中项；点表头可切换排序（名称使用自然序）。
10. `cargo test --workspace` 63 通过；clippy 无告警。

---

## 3. 架构

### 3.1 工作区

```text
spell_archive/
├── Cargo.toml                 # workspace，resolver 3，edition 2024
├── rust-toolchain.toml
├── LICENSE / NOTICE.md / README.md
├── HANDOVER.md                # 本文件
├── docs/theme_spec.md
├── scripts/sync-themes.ps1    # assets → themes/<skin>/png
├── themes/{reimu,marisa,sanae,cirno}/
│   ├── theme.toml
│   └── png/
├── crates/
│   ├── spell-core/            # 零 UI 依赖
│   ├── spell-app/             # egui + eframe
│   └── spell-cli/             # 复用 core
└── target/
```

### 3.2 分层规则（不要破坏）

1. `spell-core` 不得出现 `egui` / `eframe` / UI 概念。
   验证：`cargo tree -p spell-core | Select-String egui` 必须为空。
2. UI 不直接调用具体压缩库，只通过 `ArchiveReader` / `ArchiveWriter`。
3. 所有写盘必须经过 `safety` 层：解压用 `ExtractDirSink`，归档创建用 `ArchiveWriter`
   （内部 `AtomicFile`），禁止直接 `File::create` 目标。
4. 重活全部在后台 worker；UI 只处理 `TaskEvent`。
5. 后台 worker 必须经过 `task::worker::guarded`，畸形包 panic 不能带崩进程。

### 3.3 后端模块

| 文件 | 职责 |
|---|---|
| `backend/mod.rs` | `sniff_file` / `classify` / `open` / `create_writer(_with)` / `create_writer_append` |
| `backend/common.rs` | zip/7z/tar 共用的冲突与重解析点检测 |
| `backend/decoder.rs` | tar 与单文件流共用的解压读取器 |
| `backend/zip.rs` | zip 读取：中央目录、名字编码、预检、流式解压、AES、CRC |
| `backend/zip_write.rs` | zip 创建/追加：Store/Deflate、AES-256、zip64、原子替换 |
| `backend/sevenz.rs` | 7z 读取：固实包、AES、单条提取、空根目录过滤 |
| `backend/sevenz_write.rs` | 7z 创建：LZMA2/COPY、AES-256 头加密 |
| `backend/tar.rs` | tar 家族读取：6 种容器、PAX/GNU longname、跳过链接 |
| `backend/tar_write.rs` | tar 家族创建：6 种容器、流式进度、取消 |
| `backend/stream.rs` | 单文件流读取：gz/bz2/xz/zst/lz4 |
| `backend/name_codec.rs` | zip 名字 UTF-8 → GBK / Shift-JIS 择优 |
| `backend/volume.rs` | `.7z.001` 识别、收集与 `MultiVolumeReader` 拼接 |

### 3.4 `spell-app` 模块

| 文件 | 职责 |
|---|---|
| `main.rs` | 单实例、日志初始化、窗口参数、启动参数/目录 |
| `app.rs` | 状态、事件泵、任务表、打开/解压/创建、预检、设置保存、拖拽、热重载轮询 |
| `prefs.rs` | schema 2 配置、handoff、日志路径 |
| `logging.rs` | 文本日志 + panic hook，5 MiB 轮转 |
| `i18n/mod.rs` | zh-CN / en-US + 主题覆盖 + 标准术语开关 |
| `theme/{mod,loader}.rs` | 主题扫描、PNG 纹理、签名轮询、主题目录定位 |
| `tray.rs` | 托盘图标与菜单 |
| `shell_menu.rs` | HKCU 资源管理器右键菜单 |
| `ui/*` | 工具栏、文件浏览、归档表、任务行、预检/密码/打包对话框、设置、进度条、拖拽遮罩 |

### 3.5 启动顺序

```text
main
 ├─ SingleInstance::new("SpellArchive.v1")
 │   └─ 非首实例：把启动参数写入 handoff.txt → 激活已有窗口 → 退出
 └─ run_native（首实例）
     └─ SpellApp::new
         ├─ install_cjk_fonts          # simhei.ttf → Deng.ttf
         ├─ Prefs::load()              # %APPDATA%\SpellArchive\prefs.toml
         ├─ ThemeStore::load(ctx)      # 扫描主题 + 载入 PNG
         ├─ 按 prefs.theme 选中主题
         ├─ 恢复 locale / prefer_standard / show_hint / last_dir
         ├─ themes.apply(ctx)          # Dark + all_styles_mut
         ├─ TaskRunner::new(2)         # 两条后台 worker
         ├─ Tray::new()                # 托盘图标 + 显示/隐藏/退出菜单
         ├─ refresh_dir()              # 左侧本地文件列表
         └─ open_path(arg)             # 有启动参数则提交 Open 任务
```

### 3.6 每帧顺序

```text
App::ui(root_ui)
 ├─ close_requested → CancelClose + 隐藏（托盘存在且未退出时）
 ├─ 处理 dropped_files            # 目录 → navigate；文件 → open_path
 ├─ poll_tray()                    # 菜单事件：显示 / 隐藏 / 退出
 ├─ poll_handoff()                 # 每 250ms 取第二实例转交的路径
 ├─ poll_theme()                   # 每 1s 检查主题文件签名
 ├─ tick()                        # 排空 TaskEvent + 速度估算
 ├─ hint_banner  → Panel::top
 ├─ top_bar      → Panel::top
 ├─ status_bar   → Panel::bottom
 ├─ file_browser → Panel::left
 ├─ task_panel   → Panel::bottom
 ├─ archive_view → CentralPanel
 ├─ dialogs      → Window（预检 / 密码 / 打包；需要 ctx）
 ├─ settings     → Window（需要 ctx）
 ├─ drag_overlay → Foreground 图层（拖拽悬停提示）
 ├─ maybe_save_settings()         # 变更后 400ms 防抖落盘
 └─ request_repaint_after：有任务/设置 33ms，空闲 500ms（保证 handoff 轮询）
```

面板必须按“先 top/bottom，再 left/right，最后 CentralPanel”的顺序挂到同一个 root `Ui`。

### 3.7 数据流

打开：

```text
按钮 / 启动参数 / 双击 / 拖拽
 └─ TaskRunner::submit(TaskOp::Open)
     └─ worker: backend::open(path, password?)
         ├─ PasswordRequired → TaskEvent::NeedPassword → 密码框 → 重新提交
         └─ Ok(reader) → TaskEvent::Opened { entries, Arc<Mutex<Box<dyn ArchiveReader>>> }
```

解压 / 测试：

```text
UI 选目标目录
 └─ TaskOp::Plan { reader, ids, target, limits }   # 后台 worker 预检
     └─ TaskEvent::PlanReady { plan, target }
         └─ 预检弹窗：条目数 / 总大小 / 最大文件 / 冲突 / 剩余空间 / 加密
             └─ DecisionTable { 跳过已有 | 覆盖已有 }
             └─ TaskOp::Extract { reader, plan, decisions, limits }
                 ├─ worker: ExtractDirSink + ProgressSink
                 └─ TaskEvent::{Started, Progress, Finished, Failed} → UI

完整性测试：TaskOp::Test { reader, limits } + VerifySink
```

创建 / 追加：

```text
打包弹窗：添加文件 / 文件夹、输出路径、格式由扩展名决定、仅存储、AES、追加
 └─ TaskOp::Pack { output, inputs, kind, method, append, password }
     ├─ worker: scan_inputs
     ├─ create_writer_with / create_writer_append
     ├─ write_entries（累计进度）
     └─ PackFinished { report } / Failed
```

---

## 4. 格式支持细节

### 4.1 zip

读取：

- `zip = 4.6`，启用 `deflate`、`deflate-flate2-zlib-rs`、`deflate64`、`bzip2`、`zstd`、`ppmd`、`aes-crypto`。
- 名字用 `name_raw()` + `name_codec` 还原，不直接吃 CP437。
- 支持 AES 解压；AES 条目 CRC 字段为 0，读取时置 `crc32=None`，避免误报。
- `plan()` 负责路径净化、冲突、重解析点、超长、链接、可疑显示名。
- 解压走 64 KiB 流式读取 → `ExtractDirSink`（预算 + CRC + 原子写）。
- 同名/大小写冲突自动 `~1` 改名。
- LZMA（方法 14）由 `lzma-rust2` 自行解码：zip crate 的解压器不识别 ZIP 内部 LZMA 头。
  解析 2 字节版本 + 2 字节属性长度 + 5 字节属性后再喂给 `LzmaReader::new_with_props`；
  字典上限 512 MiB，超过直接拒绝；加密的 LZMA 条目暂不支持。
- XZ（方法 95）及未启用方法仍报 `Unsupported`，见技术债。

写入：

- Store / Deflate；zip64 大文件；本地 DOS 时间（Windows 用 `FileTimeToLocalFileTime`）。
- 追加：复制原包到临时文件 → `ZipWriter::new_append` → 原子替换；拒绝大小写重名。
- AES-256：`with_aes_encryption(AesMode::Aes256, …)`；密码走 `SecretString`。
- 追加模式不允许设置新密码。

### 4.2 7z

读取：

- `sevenz-rust2 = 0.20`（兼容 rust-version 1.85；已启用 `deflate` / `zstd` / `lz4` features）。
- LZMA/LZMA2/PPMd/BZip2/Deflate/Zstd/LZ4 由 crate 支持。
- 固实包：跳过未选条目时必须把子流 drain 掉，否则后续条目错位。
- AES-256 头加密：无密码 `open` 返回 `PasswordRequired`。
- 空名字根目录条目会过滤，`EntryId` 重新连续编号。
- 单条提取已有测试。

写入：

- `ArchiveWriter` + `push_archive_entry`；默认 LZMA2，`Method::Store` 时 COPY。
- 有密码时 `AesEncoderOptions` + `set_encrypt_header(true)`。
- 不支持追加；固实块/字典参数暂未暴露。

### 4.3 tar 家族与单文件流

读取：

- `tar` + flate2 / bzip2 / lzma-rust2（XzReader）/ zstd / lz4_flex。
- PAX、GNU longname、hardlink 由 tar crate 处理；符号链接/硬链接/特殊文件计划为 Skip。
- 文件名没有 `.tar` 时，解压前 1 KiB 判断内层是否 `ustar`，是则升级为对应 Tar 容器。
- 单文件流条目名去掉压缩扩展名：`note.txt.gz` → `note.txt`。
- 单文件流打开时先解压一遍求大小，解压时再次读取（见技术债）。

写入：

- tar / tar.gz / tar.bz2 / tar.xz / tar.zst / tar.lz4。
- 文件权限固定 644，目录 755；目录 mtime 用当前时间。
- 不支持加密与追加。

---

## 5. 核心接口

### 5.1 条目

```rust
pub type EntryId = u32;                 // 同一 open 内稳定

pub struct ArchiveEntry {
    pub raw_name: Arc<[u8]>,            // 落盘以它为准
    pub display: Arc<str>,              // 仅 UI；应经 name_codec + display_guard
    pub kind: EntryKind,                // File / Dir / Symlink / Hardlink / Other
    pub size: u64,
    pub packed: Option<u64>,            // 固实 7z/stream 为 None
    pub crc32: Option<u32>,
    pub mtime: Option<SystemTime>,       // zip=本地墙钟，7z=UTC，tar=UTC
    pub encrypted: bool,
    pub method: Method,
}
```

禁止用文件名定位条目（zip 允许重名），一律使用 `EntryId`。

### 5.2 解压契约

```rust
pub trait ArchiveReader: Send {
    fn kind(&self) -> ArchiveKind;
    fn entries(&self) -> &[ArchiveEntry];
    fn is_solid(&self) -> bool;
    fn plan(&self, ids: &[EntryId], target: &Path, limits: &Limits) -> Result<ExtractPlan, ArchiveError>;
    fn extract(&mut self, plan: &ExtractPlan, decisions: &DecisionTable,
               cancel: &CancelToken, sink: &mut dyn ExtractSink) -> Result<ExtractReport, ArchiveError>;
    fn test(&mut self, cancel: &CancelToken, sink: &mut dyn ExtractSink) -> Result<ExtractReport, ArchiveError>;
}
```

`ExtractSink::on_data` 是唯一写盘/计数点；预算、进度、CRC 都挂这里。

### 5.3 创建契约

```rust
pub trait ArchiveWriter: Send {
    fn add(&mut self, src: &Path, name_in_archive: &str, method: Option<Method>,
           cancel: &CancelToken, progress: &dyn Fn(u64)) -> Result<(), ArchiveError>;
    fn add_dir(&mut self, name_in_archive: &str) -> Result<(), ArchiveError>;
    fn set_comment(&mut self, text: &str) -> Result<(), ArchiveError>;
    fn finish(self: Box<Self>) -> Result<FinishReport, ArchiveError>;
}
```

- `writer::scan_inputs`：目录递归、空目录条目、跳过符号链接、大小写重名拒绝。
- `writer::write_entries`：统一进度（全部输入累计字节）。
- 所有写入器都使用 `AtomicFile`，取消/出错时临时文件自动删除，原目标不变。

### 5.4 任务

```rust
pub enum TaskOp {
    Open { path, password },
    Extract { reader, plan, decisions, limits },
    Test { reader, limits },
    Pack { output, inputs, kind, method, append, password },
}
```

事件：`Opened` / `NeedPassword` / `PhaseChanged` / `Started` / `Progress` /
`Warning` / `NeedDecision` / `EntryFailed` / `Finished` / `PackFinished` / `Failed`。

### 5.5 错误

`ArchiveError` 负责流程级错误（打开、密码、限额、取消、IO、Unsupported）；
`SinkError` 负责写盘/预算错误。逐条失败进入 `ExtractReport.failed`，不中断整包。

---

## 6. 安全设计

### 6.1 路径与显示名

- `safety::path_sanitize`：拒绝 `..`、绝对路径、盘符、ADS、Windows 保留名、
  尾随点/空格、超深/超长路径；`resolve_under` 保证最终路径在目标目录内。
- `safety::display_guard`：把 U+202E 等 bidi/零宽字符替换为可见标记，UI 标记“伪装”。
- 解压落盘前检测 junction/symlink（重解析点），拒绝顺着链接写出去。

### 6.2 限额

`Limits` 三档预设（严格/默认/宽松），默认值：

- 总量 20 GiB、单条 8 GiB、条目 200,000
- 流式压缩比上限 200
- 路径深度 64、长度 32767
- 字典 512 MiB、嵌套 2

`Budget` 在 `ExtractDirSink` 内执行，触发即 `SinkError::Limit`。

### 6.3 原子写

`AtomicFile`：同目录临时文件 + `ReplaceFileW` / `MoveFileExW`。
覆盖已有文件必须走它；失败/取消时原文件保持不变。
`AtomicFile` 同时实现 `Write + Seek`，供 zip / 7z / tar 写入器复用。

### 6.4 密码

- `SecretString` 内部 `Zeroizing<String>`，Debug 打码。
- 密码只通过 `TaskOp` / `ArchiveWriter` 传递，不写日志、不落盘。
- 7z 头加密无密码时 `PasswordRequired` → GUI 密码框重试。

### 6.5 取消与崩溃

- `CancelToken` 每 64 KiB / 每条目检查；压缩解码内部也会检查。
- worker 使用 `catch_unwind`；release profile 保持 `panic = "unwind"`。
- GUI 取消按钮只写 token，不阻塞 UI；worker 返回 `Cancelled`。

---

## 7. GUI 实现要点

### 7.1 egui 0.37 是 Ui-first

| 旧写法 | 0.37 写法 |
|---|---|
| `fn update(&mut self, ctx, frame)` | `fn ui(&mut self, ui: &mut egui::Ui, frame)` |
| `TopBottomPanel::top("x").show(ctx, …)` | `egui::Panel::top("x").show(ui, …)` |
| `SidePanel::left("x")` | `egui::Panel::left("x")` |
| `CentralPanel::show(ctx, …)` | `CentralPanel::show(ui, …)` |
| `.default_height / .min_height` | `.default_size / .min_size / .exact_size` |
| `ctx.set_style(...)` | `ctx.all_styles_mut(...)` |
| `ctx.set_visuals(v)` | `ctx.set_visuals_of(Theme::Dark, v)` 或 `all_styles_mut` |
| `Window::show(ctx, …)` | 仍是 `ctx` |

### 7.2 表格

`egui_extras::TableBuilder`，不要手工 `add_sized` 拼表头。
列宽：名称 280（可裁切）、大小 84、压缩后 84、比率 60、方法 72、修改时间 110、标记 remainder。

- 表头按钮可排序（名称/大小/压缩后/比率/方法/时间），再点一次反向；名称使用自然序。
- 选中集合是 `BTreeSet<usize>`：单击单选、Ctrl 切换、Shift 连选；“全选/清空”作用于当前搜索结果。
- 解压时若选中集合非空，只解压这些条目；否则解压全部。

### 7.3 任务行

固定列宽：名称 240 / 速度 96 / 状态 120 / 取消 24，间距共用；
`bar_width` 每帧算一次供所有行使用。名称和状态都 `.truncate()`。

### 7.4 符阵进度条

```rust
let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(lit, 1.0));
let fill_rect = Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * lit, rect.max.y));
painter.add(Shape::image(fill.id(), fill_rect, uv, WHITE));
```

UV 与目标矩形必须同时按比例裁剪；只裁 UV 会把左侧比例拉伸到整条。
逐格模式：`lit = round(ratio * 10) / 10`。

### 7.5 对话框

- 预检在后台 `TaskOp::Plan` 执行，完成后 `TaskEvent::PlanReady` 弹窗；不会卡住 UI。
- 预检内容：条目数、总大小、最大单文件、冲突、警告、剩余空间、加密提示；
  覆盖策略只有“跳过已有 / 覆盖已有”（Rename/AutoNumber 尚未实现）。
- 密码：打开任务返回 `NeedPassword` 或失败信息含“密码”时弹出。
- 打包：添加文件/文件夹、输出路径、扩展名决定格式、仅存储、AES、追加；
  非 zip 禁用追加；非 zip/7z 禁用加密；输出已存在需二次确认。

### 7.6 托盘与关闭行为

- `tray.rs`：`tray-icon` + `muda` 菜单（显示主窗口 / 隐藏主窗口 / 退出）。
- 图标优先读 `themes/reimu/png/avatar.png`，找不到用红色 32×32 占位。
- 有托盘时点窗口关闭按钮会 `CancelClose` 并 `Visible(false)`；托盘“退出”才真正关闭。
- 双击托盘图标恢复窗口；托盘事件在 UI 线程每帧轮询。
- 第二实例 handoff 也会自动显示窗口（隐藏状态下可用）。

### 7.7 设置持久化

- 路径：`%APPDATA%\SpellArchive\prefs.toml`（没有 APPDATA 时退回当前目录）。
- 当前 schema 2；字段：`theme`、`locale`、`prefer_standard`、`show_hint`、`close_to_tray`、`last_dir`。
- schema 1 读取时自动升级到 2（缺失 `close_to_tray` 走默认 true）。
- 变更后 400ms 防抖，用 `AtomicFile` 写入。
- 语言切换必须同时改 `app.locale` 和 `app.tr.locale`（曾因只改前者导致界面语言不变）。

### 7.8 资源管理器右键菜单

- `shell_menu.rs` + `winreg`，只写 HKCU，不需要管理员。
- 注册两个位置：`Software\Classes\*\shell\SpellArchive`（文件）和
  `Software\Classes\Directory\shell\SpellArchive`（目录）；命令为 `"<exe>" "%1"`。
- 设置页复选框实时读取注册表状态；勾选安装、取消卸载。
- Win11 默认收进“显示更多选项”；目前使用 `target/debug` 的 exe 路径，发布前需重新勾选。

### 7.9 主题热重载

- `theme::loader::signature()` 返回主题文件 `(数量, 最新 mtime)`。
- `App::poll_theme` 每秒比较签名；变化时重新 `ThemeStore::load`，按主题 id 恢复选中，
  再 `tr.set_theme` + `themes.apply(ctx)`，状态栏提示“主题文件已重新加载”。
- 没有引入文件监听依赖；代价是每秒 stat 一次主题目录（约 100 个文件）。
- 新增主题目录、修改 `theme.toml` 或替换 PNG 都会触发。

### 7.10 Windows 资源

- `crates/spell-app/build.rs` 使用 `winresource`：嵌入 `icon.ico`、`app.manifest` 和版本字符串。
- `icon.ico` 由 `themes/reimu/png/avatar.png` 经 ImageMagick 生成（16~256 多尺寸）。
- `app.manifest` 声明 PerMonitorV2 DPI、longPathAware、Win10/11 supportedOS。
- `main.rs` 用 `cfg_attr(not(debug_assertions), windows_subsystem = "windows")`：
  debug 保留控制台，release 不弹黑框。
- `build.rs` 修改后需重新编译；`cargo:rerun-if-changed` 已覆盖图标与 manifest。

### 7.11 拖拽悬停提示

- `ui::drag_overlay` 读取 `raw.hovered_files`，在 `Order::Foreground` 图层画半透明遮罩、
  accent 边框和“松开以打开归档 / 浏览文件夹”提示。
- 不拦截鼠标事件；drop 仍由 `App::ui` 开头统一处理。
- 当前只打开第一个文件，多文件拖入时其余被忽略（见技术债 #12）。

### 7.12 字体

`install_cjk_fonts` 读 `C:/Windows/Fonts/simhei.ttf`（回退 `Deng.ttf`）。
egui 默认无 CJK 字形，不装字体就是豆腐块。
待办：`fontdb` 查找 + 内嵌 Noto Sans SC 子集（OFL），供 Linux 构建兜底。

### 7.13 主题包导入

- 设置页“导入主题包…”选择 zip；要求包内是 `<主题目录>/theme.toml`。
- 用 `backend::open` + `ExtractDirSink` 解压到 `themes/`；冲突策略默认 Skip。
- 导入成功后把 `next_theme_check` 提前，下一次热重载轮询立即生效。
- 同步执行；超大主题包会短暂卡住 UI（技术债 #3）。

### 7.14 日志与自然排序

- `logging.rs` 写 `%APPDATA%\SpellArchive\spellarchive.log`；启动、任务失败、设置保存失败、
  主题导入、panic 都会落盘；超过 5 MiB 下次启动截断。
- 文件浏览器使用 `natural_cmp`：`file2` 排在 `file10` 前，数字段按数值比较。
- 归档表支持表头排序与多选；排序状态暂不持久化（技术债 #7）。

### 7.15 打包

- `scripts/package.ps1`：release 构建 → `dist/SpellArchive/`（exe + themes + 文档）。
- `packaging/SpellArchive.iss`：Inno Setup，安装到 `%LOCALAPPDATA%\Programs\SpellArchive`，
  无需管理员；有 `iscc` 时生成 `dist/SpellArchive-<version>-setup.exe`。
- 未做代码签名（技术债 #15）。

---

## 8. 主题与素材

### 8.1 数据流

```text
../assets/  (原始素材)
   └─ scripts/sync-themes.ps1
        └─ themes/<skin>/png/       (运行时读取)
             └─ theme.toml [resource] key → 路径
                  └─ ResKey → HashMap<ResKey, TextureHandle>
                       └─ UI
```

素材源与主题目录是两份：改素材后跑一次 `.\scripts\sync-themes.ps1`，`theme.toml` 不会被覆盖。

注意：`assets/tokens/tokens.json` 里的文件名多数不存在；实际可用的是
`icons/icon-*.png`、`avatars/*-256.png`、`progress/*.png`。
PanelBg / 粒子 / 失败态进度图尚未提供，主题里故意缺失并走回退。

### 8.2 新增主题

1. 复制 `themes/reimu/` 为 `themes/<新 id>/`。
2. 改 `theme.toml` 的 `[meta]`、`[color]`、`[resource]`。
3. 不需要改 Rust；启动自动扫描；`loader::rank()` 只影响排序。
4. 缺资源会回退，不会崩；`[resource]` 写错 Key 只 warn。

### 8.3 新增资源 Key

四处必须同时改：

1. `docs/theme_spec.md`（唯一真源）
2. `spell-app/src/theme/mod.rs`：`ResKey` + `ALL` + `name()`
3. 各主题 `theme.toml` 的 `[resource]`
4. 使用处

建议后续加 `xtask check-theme-keys` 自动比对。

### 8.4 Loader

- 主题根目录：`./themes` → `<exe>/themes` → `<exe>/../../themes`。
- 路径必须相对、无 `..`，canonicalize 后仍在主题目录内。
- `sampling_pixel = true` → NEAREST，否则 LINEAR。
- `schema > 1` 只警告，不 panic。
- 尚未执行 8 MiB / 64 MiB / 500 文件限制；主题包导入前必须补。

---

## 9. 测试

### 9.1 命令

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

当前共 **65** 个测试。

### 9.2 测试分布

| 位置 | 数量 | 覆盖 |
|---|---:|---|
| `spell-core` 单元测试 | 32 | path_sanitize、display_guard、bomb、atomic_fs、worker、plan、name_codec、volume 读取+分卷、sevenz、stream |
| `spell-app` 单元测试 | 4 | theme loader 2、prefs 解析 1、自然排序 1 |
| `tests/zip_backend.rs` | 10 | zip 往返、穿越、绝对路径、大小写冲突、Skip/Overwrite、条目上限、CRC、BZip2/Zstd/PPMd 往返、LZMA/Deflate64 fixtures |
| `tests/zip_write.rs` | 7 | zip 创建、取消、原子性、名字净化、追加、重名拒绝、AES |
| `tests/task_runner.rs` | 5 | Open→Plan→Extract、取消、Pack、失败事件 |
| `tests/tar_backend.rs` | 1 | 6 种 tar 容器读取/解压/测试 |
| `tests/tar_write.rs` | 2 | 6 种 tar 容器创建往返、密码拒绝 |
| `tests/stream_backend.rs` | 1 | 5 种单文件流读取/解压/测试 |
| `tests/sevenz_write.rs` | 3 | 7z 普通往返、加密往返、分卷拼接 |

### 9.3 手工验收（每次改格式/任务层都跑）

1. `cargo run -p spell-app -- <zip>`：条目、时间、图标正确。
2. 解压 3 个条目到空目录，完成后核对内容。
3. 解压到已有文件：分别试“跳过”和“覆盖”。
4. 大包解压中点取消，UI 不卡，任务显示“已取消”。
5. 创建 zip / 7z / tar.gz，用 `spell list` 与 `spell test` 验证。
6. 创建加密 zip / 7z，无密码打开应提示，输入密码后可解压。
7. 追加到 zip，确认原条目保留；追加同名应失败且原包不变。
8. 切主题/语言，退出重开，设置保持。
9. 拖入归档/目录，行为正确。

---

## 10. 已知问题 / 技术债

| # | 问题 | 影响 | 建议 |
|---:|---|---|---|
| 1 | `task_panel` 每帧 clone 整个 Theme（含纹理 HashMap） | 轻微浪费 | `Arc<Theme>` 或只克隆纹理 id |
| 2 | 冲突策略 Rename/AutoNumber 没有真正改名 | UI 只能选跳过/覆盖 | 在 `resolve_all` 对已存在文件生成 `~n` |
| 3 | 主题包导入仍在 UI 线程执行 | 超大主题包会短暂卡顿 | 需要时改成 `TaskOp::InstallTheme` |
| 4 | 多选只有解压能用，没有批量删除/批量移动 | 批量操作仍有限 | 视需求加右键批量菜单 |
| 5 | 进度条底图拉伸到任意宽高 | 像素素材边缘不完美 | 按 `theme.scale` 吸附整数倍 |
| 6 | 启动加载全部主题纹理 | 启动内存/时间偏高 | 懒加载，切换再解码 |
| 7 | 归档表排序状态不持久化；搜索只做小写子串 | 重启后回到名称升序 | 排序写进 prefs；搜索支持通配/正则 |
| 8 | 无 SVG 支持 | plain 主题只能用纯色 | 开 `egui_extras` svg |
| 9 | `[resource_nine_patch]` 解析但未使用 | 面板背景不能九宫格 | `ui/nine_patch.rs` |
| 10 | 日志是简单文本，没有级别/模块 | 排查复杂问题不够 | 需要时换 `tracing` |
| 11 | `prefs.toml` 目前只有 schema 1→2 的被动补齐 | 再改字段需显式迁移 | 在 `Prefs::load` 里继续加分支 |
| 12 | 拖拽多个文件只打开第一个 | 批量拖入时其余项目被忽略 | 需要标签页/多 reader；或提示数量 |
| 13 | `themes/*/png` 是复制品 | 素材更新要跑同步脚本 | 已有 sync 脚本 |
| 14 | 当前批次未提交 | 无回滚点 | 确认测试后提交 |
| 15 | exe 未做代码签名 | SmartScreen 可能拦截 | 发布前购买/配置签名证书 |
| 16 | zip 写入只有 Store/Deflate；XZ 读取未启用 | 创建时不能选 bzip2/zstd/lzma；XZ 包报 Unsupported | 读取已覆盖 Deflate64/BZip2/Zstd/PPMd/LZMA；XZ 需要时再评估 |
| 17 | zip 追加会完整复制原包 | 大包耗时、临时空间翻倍 | 增量原地追加风险高，暂不做 |
| 18 | 打包扫描跳过符号链接 | 链接内容不会进包 | 需要时提供“跟随链接”+ 环检测 |
| 19 | 追加模式不能设置新密码 | 加密包追加的新条目不加密 | 明确提示或禁止追加加密包 |
| 20 | 7z 固实包单条提取进度按写出字节算 | 跳过前序数据时进度停滞/跳变 | 通过解码字节回调上报压缩进度 |
| 21 | 7z 不支持追加 | 增量修改需重建 | 复制重写，暂不做 |
| 22 | 7z 写入固定 LZMA2/COPY 与默认参数 | 不能调固实块、字典大小 | 暴露 `EncoderConfiguration` |
| 23 | 7z 分卷要求卷号从 001 连续 | 缺卷时只报打开失败 | 错误信息里列出缺失卷号 |
| 24 | 单文件流打开时先解压一遍求大小 | 大文件打开/解压读两遍 | 列表尺寸懒加载或显示“未知” |
| 25 | tar 写入不保留原始权限/目录 mtime | 解压后 644/755 | 从 metadata 读取 mode/mtime |
| 26 | tar 家族不支持追加与加密 | 只能新建 | 追加需重写整包 |
| 27 | RAR 未实现 | 只能识别 magic | libarchive 或可选 C 后端 |
| 28 | 单实例 handoff 只转发第一个路径，且靠 250/500ms 文件轮询 | 多文件转交只开第一个；空闲有低频唤醒 | 需要时换 named pipe / WM_COPYDATA |
| 29 | 托盘菜单文案固定中文，切语言不刷新 | 英文界面下托盘仍是中文 | 语言变更时重建菜单 |
| 30 | 右键菜单记录的是当前 exe 绝对路径 | 移动/升级 exe 后菜单失效 | 安装包升级后重新写注册表，或启动时自检路径 |

---

## 11. 下一步路线

### P1-3 桌面集成（已完成）

1. 单实例：mutex + `handoff.txt` + `SetForegroundWindow`。
2. 托盘：显示/隐藏/退出菜单、关闭到托盘、双击恢复。
3. 右键菜单：`winreg` 写 HKCU，文件与目录，设置页开关；Win11 收进“显示更多选项”。
4. 主题热重载：每秒轮询 `theme.toml` / `png` 签名，保留当前主题 id。
5. 拖拽：文件/目录打开 + 全屏悬停覆盖提示。

### P2 功能

- 九宫格面板、SVG 图标、粒子效果。
- 搜索增强（通配/正则）与排序持久化。
- 语言包扩展、字体 fallback（内嵌 Noto Sans SC 子集）。
- `tracing` 结构化日志（当前是文本日志）。
- 代码签名；Inno 脚本已就绪。
- RAR 读取（可选特性/外部后端）。

### 格式增强

- zip：XZ 解码；写入端可选 BZip2/Zstd；加密 LZMA 条目。
- 7z：固实包压缩字节进度；可选追加（需重写）；缺少卷号时给出缺失列表。
- tar：保留权限/mtime；可选追加/加密。
- RAR：可选特性或外部后端（注意许可证与单 exe 约束）。

---

## 12. 开发约定与坑

1. **文件编码**：统一 UTF-8。不要在 PowerShell 5.1 里用
   `Get-Content -Raw` + `Set-Content` 往返改源码；本项目已因此出现过中文乱码和换行变化。
   用编辑器或专用工具。
2. **不要开 `panic = "abort"`**：release profile 固定 unwind，否则 worker 兜底失效。
3. **不要用文件名定位条目**：zip 允许重名，一律 `EntryId`。
4. **不要绕过 `path_sanitize` / `write_atomic`**；创建归档只能走 `ArchiveWriter`。
5. **不要给 `spell-core` 加 UI 依赖**。
6. **压缩/解压不要进 rayon 全局池**；嵌套并行 + 内存峰值失控。
7. **时间语义**：zip 是本地墙钟（无时区），7z/tar 是 UTC；混用会漂移。
8. **空目录**：zip/tar 需要显式目录条目，否则解压丢结构。
9. **FAT32**：单文件 4 GiB 上限，写失败要给换文件系统建议。
10. **符号链接**：Windows 创建需特权；默认跳过是正确的。
11. **固实/流式包**：跳过条目必须把子流读完（drain），否则后续条目错位。
12. **主题文案**：可改措辞，但 tooltip 必须给标准术语，且不能隐藏错误。
13. **素材许可证**：代码 MIT，内置素材 CC BY-NC-SA 4.0；商用需替换素材。
14. **归档名净化**：用户输入统一经 `writer::normalize_entry_name`，不能直接进 zip。
15. **AES CRC**：AES 条目 CRC 字段为 0，不能参与 CRC 比较；依赖 AE-2 认证码。
16. **语言切换**：同时更新 `app.locale` 与 `app.tr.locale`，否则只有按钮文字变。
17. **设置保存**：变更后 400ms 防抖写 `prefs.toml`；不要在每帧直接写盘。

---

## 13. 关键决策记录（ADR）

| 决策 | 结论 | 原因 |
|---|---|---|
| GUI | egui + eframe 0.37 | 自绘像素 UI、单二进制、跨平台 |
| 7z | sevenz-rust2 0.20 | 纯 Rust；7z.dll 无 C ABI，需 COM vtable + 分架构 DLL |
| tar 解压 | tar + flate2/bzip2/lzma-rust2/zstd/lz4_flex | 纯 Rust 为主，保持单 exe |
| 解压接口 | plan → decisions → extract 两阶段 | worker 不能同步询问 UI |
| 覆盖 | 临时文件 + ReplaceFileW | 直接写会截断原文件 |
| 任务 | 2 worker + crossbeam-channel | UI 不卡；取消/进度事件统一 |
| 日志 | 文本文件 + panic hook | 零额外依赖，足够定位用户报错 |
| 文案 | i18n 与主题解耦 | 换素体主题不能丢中文 |
| 表格 | egui_extras TableBuilder | 手工拼列必然错位 |
| 工程结构 | workspace，core 零 UI 依赖 | 用 Cargo 强制分层 |
| 设置 | `%APPDATA%\SpellArchive\prefs.toml` | 不污染安装目录，符合 Windows 习惯 |
| 单实例 | mutex + handoff 文件 + SetForegroundWindow | 不引入 IPC 依赖；跨进程转交简单 |
| 托盘 | tray-icon + muda，关闭隐藏 | 桌面工具习惯；托盘菜单可恢复/退出 |
| 右键菜单 | winreg 写 HKCU | 免管理员；Win11 收进“显示更多选项” |
| 平台 | Windows 10+ / x64 | Win7 需要旧工具链，不值得 |

---

## 14. 交接检查清单

接手当天逐条勾：

- [ ] 读 README、本文件、`docs/theme_spec.md`、设计文档 v2.1
- [ ] `cargo test --workspace`（65 通过）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo run -p spell-app` 能开，5 个主题可切换
- [ ] 打开 zip / 7z / tar.gz / 单文件 gz，条目真实显示
- [ ] 解压 / 取消 / 覆盖策略 / 密码重试各试一次
- [ ] 创建 zip / 7z / tar.gz，并 `spell list` 读回
- [ ] 追加 zip：原条目保留，重名失败
- [ ] 确认 `%APPDATA%\SpellArchive\prefs.toml` 保存主题/语言/目录
- [ ] 拖拽归档与目录行为正确
- [ ] 已开窗口时再次双击归档：第二实例退出，第一实例打开该文件
- [ ] 点窗口关闭按钮：进程留在托盘；再次启动或双击托盘能恢复
- [ ] 设置页勾选/取消右键菜单，`reg query HKCU\Software\Classes\*\shell\SpellArchive` 有/无
- [ ] 运行中 touch `themes/*/theme.toml`，2 秒内出现“主题文件已重新加载”
- [ ] 从资源管理器拖文件到窗口，出现“松开以打开归档”覆盖层
- [ ] 设置页切换“关闭窗口时最小化到托盘”，重开程序后保持
- [ ] 导入一个 `<主题>/theme.toml` 的 zip，主题出现在下拉框
- [ ] 解压时预检在后台（快速点其他 UI 不卡），随后弹预检框
- [ ] 归档表点“全选”后解压只处理选中项；点表头可切换排序
- [ ] `%APPDATA%\SpellArchive\spellarchive.log` 有启动记录；panic 会落盘
- [ ] `scripts/package.ps1` 能产出 `dist/SpellArchive/`
- [ ] `(Get-Item target/debug/spell_archive.exe).VersionInfo` 有 FileDescription/FileVersion
- [ ] 确认 `spell-core` 依赖树无 egui
- [ ] 检查 `git status`，决定是否提交当前 5 个文件
- [ ] 确认素材许可证边界（代码 MIT / 素材 CC BY-NC-SA 4.0）

---

## 15. 文档索引

| 文件 | 内容 |
|---|---|
| `README.md` | 对外介绍、命令、能力表 |
| `HANDOVER.md` | 现状、架构、测试、债、路线 |
| `../SpellArchive 开发方案（Rust 版）v2.1.md` | 产品设计、安全规范、完整路线图 |
| `docs/theme_spec.md` | 主题 schema、ResKey、回退、可发现性 |
| `NOTICE.md` | 许可证边界、同人声明 |
| `LICENSE` | MIT 全文 |
