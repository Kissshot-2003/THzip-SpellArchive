# SpellArchive 交接文档

> 面向接手的开发者。读完本文件应能：跑起来、看懂现有哪些是真实现、哪些是占位、按既定路线继续写。
> 最后更新：2026-11（P0-2 任务队列 / P0-3 打开与预检 / P0-4 zip 创建、追加与 AES 加密完成）

## 0. 先读这三份

| 文档 | 作用 |
|---|---|
| `../SpellArchive 开发方案（Rust 版）v2.1.md` | 完整设计：安全、接口、路线图、风险 |
| `docs/theme_spec.md` | `theme.toml` 规范、资源 Key、回退规则 |
| `NOTICE.md` | 代码/素材/字体/同人授权边界 |

---

## 1. 当前状态一句话

**zip 已能在 GUI 里完成「打开 → 看条目 → 预检 → 解压 / 测试 / 取消」以及「选文件/文件夹 → 创建 / 追加 / AES 加密 zip」；后台任务线程池、密码重试、覆盖策略已接通。7z/tar 仍是占位。**

已删除全部演示数据。启动参数 `spell_archive.exe <归档>` 会直接打开文件（Windows「打开方式」可用）。

---

## 2. 五分钟上手

### 环境

- Windows 10/11 x64
- Rust stable（当前验证 1.99.1；`rust-version = 1.85`，edition 2024）
- MSVC Build Tools（`zstd`/`bzip2` 将来要用，当前依赖已可编译）
- 素材源目录：`C:\Users\karl2\Desktop\SpellArchive\assets`（与工程同级）

> 工程已有 git 仓库（初始提交 `36842a2`）。当前批次改动尚未提交，接手后先确认 `git status`。

### 命令

```powershell
cd C:\Users\karl2\Desktop\SpellArchive\spell_archive
cargo test --workspace          # 47 个测试
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p spell-app          # GUI
cargo run -p spell-app -- <归档>  # 启动并直接打开
cargo run -p spell-cli -- sniff <文件>            # 格式嗅探
cargo run -p spell-cli -- list <zip>              # 列出条目
cargo run -p spell-cli -- test <zip>              # CRC 完整性测试
cargo run -p spell-cli -- extract <zip> <目录>     # 安全解压（--force 覆盖）
cargo run -p spell-cli -- pack <输出.zip> <输入...>  # 创建/追加（--append / --store / --force / --password）
```

### 验收“环境没问题”

1. GUI 能开，左上角是灵梦像素头像，工具栏按钮有图标。
2. 主题下拉框有 5 项：灵梦 / 魔理沙 / 早苗 / 琪露诺 / 素体。
3. `spell sniff` 对任意 zip 输出 `zip`。
4. 打开任意 zip 后条目真实显示；点「解除符卡封印」可选目录并弹出预检；运行中任务有取消按钮。
5. 点「构筑封印结界」能添加文件/文件夹、创建 zip，任务队列显示压缩前后大小。
6. 试一次追加：对已存在的 zip 勾选「追加到现有 zip」，原条目应保留。
7. 试一次加密：勾选 AES-256 设密码创建，再用密码解压应无 CRC 误报。
8. `cargo test` 全绿（47）、clippy 无告警。

---

## 3. 完成度矩阵

### 3.1 `spell-core`（零 GUI 依赖）

| 模块 | 状态 | 说明 |
|---|---|---|
| `types` | 完成 | `EntryId` / `ArchiveEntry`（原始字节名 + display）/ `Method` / `ArchiveKind` |
| `error` | 完成 | 逐条错误、`PasswordRequired`、`LimitExceeded`、`Cancelled` |
| `limits` | 完成 | 严格/默认/宽松三档 |
| `plan` | 完成 | `ExtractPlan` / `PlannedItem` / `PlanAction` / `DecisionTable` |
| `reader` | 完成（契约） | `ArchiveReader::{plan, extract, test}` + `ExtractSink` |
| `writer` | **完成** | `ArchiveWriter` + 目录扫描 `scan_inputs` + 批量写入 `write_entries`；跳过链接、拒绝重名 |
| `backend::zip_write` | **完成** | Store / Deflate、目录条目、zip64、本地 DOS 时间、`AtomicFile` 原子替换；带测试 |
| `backend::sniff` | 完成 | magic bytes 优先，扩展名只区分 tar.gz / gz 等复合容器 |
| `backend::zip` | **完成（读）** | 中央目录解析、名字编码还原、plan 预检、流式解压、`test()` / CRC；AES 解压（跳过 AES CRC=0 误报）；同名 `~1` 改名 |
| `backend::open` | zip 已接实现 | 其余格式（7z/tar/单文件流）仍是占位 |
| `backend::name_codec` | 完成 | UTF-8 → GBK / Shift-JIS 择优，带测试 |
| `backend::volume` | 完成 | `.7z.001` 识别与收集，带测试 |
| `safety::path_sanitize` | **完成** | `..`、绝对路径、ADS、保留名、尾随点空格、深度/长度；带测试 |
| `safety::display_guard` | **完成** | U+202E 等 bidi/零宽字符替换为可见标记；带测试 |
| `safety::bomb::Budget` | **完成** | 总量/单条/条目数/流式倍率；带测试 |
| `safety::atomic_fs` | **完成** | `AtomicFile` 流式写 + `ReplaceFileW` / `MoveFileExW`、reparse point 检测；带测试 |
| `safety::safe_extract` | **完成** | `ExtractDirSink`：唯一写盘入口，预算/CRC/原子覆盖/junction 拒绝 |
| `safety::verify_sink` | **完成** | 只算 CRC 不落盘，`test()` 与 CLI 共用 |
| `safety::disk` | 完成 | `available_space`（GetDiskFreeSpaceExW） |
| `plan::resolve_all` | 完成 | 决策表 → 最终动作；后端与 sink 共用，保证路径一致 |
| `task::event` | 完成 | 事件枚举（含 `Opened` / `NeedPassword`）+ `CancelToken` |
| `task::runner` | **完成** | 2 条后台 worker、`crossbeam-channel` 双向通道、进度节流、取消、panic 兜底；带集成测试 |
| `task::manager` | 完成（备用） | 内存任务表；GUI 实际使用 `runner` 的事件流 |
| `task::worker::guarded` | 完成 | `catch_unwind` 转任务失败；带测试 |
| `secret` | 完成 | `Zeroizing<String>` + Debug 打码 |

### 3.2 `spell-app`

| 模块 | 状态 | 说明 |
|---|---|---|
| `main` | 完成 | 1280×820，原生标题栏；支持 `<归档>` 启动参数 |
| `app` | **完成（真实数据）** | 归档打开、事件泵、任务表、密码重试、解压预检、速度估算；无演示代码 |
| `i18n` | 完成 | 内置 zh-CN/en-US + 主题覆盖 + “显示标准术语”开关 |
| `theme::loader` | 完成 | 扫描 `theme.toml`、解析、PNG 载入、路径逃逸校验；带测试 |
| `theme::mod` | 完成 | `ResKey`、`Palette`、`ThemeStore`、全局样式应用 |
| `ui::archive_view` | **完成（真实数据）** | `TableBuilder` 虚拟化表格、真实 mtime、图标、搜索、选中、空状态打开按钮 |
| `ui::file_browser` | 完成（基础） | 本地目录进入/返回；单击嗅探格式，双击打开归档 |
| `ui::task_panel` | **完成（真实任务）** | 固定列宽 + 符阵进度条 + 速度 + 取消 + 清除已完成 |
| `ui::dialogs` | **完成** | 解压预检、密码重试、创建 zip（添加文件/文件夹、输出路径、仅存储、覆盖确认） |
| `ui::spell_progress` | 完成 | 底图 + 填充贴图（UV 与矩形同时裁剪），缺图回退 10 段纯色 |
| `ui::settings` | 完成（基础） | 主题/语言/术语开关/限额展示；不持久化 |
| `ui::mod` | 完成 | 工具栏（头像 + 图标按钮）、状态栏、首启提示 |

### 3.3 未开始

右键菜单、托盘、单实例、拖拽、设置持久化、主题热重载/导入、
7z/tar 创建、7z 加密写入、分卷、包内预览、RAR、九宫格、SVG、粒子。

---

## 4. 目录地图

```text
spell_archive/
├── Cargo.toml                 # workspace（resolver 3，edition 2024，release: lto=thin, panic=unwind）
├── rust-toolchain.toml
├── README.md / NOTICE.md      # 缺 LICENSE（MIT 文本待补）
├── docs/theme_spec.md
├── scripts/sync-themes.ps1    # 从 ../assets 重新同步 PNG 到各主题
├── themes/{reimu,marisa,sanae,cirno}/
│   ├── theme.toml             # schema=1
│   └── png/                   # 图标 16 + 头像 + 进度底图/填充
├── crates/
│   ├── spell-core/            # 无 egui / winit 依赖（Cargo 层面强制）
│   ├── spell-app/             # egui UI + 主题 + 平台（平台尚未实现）
│   └── spell-cli/             # 复用 core；目前只有 sniff 真正可用
└── target/
```

约 7400 行 Rust（含测试）。

---

## 5. 架构与调用流

### 5.1 分层规则（不要破坏）

1. `spell-core` **不得**出现 `egui` / `eframe` / UI 概念。验证：
   `cargo tree -p spell-core | Select-String egui` 必须为空。
2. UI 永远不直接调具体压缩库，只通过 `ArchiveReader` / `ArchiveWriter`。
3. 所有写盘入口必须走 `safety::*`：解压、拖拽解压、主题包导入、CLI。
4. 重活全部在后台线程；UI 线程只收 `TaskEvent`。

### 5.2 当前启动顺序

```text
main
 └─ run_native
     └─ SpellApp::new
         ├─ install_cjk_fonts        # simhei.ttf → 回退 Deng.ttf
         ├─ ThemeStore::load(ctx)    # 扫描 + 解析 + 载入 PNG 纹理
         ├─ themes.apply(ctx)        # ThemePreference::Dark + all_styles_mut
         ├─ TaskRunner::new(2)       # 两条后台 worker + 双向通道
         ├─ refresh_dir()            # 本地文件列表
         └─ open_path(arg)           # 有启动参数时立即提交 Open 任务
```

### 5.3 每帧

```text
App::ui(root_ui)
 ├─ tick()                        # 排空 TaskEvent + 速度估算
 ├─ hint_banner → Panel::top
 ├─ top_bar     → Panel::top
 ├─ status_bar  → Panel::bottom
 ├─ file_browser→ Panel::left
 ├─ task_panel  → Panel::bottom
 ├─ archive_view→ CentralPanel
 ├─ dialogs     → Window（预检 / 密码；需要 ctx）
 └─ settings    → Window（需要 ctx，不是 root ui）
```

有活动任务时末尾 `request_repaint_after(33ms)`，空闲时完全不请求重绘。

面板必须按“先 top/bottom，再 left/right，最后 CentralPanel”的顺序挂到同一个 root `Ui` 上。

### 5.4 当前真实数据流（已实现）

```text
打开文件（按钮 / 启动参数 / 左侧双击）
 └─ TaskRunner::submit(TaskOp::Open)
     └─ worker: backend::open(path, password?)
         ├─ Err(PasswordRequired) → TaskEvent::NeedPassword → 密码框 → 重新提交
         └─ Ok(reader) → TaskEvent::Opened { entries, Arc<Mutex<reader>> }
             └─ UI 快照 entries → 表格
                 └─ 用户点解压
                     ├─ rfd 选目标目录
                     ├─ reader.try_lock().plan(ids, target, limits) → 预检弹窗
                     ├─ UI 选覆盖策略 → DecisionTable
                     └─ TaskRunner::submit(TaskOp::Extract { reader, plan, decisions, limits })
                         ├─ worker: ExtractDirSink + ProgressSink
                         └─ TaskEvent::{Started, Progress, Finished, Failed} → UI
```

`Arc<Mutex<Box<dyn ArchiveReader>>>` 让 UI 与 worker 共享同一个 reader；UI 侧预检必须
`try_lock`，避免解压进行中把界面卡死。测试任务同理（`TaskOp::Test` + `VerifySink`）。

创建 zip：

```text
打包弹窗（添加文件 / 添加文件夹 / 输出路径 / 仅存储）
 └─ TaskRunner::submit(TaskOp::Pack { output, inputs, method })
     └─ worker: scan_inputs → create_writer(zip_write) → write_entries
         ├─ Progress → UI
         └─ PackFinished { report } / Failed
```

`scan_inputs` 与 `write_entries` 都返回普通错误；`write_entries` 的进度是**全部输入累计字节**。

---

## 6. 核心接口速查

### 6.1 条目

```rust
pub type EntryId = u32;                 // entries 数组下标，同一 open 内稳定

pub struct ArchiveEntry {
    pub raw_name: Arc<[u8]>,            // 落盘以它为准
    pub display: Arc<str>,              // 仅 UI；应经 name_codec + display_guard
    pub kind: EntryKind,
    pub size: u64,
    pub packed: Option<u64>,            // 固实 7z 为 None
    pub crc32: Option<u32>,
    pub mtime: Option<SystemTime>,       // 7z=UTC，zip=本地时间
    pub encrypted: bool,
    pub method: Method,
}
```

**禁止按文件名选条目**：zip 允许重名。一律用 `EntryId`。

### 6.2 解压契约

```rust
trait ArchiveReader {
    fn kind(&self) -> ArchiveKind;
    fn entries(&self) -> &[ArchiveEntry];
    fn plan(&self, ids, target, limits) -> Result<ExtractPlan, ArchiveError>;
    fn extract(&mut self, plan, decisions, cancel, sink) -> Result<ExtractReport, ArchiveError>;
    fn test(&mut self, cancel, sink) -> Result<ExtractReport, ArchiveError>;
}
```

`ExtractSink::on_data` 是**唯一**写盘/计数点；膨胀熔断、进度、CRC 都挂这里。

### 6.3 硬约束（写代码前先看）

| 约束 | 原因 |
|---|---|
| 覆盖已有文件必须 `safety::write_atomic` | 直接 `File::create` 失败会截断用户原文件 |
| 解压前必须 `plan()`，UI 先解决冲突 | worker 不能同步询问用户 |
| 固实包单条提取也要顺序解码 | 进度按“已处理压缩字节”算，否则长时间 0% |
| worker 用 `task::worker::guarded` | 畸形包 panic 不能带崩进程 |
| release 不能设 `panic = "abort"` | 会让 `catch_unwind` 失效 |
| 压缩/解压不进 rayon 全局池 | 嵌套并行 + 内存峰值失控 |
| `spell-core` 不新增 UI 依赖 | 用 `cargo tree` 守门 |

---

## 7. GUI 关键实现细节

### 7.1 egui 0.37 是 Ui-first，网上教程大多过时

| 旧写法 | 0.37 写法 |
|---|---|
| `impl App { fn update(&mut self, ctx, frame) }` | `fn ui(&mut self, ui: &mut egui::Ui, frame)` |
| `eframe::run_native` 内部无感 | 一样，但根是 `Ui` |
| `egui::TopBottomPanel::top("x").show(ctx, …)` | `egui::Panel::top("x").show(ui, …)` |
| `egui::SidePanel::left("x")` | `egui::Panel::left("x")` |
| `CentralPanel::show(ctx, …)` | `CentralPanel::show(ui, …)` |
| `.default_height / .min_height` | `.default_size / .min_size / .exact_size` |
| `ctx.set_style(style)` | `ctx.all_styles_mut(\|style\| { … })` |
| `ctx.set_visuals(v)` | `ctx.set_visuals_of(Theme::Dark, v)` 或 `all_styles_mut` |
| `Window::show(ctx, …)` | 仍是 `ctx` |

### 7.2 表头/行对齐

用 `egui_extras::TableBuilder`，**不要手工 `add_sized` 拼表头**（这正是上一版错位的原因）。
列宽：名称 280（可裁切）、大小 84、压缩后 84、比率 60、方法 72、修改时间 110、标记 remainder。

### 7.3 任务行

固定列宽：名称 240 / 速度 96 / 状态 120 + 3 段间距，`bar_width` 每帧算一次后所有行共用。
名称与状态都加 `.truncate()`，否则长文案会把进度条挤短（上一版长度不一致的原因）。

### 7.4 符阵进度条

```rust
// 关键：uv 和目标矩形必须同时按比例裁剪
let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(lit, 1.0));
let fill_rect = Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * lit, rect.max.y));
painter.add(Shape::image(fill.id(), fill_rect, uv, WHITE));
```

只裁 uv 会把左侧 52% 拉伸到整条。逐格模式：`lit = round(ratio*10)/10`。

### 7.5 字体

`install_cjk_fonts` 读 `C:/Windows/Fonts/simhei.ttf`（回退 `Deng.ttf`）。
egui 默认无 CJK 字形，不装字体就是豆腐块。
**待办**：换成 `fontdb` 查找 + 内嵌 Noto Sans SC 子集（OFL），供 Linux 构建兜底。

---

## 8. 主题与素材

### 8.1 数据流

```text
../assets/  (原始素材，作者维护)
   └─ scripts/sync-themes.ps1
        └─ themes/<skin>/png/   (运行时实际读取)
             └─ theme.toml 的 [resource] key → 路径
                  └─ ResKey → HashMap<ResKey, TextureHandle>
                       └─ UI
```

素材源与主题目录是**两份**：改素材后跑一次 `.\scripts\sync-themes.ps1`，`theme.toml` 不会被覆盖。

注意：`assets/tokens/tokens.json` 里的文件名（`arc_zip.png`、`panel/…`、`particles/…`）多数并不存在；
实际可用的是 `icons/icon-*.png`、`avatars/*-256.png`、`progress/*.png`。主题已按真实文件名映射。
`assets/README.md` 也说明了 PanelBg / 粒子 / 失败态进度图尚未提供——这些 Key 在主题里故意缺失，走回退。

### 8.2 新增一个主题

1. 复制 `themes/reimu/` 为 `themes/<新 id>/`。
2. 改 `theme.toml`：`[meta]`、`[color]`、`[resource]` 里的填充图/头像。
3. 不需要改 Rust。启动自动扫描；`loader` 里 `rank()` 只影响排序，未知 id 排最后。
4. 缺资源会回退内置主题，不会崩；`[resource]` 里写错 Key 只 warn。

### 8.3 新增一个资源 Key

改四处，缺一不可：

1. `docs/theme_spec.md` 的 Key 表（唯一真源）
2. `spell-app/src/theme/mod.rs` 的 `ResKey` 枚举 + `ALL` + `name()`
3. 各主题 `theme.toml` 的 `[resource]`
4. 使用处

将来应加 `xtask check-theme-keys` 自动比对（见路线图）。

### 8.4 Loader 行为

- 主题根目录查找顺序：`./themes` → `<exe>/themes` → `<exe>/../../themes`（开发路径）。
- 路径必须相对、无 `..`，`canonicalize` 后仍在主题目录内。
- `sampling_pixel = true` → `TextureOptions::NEAREST`；否则 LINEAR。
- `schema > 1` 只警告，不 panic。
- 目前**未**执行规格里的 8 MiB/64 MiB/500 文件限制，只有路径安全；接入主题包导入前必须补。

---

## 9. 已知问题 / 技术债

| # | 问题 | 影响 | 建议 |
|---|---|---|---|
| 1 | `task_panel` 每帧 `clone()` 整个 Theme（含纹理 HashMap） | 轻微浪费 | `Arc<Theme>` 或只克隆纹理 id |
| 2 | 解压冲突的 `Rename/AutoNumber` 策略没有真正改名 | 只能选跳过 / 覆盖 | 在 `resolve_all` 里对已存在文件生成 `~n` |
| 3 | 预检在 UI 线程执行（大目录/慢盘会短暂卡顿） | 大量冲突时体感差 | 改成 `TaskOp::Plan` 后台任务 |
| 4 | 单条选择只能解压一个条目，没有多选 / 全选反选 | 大批量操作不顺手 | 表格加多选集合 |
| 5 | 进度条底图被拉伸到任意宽高 | 像素素材非整数缩放，边缘不完美 | 按 `theme.scale` 吸附到整数倍 |
| 6 | 主题加载全部 4 套（约 96 个纹理） | 启动内存/时间偏高 | 懒加载，切换时再解码 |
| 7 | 搜索只做小写子串；排序是字典序 | 无自然序（file10 在 file2 前） | 实现 natural sort |
| 8 | 无 SVG 支持 | plain 主题暂时也用纯色 | 需要时开 `egui_extras` svg |
| 9 | `[resource_nine_patch]` 解析了但未使用 | 面板背景还不能九宫格 | 实现 `ui/nine_patch.rs` |
| 10 | 无日志 / 崩溃落盘 | 用户报错难定位 | 接 `tracing` + panic hook |
| 11 | 无设置持久化 | 每次启动重置 | `settings.toml` + schema 迁移 |
| 12 | 无单实例 / 右键 / 托盘 | 不像桌面工具 | v0.1 后半段补 |
| 13 | `themes/*/png` 是复制品 | 素材更新需跑同步脚本 | 已有 `scripts/sync-themes.ps1` |
| 14 | 当前批次改动未提交 | 无回滚点 | 确认测试后提交（不要混入临时文件） |
| 15 | 无 `build.rs`（图标/版本/manifest） | exe 无图标、长路径未声明 | v0.1 发布前补 |
| 16 | zip 创建只支持 Store / Deflate | 不能选 Deflate64 / bzip2 / zstd 等 | 按需启用 zip features 并补 `map_method` |
| 17 | 追加会完整复制原包 | 大包耗时、临时空间翻倍 | 需要增量时再研究原地追加（风险高，不建议） |
| 18 | 打包扫描跳过符号链接 | 链接内容不会进包 | 需要时提供“跟随链接”选项并做环检测 |
| 19 | 追加模式不允许设置新密码 | 不能给旧包批量改密 | 先解包再重建，或后续实现重写加密 |

---

## 10. 下一步路线（按依赖顺序）

### ✅ P0-1 zip 后端（已完成）

已实现于 `crates/spell-core/src/backend/zip.rs` + `safety/safe_extract.rs`：

- `zip = 4.6`（`deflate` / `aes-crypto`），名字用 `name_raw()` + `name_codec` 还原（不吃 CP437）。
- `plan()`：`sanitize_rel` → `resolve_under`，冲突/重解析点/超长/链接/可疑显示名全部进 `ExtractPlan`。
- `extract()`：`resolve_all` 统一决策 → 64 KiB 流式读取 → `ExtractDirSink`（Budget + CRC + AtomicFile）。
- `test()`：`VerifySink` 只算 CRC，不落盘。
- CLI：`list` / `test` / `extract [目标] [--force] [--password]`。
- 测试：`crates/spell-core/tests/zip_backend.rs`（往返、穿越、绝对路径、大小写冲突、Skip/Overwrite、条目上限、CRC）。

**尚未做**：bzip2/zstd/lzma 压缩方法的解码（当前未启用对应 feature，遇到会报 `Unsupported`）。

### ✅ P0-2 真实任务队列（已完成）

- `task/runner.rs`：`crossbeam-channel` + 2 条 worker；`TaskOp::{Open, Extract, Test, Pack}`。
- worker 内 `guarded()`；事件 `try_recv` 批量排空，`ProgressSink` 按 100ms / 1 MiB 节流。
- 取消按钮写入 `CancelToken`；每帧活动任务 `request_repaint_after(33ms)`。
- 集成测试：`crates/spell-core/tests/task_runner.rs`（打开→计划→解压、失败事件）。

### ✅ P0-3 打开归档 + 预检（已完成）

- `rfd` 文件 / 目录对话框；启动参数直接打开。
- `TaskEvent::Opened` 携带 `Arc<Mutex<Box<dyn ArchiveReader>>>`，条目快照进表格。
- 预检弹窗展示条目数 / 总大小 / 最大单文件 / 冲突 / 警告 / 剩余空间 / 加密提示，
  可选「跳过已有 / 覆盖已有」，再提交 `TaskOp::Extract`。
- `NeedPassword` → 密码弹窗 → 重新提交 `TaskOp::Open`。

### ✅ P0-4 zip 创建与追加（已完成）

- `backend/zip_write.rs`：`ZipArchiveWriter` 接 `zip::ZipWriter<AtomicFile>`（给 `AtomicFile` 实现了 `Write/Seek`）。
- `writer::scan_inputs`：目录递归、空目录条目、跳过符号链接、大小写重名拒绝。
- `writer::write_entries`：统一进度（累计字节），`ArchiveWriter::add` 内 64 KiB 流式写。
- Store / Deflate；zip64 大文件；本地 DOS 时间（Windows 走 `FileTimeToLocalFileTime`）。
- 取消 / 出错时 `AtomicFile` 自动删临时文件，原目标保持不变。
- CLI `pack`、GUI 创建 zip 弹窗、`tests/zip_write.rs` + `task_runner::packs_through_worker`。
- 追加模式：`AtomicFile::create_copy_of` 复制原包 → `ZipWriter::new_append` → 追加后原子替换；
  保留原条目并拒绝大小写重名。
- 加密写入：`with_aes_encryption(AesMode::Aes256, …)`；密码走 `SecretString`，CLI `--password`、
  GUI 勾选 AES-256。追加模式暂不允许设置新密码。
- 读取 AES 条目时 CRC 字段为 0，不能参与 CRC 比较；`test()` / 解压依赖 AE-2 认证码。

**下一步建议**：7z 后端（P1-1），或补充 GUI 设置持久化 / 桌面集成。

### P1-1 7z 后端（3–5 天）

- `sevenz-rust2`，AES-256、固实、`-mhe=on`（列表也要密码 → `PasswordRequired`）。
- 固实包单条提取需顺序解码；进度按压缩字节。
- 先跑通解压，再做创建（并行分块编码是 M0 风险项）。

### P1-2 tar 家族（2–3 天）

`tar` + flate2 / bzip2 / lzma-rs / zstd / lz4_flex；处理 pax、GNU longname、hardlink、base-256 长度。
复合容器嗅探：先解压前 1 KiB 判断内层是否 tar，否则按单文件流处理。

### P1-3 桌面集成（2–3 天）

`rfd`、`single-instance`、`tray-icon`、`windows-registry`（HKCU 右键）、`notify`（主题热重载）。
Win11 注册表菜单只会进“显示更多选项”，README 先说明。

### P2

主题包导入（复用 `safe_extract`）、九宫格、SVG、设置持久化、`tracing`、
`xtask check-theme-keys`、CLI 完善、RAR 可选特性。

---

## 11. 交接检查清单

接手当天逐条勾：

- [ ] `cargo test --workspace`（47 通过）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo run -p spell-app -- <zip>` 直接打开归档，条目、时间、图标正常
- [ ] 试一次解压：选目录 → 预检 → 开始 → 进度/速度/完成状态正确
- [ ] 试一次取消：大包解压中点 ×，任务在数百毫秒内失败为「已取消」
- [ ] 试一次创建：添加文件夹 → 输出 zip → 完成后用 `spell list` 验证条目
- [ ] `cargo run -p spell-cli -- sniff <zip>` 输出 `zip`
- [ ] 读过 `docs/theme_spec.md` 与设计文档 v2.1
- [ ] 知道 7z/tar 后端仍是占位；zip 读写已可用（GUI + CLI）
- [ ] 确认 `spell-core` 依赖树无 egui
- [ ] `LICENSE` 已补；确认是否提交当前批次改动

---

## 12. 开发约定与坑

1. **文件编码**：用编辑器/专用工具写 UTF-8。不要在 Windows PowerShell 5.1 里
   `Get-Content -Raw` + `Set-Content` 往返改源码——本项目已经因此产生过一次中文乱码，
   而且会吃掉换行。
2. **不要开 `panic = "abort"`**（release profile 已固定 unwind）。
3. **不要用文件名定位条目**，用 `EntryId`。
4. **不要绕过 `safety::path_sanitize` / `write_atomic`**。创建归档也只能走
   `ArchiveWriter`（内部已包 `AtomicFile`），不要直接 `File::create` 输出目标。
5. **不要给 `spell-core` 加 UI 依赖**。
6. **压缩不要用 rayon 全局池**；rayon 只做扫描、分类、将来的 7z 分块编码。
7. **时间语义**：zip = 本地时间（无时区），7z = UTC；混用会漂移。
8. **空目录**：zip 需要显式 `dir/` 条目，否则解压丢结构。
9. **FAT32** 单文件 4 GiB 上限，写失败要给换文件系统的建议。
10. **符号链接**：Windows 创建需特权/开发者模式，默认跳过是正确行为。
11. **主题文案**：可以改措辞，但 tooltip 必须给标准术语，且不能隐藏错误信息。
12. **素材许可证**：代码 MIT，内置素材 CC BY-NC-SA 4.0；商用需替换素材。
13. **打包扫描**：目录递归默认跳过符号链接；归档内名字统一由
    `writer::normalize_entry_name` 净化，用户输入不能直接进 zip。

---

## 13. 关键决策记录（简短 ADR）

| 决策 | 结论 | 原因 |
|---|---|---|
| GUI | egui + eframe 0.37 | 自绘像素 UI、单二进制、跨平台 |
| 7z | sevenz-rust2（纯 Rust） | 7z.dll 无 C ABI，需 COM vtable + 分架构 DLL |
| 通用解压 | zip / tar / flate2 / zstd / lz4_flex / lzma-rs | libarchive 是 C 依赖，破坏单 exe |
| 解压接口 | 两阶段 plan → decisions → extract | worker 无法同步询问 UI |
| 覆盖 | 临时文件 + ReplaceFileW | 直接写会截断原文件 |
| 文案 | i18n 与主题解耦 | 换素体主题不能丢中文 |
| 表格 | egui_extras TableBuilder | 手工拼列必然错位 |
| 工程结构 | workspace，core 零 UI 依赖 | 用 Cargo 强制分层 |
| 平台 | Windows 10+ / x64 | Win7 需要旧工具链，不值得 |

---

## 14. 文档索引

| 文件 | 内容 |
|---|---|
| `README.md` | 快速开始、模块状态 |
| `../SpellArchive 开发方案（Rust 版）v2.1.md` | 完整设计、安全规范、路线图、风险表 |
| `docs/theme_spec.md` | 主题 schema、ResKey、回退、可发现性规则 |
| `NOTICE.md` | 许可证边界、同人声明 |
| 本文件 | 交接、现状、下一步、坑 |
