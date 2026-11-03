# SpellArchive 交接文档

> 面向接手的开发者。读完本文件应能：跑起来、看懂现有哪些是真实现、哪些是占位、按既定路线继续写。
> 最后更新：2026-11（骨架 + 主题/素材接入完成后）

## 0. 先读这三份

| 文档 | 作用 |
|---|---|
| `../SpellArchive 开发方案（Rust 版）v2.1.md` | 完整设计：安全、接口、路线图、风险 |
| `docs/theme_spec.md` | `theme.toml` 规范、资源 Key、回退规则 |
| `NOTICE.md` | 代码/素材/字体/同人授权边界 |

---

## 1. 当前状态一句话

**可运行的 GUI 骨架 + 已接入四套美术主题；归档后端（zip/7z/tar）尚未接入，解压/打包/列表都是占位。**

界面里的文件列表和任务队列是硬编码演示数据，不是真实归档内容。

---

## 2. 五分钟上手

### 环境

- Windows 10/11 x64
- Rust stable（当前验证 1.99.1；`rust-version = 1.85`，edition 2024）
- MSVC Build Tools（`zstd`/`bzip2` 将来要用，当前依赖已可编译）
- 素材源目录：`C:\Users\karl2\Desktop\SpellArchive\assets`（与工程同级）

> 工程目前**不是 git 仓库**。接手第一件事建议 `git init` 并提交当前基线。

### 命令

```powershell
cd C:\Users\karl2\Desktop\SpellArchive\spell_archive
cargo test --workspace          # 26 个测试
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p spell-app          # GUI
cargo run -p spell-cli -- sniff <文件>   # 格式嗅探（唯一可用的 CLI 功能）
```

### 验收“环境没问题”

1. GUI 能开，左上角是灵梦像素头像，工具栏按钮有图标。
2. 主题下拉框有 5 项：灵梦 / 魔理沙 / 早苗 / 琪露诺 / 素体。
3. `spell sniff` 对任意 zip 输出 `zip`。
4. 列表列头与数据行对齐，任务队列两条进度条等长。
5. `cargo test` 全绿、clippy 无告警。

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
| `writer` | 完成（契约） | `ArchiveWriter`，默认返回 `Unsupported` |
| `backend::sniff` | 完成 | magic bytes 优先，扩展名只区分 tar.gz / gz 等复合容器 |
| `backend::open` | 分派完成、实现为占位 | 会返回 `ZipReader` 等结构，但 `entries` 为空、`extract` 返回 `Unsupported` |
| `backend::name_codec` | 完成 | UTF-8 → GBK / Shift-JIS 择优，带测试 |
| `backend::volume` | 完成 | `.7z.001` 识别与收集，带测试 |
| `safety::path_sanitize` | **完成** | `..`、绝对路径、ADS、保留名、尾随点空格、深度/长度；带测试 |
| `safety::display_guard` | **完成** | U+202E 等 bidi/零宽字符替换为可见标记；带测试 |
| `safety::bomb::Budget` | **完成** | 总量/单条/条目数/流式倍率；带测试 |
| `safety::atomic_fs` | **完成** | 临时文件 + `ReplaceFileW` / `MoveFileExW`；带测试（失败不毁原文件） |
| `task::event` | 完成 | 事件枚举 + `CancelToken` |
| `task::manager` | 部分 | 内存任务表，**没有线程调度** |
| `task::worker::guarded` | 完成 | `catch_unwind` 转任务失败；带测试 |
| `secret` | 完成 | `Zeroizing<String>` + Debug 打码 |

### 3.2 `spell-app`

| 模块 | 状态 | 说明 |
|---|---|---|
| `main` | 完成 | 1280×820，原生标题栏 |
| `app` | 完成（含演示数据） | 状态、目录浏览、演示任务动画、待删除的 `demo_entries` / `demo_tasks` |
| `i18n` | 完成 | 内置 zh-CN/en-US + 主题覆盖 + “显示标准术语”开关 |
| `theme::loader` | 完成 | 扫描 `theme.toml`、解析、PNG 载入、路径逃逸校验；带测试 |
| `theme::mod` | 完成 | `ResKey`、`Palette`、`ThemeStore`、全局样式应用 |
| `ui::archive_view` | 完成（演示数据） | `egui_extras::TableBuilder` 虚拟化表格、图标、搜索、选中、标记列 |
| `ui::file_browser` | 完成（基础） | 本地目录进入/返回，隐藏 `.` 开头项，最多 300 项 |
| `ui::task_panel` | 完成（演示） | 固定列宽 + 符阵进度条 + 重播按钮 |
| `ui::spell_progress` | 完成 | 底图 + 填充贴图（UV 与矩形同时裁剪），缺图回退 10 段纯色 |
| `ui::settings` | 完成（基础） | 主题/语言/术语开关/限额展示；不持久化 |
| `ui::mod` | 完成 | 工具栏（头像 + 图标按钮）、状态栏、首启提示 |

### 3.3 未开始

文件对话框、右键菜单、托盘、单实例、拖拽、设置持久化、主题热重载/导入、
压缩创建、加密、分卷、包内预览、RAR、九宫格、SVG、粒子。

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

约 3300 行 Rust。

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
         └─ refresh_dir()            # 演示 entries / tasks
```

### 5.3 每帧

```text
App::ui(root_ui)
 ├─ tick(ctx)                     # 演示进度动画（仅演示）
 ├─ hint_banner → Panel::top
 ├─ top_bar     → Panel::top
 ├─ status_bar  → Panel::bottom
 ├─ file_browser→ Panel::left
 ├─ task_panel  → Panel::bottom
 ├─ archive_view→ CentralPanel
 └─ settings    → Window（需要 ctx，不是 root ui）
```

面板必须按“先 top/bottom，再 left/right，最后 CentralPanel”的顺序挂到同一个 root `Ui` 上。

### 5.4 后端接入后的目标流（尚未实现）

```text
打开文件
 └─ backend::open(path, password?)
     ├─ Err(PasswordRequired) → 密码框 → 重试
     └─ Ok(reader)
         └─ 快照 entries → 列表
             └─ 用户点解压
                 ├─ reader.plan(ids, target, limits) → 预检面板（冲突/炸弹/空间）
                 ├─ UI 填 DecisionTable
                 └─ task_manager.spawn(reader, plan, decisions, cancel, sink)
                     └─ TaskEvent → UI（30fps 节流）
```

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
| 1 | 没有 `LICENSE` 文件 | NOTICE 引用了它，发布不合规 | 补 MIT 全文 |
| 2 | `demo_entries` / `demo_tasks` 硬编码在 `app.rs` | 与真实功能混淆 | 接后端时删除 |
| 3 | 列表/任务无真实数据源 | 无法验收真实解压 | 先接 zip |
| 4 | `task_panel` 每帧 `clone()` 整个 Theme（含纹理 HashMap） | 轻微浪费 | `Arc<Theme>` 或只克隆纹理 id |
| 5 | 进度条底图被拉伸到任意宽高 | 像素素材非整数缩放，边缘不完美 | 按 `theme.scale` 吸附到整数倍 |
| 6 | 主题加载全部 4 套（约 96 个纹理） | 启动内存/时间偏高 | 懒加载，切换时再解码 |
| 7 | 搜索只做小写子串；排序是字典序 | 无自然序（file10 在 file2 前） | 实现 natural sort |
| 8 | 无 SVG 支持 | plain 主题暂时也用纯色 | 需要时开 `egui_extras` svg |
| 9 | `[resource_nine_patch]` 解析了但未使用 | 面板背景还不能九宫格 | 实现 `ui/nine_patch.rs` |
| 10 | 无日志 / 崩溃落盘 | 用户报错难定位 | 接 `tracing` + panic hook |
| 11 | 无设置持久化 | 每次启动重置 | `settings.toml` + schema 迁移 |
| 12 | 无单实例 / 右键 / 托盘 | 不像桌面工具 | v0.1 后半段补 |
| 13 | `themes/*/png` 是复制品 | 素材更新需跑同步脚本 | 已有 `scripts/sync-themes.ps1` |
| 14 | 未初始化 git | 无历史可回滚 | 立刻 `git init` |
| 15 | 无 `build.rs`（图标/版本/manifest） | exe 无图标、长路径未声明 | v0.1 发布前补 |

---

## 10. 下一步路线（按依赖顺序）

### P0-1 接 zip 后端（1–2 天）

文件：`crates/spell-core/src/backend/zip.rs`，依赖加 `zip = { version = "4", features = ["aes-crypto"] }`（版本以 crates.io 为准）。

实现要点：

1. `ZipReader::new`：
   - `BufReader::with_capacity(256*1024, File::open(path))`
   - `ZipArchive::new`
   - 遍历 `0..len()`，用 `by_index_raw(i)` 只读元数据（不解压）
   - 名字：若 UTF-8 标志位 → `decode_strict_utf8`；否则 `name_codec::decode`
   - `display = display_guard(...).text`；`kind` 由 `is_dir()` 判定
2. `plan()`：
   - 对每个选中 id 调 `sanitize_rel(display)` + `resolve_under(target)`
   - 检查目标是否已存在 → `Conflict`
   - 累加 `total_bytes`、记录 `largest_entry`
   - `free_space` 用 `fs2::available_space`
3. `extract()`：
   - 用 `by_index_decrypt` 打开条目（密码错误映射 `BadPassword`）
   - `sink.on_entry_begin` → 循环 `read`（64 KiB 缓冲）→ `sink.on_data` → `on_entry_end`
   - 每次 `on_data` 前 `cancel.is_cancelled()`
   - 单条失败记入 `ExtractReport.failed` 继续下一条
4. `WriteSink`（放 `spell-core/src/safety/safe_extract.rs`）：
   - `on_data` 里调用 `Budget::on_write`（熔断）
   - 写盘用 `write_atomic`（覆盖安全）+ `path_sanitize`
   - CRC 校验用 `crc32fast`
5. 测试：`tests/backend_roundtrip.rs` 用 `zip` crate 现场造包；安全用例见 `docs/security` 清单。

验收：`spell-cli list` 能列出条目；`spell-cli extract` 能安全解出文件；`cargo test` 新增往返 + 路径攻击用例。

### P0-2 真实任务队列（1–2 天）

- `task/manager.rs`：加 `crossbeam-channel` + 1~2 条 heavy worker。
- `TaskTable::spawn(kind, payload)`；worker 内 `guarded(|| ...)`。
- `TaskEvent` → UI：`try_recv` 批量取，合并进度，`ctx.request_repaint()` 节流 30fps。
- 删除 `demo_tasks` 与 `tick()` 里的演示动画。

验收：解压 1 GiB 包时 UI 不卡；取消在 200ms 内生效；任务失败有具体条目。

### P0-3 打开归档 + 预检（1 天）

- 加 `rfd`，工具栏“打开归档”接文件对话框。
- `backend::open` → `entries` 快照 → 表格；删除 `demo_entries`。
- 预检弹窗：条目数 / 总大小 / 最大单文件 / 冲突 / 密码 / 剩余空间。

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

- [ ] `cargo test --workspace`（26 通过）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo run -p spell-app` 出界面，5 个主题可切换，图标/头像/进度条正常
- [ ] `cargo run -p spell-cli -- sniff <zip>` 输出 `zip`
- [ ] 读过 `docs/theme_spec.md` 与设计文档 v2.1
- [ ] 知道 `demo_entries` / `demo_tasks` 是假数据
- [ ] 知道 zip/7z/tar 后端是占位（`list` 返回 0 条）
- [ ] 确认 `spell-core` 依赖树无 egui
- [ ] 补 `LICENSE`（MIT）并 `git init`

---

## 12. 开发约定与坑

1. **文件编码**：用编辑器/专用工具写 UTF-8。不要在 Windows PowerShell 5.1 里
   `Get-Content -Raw` + `Set-Content` 往返改源码——本项目已经因此产生过一次中文乱码，
   而且会吃掉换行。
2. **不要开 `panic = "abort"`**（release profile 已固定 unwind）。
3. **不要用文件名定位条目**，用 `EntryId`。
4. **不要绕过 `safety::path_sanitize` / `write_atomic`**。
5. **不要给 `spell-core` 加 UI 依赖**。
6. **压缩不要用 rayon 全局池**；rayon 只做扫描、分类、将来的 7z 分块编码。
7. **时间语义**：zip = 本地时间（无时区），7z = UTC；混用会漂移。
8. **空目录**：zip 需要显式 `dir/` 条目，否则解压丢结构。
9. **FAT32** 单文件 4 GiB 上限，写失败要给换文件系统的建议。
10. **符号链接**：Windows 创建需特权/开发者模式，默认跳过是正确行为。
11. **主题文案**：可以改措辞，但 tooltip 必须给标准术语，且不能隐藏错误信息。
12. **素材许可证**：代码 MIT，内置素材 CC BY-NC-SA 4.0；商用需替换素材。

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
