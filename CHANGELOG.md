# 更新日志

本文件格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- **日语界面**：新增第三种界面语言 `日本語`（语言循环：简体中文 → English → 日本語），
  覆盖全部 251 条界面文案、结构化错误原因与预检警告；占位符一致性由测试守护。

### 变更

- **7z 默认级别 6 → 3**（`Fast` + `Hc4` match finder）。文本数据打包提速约 4 倍，
  随机数据持平，压缩率仅降约 9%；同压缩级别口径下创建 7z 从「慢 4 倍」变为
  快于 7-Zip 26.04（1.34s ↔ 3.14s，2.35x）。追求极致压缩率可用 `--7z-level 6–9`。
  此前「创建 7z 慢」的结论来自不对等口径（L6 ↔ `-mx5`），基准已统一为 L3 ↔ `-mx3`。

## [0.2.1] - 2026-11-16

### 新增

- `main` 分支保护：改动必须走 pull request，`check` / `supply-chain` /
  `differential` / `long-path` 四个 CI 检查全绿才能合并；强推与删分支被禁；
  提交历史保持线性。详见 `CONTRIBUTING.md` 的「提交流程」。
- zip 加密条目现在支持**任意压缩方法组合**（含 AES+LZMA / AES+XZ）。
  WinZip AES 规范把加密条目的 compression method 记作 99、真实方法放在
  0x9901 extra field 里；zip crate 9.x 解析时已用真实方法覆盖 99 并按
  「先解密、再按真实方法解压」走完，因此加密条目一律交给它处理即可。
  原先的判断基于旧版 crate 的私有 API，前提已不成立。
  未加密的 LZMA / XZ 仍走本项目自己的解码器——字典上限与进度策略由本项目掌握。
  夹具 `aes-lzma.zip` / `aes-xz.zip` 按 AE-2 规范构造（7-Zip 与 pyzipper
  都写不出这个组合），生成脚本在 `tests/fixtures/generate/`。
- 流程冒烟测试：打开 → 预检 → 解压 → 取消。完整 egui_kittest 窗口测试需要
  真实 wgpu 后端，无头环境跑不了；改为在 `app/tests.rs` 里走 core 的同一条
  业务路径。
- 迁移回归测试：旧菜单 → 新菜单的替换顺序、旧 `*` 菜单清理、迁移幂等性。
  动注册表的测试用进程内互斥锁串行，避免并行测试互相删键。
- 开源社区文件：`CONTRIBUTING.md`、`SECURITY.md`、`CODE_OF_CONDUCT.md`、
  issue / PR 模板。README 补上徽章、安装方式、许可边界与致谢。
- Win11 右键菜单优化：
  - 菜单改为**按归档扩展名注册**（`.zip` / `.7z` / `.rar` / `.tar` 等 12 种），
    不再挂在 `*\shell` 下——之前右键任何一个 `.txt` 都会看到「解压到当前文件夹」。
  - 支持**多选批量传参**（`%1` → `%*` + `MultiSelectModel=Player`），一次选中多个文件
    能一次加进压缩包。
  - 新增**文件夹空白处**右键菜单（`Directory\Background\shell`，传 `%V`），
    在文件夹里点空白处即可把该文件夹打包。
  - 新增**「恢复 Win10 经典右键菜单」开关**：Win11 的紧凑菜单只显示通过
    `IExplorerCommand` + 应用标识注册的命令，注册表动词一律被塞进「显示更多选项」。
    该开关用 CLSID 掩码让资源管理器退回 Win10 风格菜单，菜单项直接出现在右键第一层。
  - 注册表变更后调用 `SHChangeNotify` 通知资源管理器重读，不必重启 explorer.exe。
  - 升级时自动清理旧版挂在 `*\shell` 下的菜单键。
- 代码签名通道：`scripts/sign.ps1`（PFX 文件或证书存储指纹两种方式，RFC 3161 时间戳，
  签完自动 verify）+ release.yml 的 Sign 步骤 + `docs/legal/signing.md`。
  无证书时 CI 明确跳过并打印指引——签名只剩「买证书」这一步。
- RAR 后端换成 `unrar-rs`（纯 Rust）：`copy_to` 把解码产物直接写进 sink，
  **条目流式解码**，内存占用与条目大小无关。旧的 16 MiB 内存上限与临时文件中转全部移除。
- CI：stable 测试 / clippy / fmt，tag 触发可复现打包并上传 Release。
- 差分测试：与固定 SHA-256 的 7-Zip 26.04 双向比对列出/解压、AES zip、AES 7z 与 7z 分卷。
- fuzz：4 个 cargo-fuzz 目标 + stable 确定性变异 smoke 测试。
- criterion 基准：格式嗅探、10k 条目 zip 列表、16 MiB 解压、4 MiB 7z 打包。
- 便携模式：exe 同目录存在 `portable.flag` 或 `themes/` 时，配置、日志、handoff 写入 `<exe>\config\`，首次启动从 `%APPDATA%` 迁移。
- CLI `--json`：`sniff` / `list` / `test` / `extract` / `pack` 输出单行 JSON。
- zip 归档注释：创建时可填写，打开后在标题旁显示提示。
- 主题包导出：设置页把当前主题目录打包为 zip。
- 条目预览：双击或工具栏按钮读取前 512 KiB，文本与 PNG / JPEG / GIF / BMP 弹窗显示，超限截断提示。
- CLI `--comment`：新建 zip 时写入归档注释；`list --json` 输出 comment 字段。
- 打包任务模板：保存/应用常用格式与压缩参数（不含密码），settings schema 升到 5。
- 7z 分卷创建：core 按字节切分，CLI `--volume-size`，GUI 打包对话框可选分卷大小。
- 限额扩到 9 项（总量 / 单条 / 条目数 / 压缩比 / 路径深度 / 路径长度 / 字典 / 套娃深度 / 保留空间），
  三档预设（严格 / 默认 / 宽松），设置页逐项可编辑并落盘；字段定义收敛到 `core::limits`，UI 只做适配。
- 解压前空间预检与磁盘格式检查：可用空间不足、FAT32 上超 4 GiB 的条目、包内含套娃归档都会进预检列表；
  「全部覆盖」、目标是 FAT32、含套娃归档、删除条目四类操作加二次确认。
- 任务暂停 / 继续：任务行新增 `❚❚` / `▶`，暂停在批量边界生效，取消时自动解除暂停。
- 命令面板、窗口位置记忆（离屏自动回落到主屏）、DPAPI 记住密码、CRC 列与列宽记忆、
  可疑条目过滤与图标、动效 / 高对比度开关。
- 无头 UI 冒烟测试（egui_kittest 3 项）、属性测试（proptest：core 10 项 + 主题清单/资源路径 2 项）、
  长路径回归 3 项、多句柄共享读回归 3 项。
- 与 7-Zip 26.04 的对比基准 `benches/vs_7zip.rs`；`THZIP_ENFORCE_REDLINE=1` 时未达红线直接非零退出。
- 供应链门禁 `deny.toml` + CI `cargo-deny`（本地 `cargo deny check` 四类检查全绿）：
  许可白名单补 OFL-1.1 / Ubuntu-font-1.0 / bzip2-1.0.6；字体栈 rustybuzz、ttf-parser 两条
  unmaintained 公告附理由忽略（resvg 0.45 卡住升级路径，且只处理本机主题图标）；
  顺带修掉 `thzip-app` / `thzip-cli` 对 `thzip-core` 的 `*` 版本要求（改成 `0.2.0`）；
  每周一 03:00 定时 fuzz 工作流（5 个目标）；
  CI 增加长路径 job、特性矩阵编译（`--no-default-features` / `svg-text` / `dhat`）与体积门禁。
- 伪本地化 `THZIP_PSEUDO_LOCALE=1`，配「漏翻」检测测试；`dhat` feature 出堆分析报告。
- 启动时清扫自身遗留的临时文件（前缀 + 后缀 + 最小年龄三重条件，不误删别人正在写的文件）。

### 变更

- 最低 Rust 版本提升到 1.93。
- MSVC 运行库改为静态链接（`.cargo/config.toml` 的 `+crt-static`）：exe 不再依赖
  `VCRUNTIME140.dll` / `MSVCP140.dll`，代价是各自增大 1 MiB 左右。
- 任务队列由无界改为有界（1024）+ 提交超时 5 s；同一路径的写操作按 `canonicalize` 键自动串行，
  避免两个任务同时改一个包；事件通道保持无界（worker 唯一上报途径，阻塞会死锁）。
- 解压改用 `Durability::Flush`、归档产出用 `Durability::Sync`：解压只保证原子替换，不再逐文件 fsync。
- 目标文件不存在时直写最终路径（`create_new`），省掉临时文件与改名；覆盖已有文件仍走完整原子替换。
- 写盘侧名字净化与读盘侧统一（L-4）：补设备名、非法字符、段尾点与空格、段长 255。
- RAR 单条内存上限 512 MiB → 16 MiB，超过走临时文件中转。
- 7z 重写不再对包内既有重名条目去重——之前会悄悄丢掉条目。
- `zip` 4.6.1 → 9.6.0：修掉 RUSTSEC-2026-0367 / 0368 / 0369 / 0429（四条都落在
  `ZipArchive::extract` / `ZipStreamReader::extract` 这组解压快捷 API 上，本项目一直自己走
  sink 解压、不解符号链接，但依赖带洞就不该留着）。顺带适配两处签名变化：
  `ZipFile::name()` 返回 `Result`（非 UTF-8 名字），`ZipWriter::set_comment()` 返回 `Result`（注释上限 32 KiB）。
- `app.rs` 拆分：状态 → `state.rs`、搜索 → `filter.rs`、打包 → `pack.rs`、改名 → `relocate.rs`、测试 → `tests.rs`，主文件约 1380 行。
- `sevenz-rust2` 升级到 0.24.2。
- 依赖例行升级（Dependabot 7 个 PR 一次性收口）：`zstd` 0.13 → 0.14、`lz4_flex` 0.12 → 0.14、
  `toml` 1.1.8 → 1.2.0、`fontdb` 0.23 → 0.24、`encoding_rs` 0.8.35 → 0.8.44、
  `flate2` 1.1.10 → 1.1.11、`crc32fast` 1.5.4 → 1.5.5；四个 CI 检查（含与 7-Zip 的差分测试）全绿。
  `dependabot.yml` 补上 `zstd` / `lz4_flex` 的 0.x 小版本 ignore，与其余解压后端同策略：
  这类升级走手动迁移 + 差分测试，不再自动开 PR。

### 性能

- 主程序体积 23.17 MiB → 21.13 MiB（−8.8%）后，因 RAR 后端换成纯 Rust 的 `unrar-rs`
  （纠错 / 并行 / CRC 加速代码随项目静态链入，约 +3.8 MiB）最终为 **24.92 MiB**，
  精简构建 23.65 MiB，7z 压缩分发后 10.74 MiB；CLI 7.18 MiB（7z 后 3.32 MiB）。
  中途的两笔支出：crt-static 静态链接 MSVC 运行库（各约 +1 MiB）、
  以及 zip 升到 9.6 修安全公告（+0.13 MiB）。
  - release profile 改为 `strip = "symbols"` + `lto = "fat"`。
  - `eframe` 关掉 `wayland` / `x11` / `web_screen_reader` / `system_fonts`（项目已自建完整 CJK 字体方案：内嵌 Noto Sans SC 子集 + Windows 字体 + fontdb 兜底）。
  - `wgpu` 只保留 D3D12 后端，去掉 Vulkan / Metal / GLES / WebGPU（Vulkan 的 `ash` 绑定单独就占 68 MB rlib）。
  - `resvg` 关掉 `text` / `system-fonts` / `raster-images`；`egui_extras` 关掉 default features。
  - `accesskit`（屏幕阅读器）与 `svg-text`（SVG 主题文字）做成 Cargo feature，`--no-default-features` 可出精简版。
- 8 MiB 以内的 zip 整包读入内存：10k 条目列表 328 ms → 8 ms；大包仍走文件流。
- 单文件流尺寸优先读元数据：gz 尾部 ISIZE、zstd/lz4 帧头 Content Size，避免打开即整包解压；bz2/xz 保留回退。
- **与 7-Zip 26.04 的对比**（24.2 MiB / 204 条目，三次取最小）：创建 zip 0.58 s 对 0.58 s（1.01x）、
  解压 zip 0.40 s 对 0.42 s（1.05x）、创建 7z 3.46 s 对 0.88 s（0.25x）。
  - 解压从 0.41x 提到 1.05x：逐文件 fsync 改 `Durability::Flush`、目录链校验加缓存、新建文件直写不建临时文件。
  - 创建 7z 从 6.6 s 降到 3.46 s：固态块批量提交（`push_archive_entries`，64 MiB / 512 条目一块，
    编码器不再按条目重建）+ LZMA2 并行分块（`from_level_mt`，块=字典，线程 ≤ 8）。
    剩余差距是纯 Rust `lzma-rust2` 与 7-Zip 手写汇编的差距，换 C 编码器与「零 C 依赖」冲突，明确不做。
- 打包脚本自动在 PATH 与常见安装目录查找 Inno Setup 的 `iscc`，并打印安装包 SHA-256。
- CLI 集成测试：`--json` 全流程与 7z 分卷创建/回读。

### 修复

- **构建**：`build.rs` 不再强依赖注册表定位 Windows SDK。之前 `winresource` 内部调用 `reg.exe` 查询 SDK 路径，在沙箱、域策略禁用 `reg.exe`、精简 CI 镜像下直接失败，错误信息只剩「系统找不到指定的路径」。现在先按标准路径探测 `rc.exe` 并设好 `RC_PATH` 绕开注册表，探测失败才退回注册表；panic 信息附带可操作提示。
- **安全**：解压根目录之下的祖先链含 junction / symlink 时不再越界写盘。之前只检查直接父目录，直接父目录不存在时检查必然通过，随后的建目录操作却会穿过祖先链接把内容写到解压根目录之外。改为从根目录逐级校验并逐级创建。
- **安全**：CRC 校验失败的条目不再落盘。之前只记录一条「CRC 不符」提示，随后照常提交，损坏内容已落到最终路径（zip 的 XZ / LZMA 条目走 `by_index_raw`，库层不校验 CRC，必然落盘）。现在临时文件被删除、目标位置保持原样。
- **数据**：重写归档时按实际落盘路径读回内容。之前用 `plan.items[].rel_path` 读取，而 sink 会对包内同名条目改名（`a.txt` → `a~1.txt`），导致第二个条目被写成第一个条目的内容。tar / 7z 的追加路径同样受影响，一并修正。
- 给归档自带重名条目的归档做改名操作不再误报重名：只检查**改名引入**的重名，归档原有的重名条目原样保留。
- 重写归档遇到无法写回的条目（符号链接、不支持的方法）时明确报错，不再报出难以定位的「读取条目失败」。
- 安装版带有 `themes/` 曾被误判为便携模式；现在检测 `unins000.exe`，安装版继续使用 `%APPDATA%`。
- CLI `--version` 曾输出旧的 `spell` 名称；现在输出 `thzip <版本> (THzip-SpellArchive)`。
- 7z 目录曾被写成 K_ANTI 反条目；升级后目录正确，并写入 Windows 属性（目录 0x10 / 文件 0x20，只读追加 0x01）。
- **安全（M-4）**：路径校验与落盘之间的 TOCTOU 窗口。校验用的是路径字符串，落盘前路径可能被换成链接。
  现在文件句柄打开后取 `GetFinalPathNameByHandleW` 拿真实路径，必须落在解压根目录之下，否则按 `ReparsePointWrite` 拒绝。
- **限额（L-1）**：压缩比检测对 tar / rar / 固实 7z 完全不触发（这些格式没有逐条 `packed`）。
  补整包口径（解压总量 / 归档体积），四个后端统一接入 `plan_safety_warnings`。
- **预览（L-3）**：`MemorySink` 不接预算、不校验 CRC，恶意包可让预览吃掉任意内存。
  改用 `Budget` + `crc32fast`，`into_parts` 返回完整性标志。
- **zip 追加（L-5）**：`reopen_rw` 按路径重开文件而不是复用句柄，改为 `rewind()`。
- **改名（L-6）**：`PlanAction::Rename` 的目标名在落到计划时被丢弃（只保留原始相对路径），
  大小写冲突检测成死代码。现在重新净化目标名并拼到根目录下，`case_collisions()` 供 UI 提示。
- **非 Windows（L-7）**：`set_mtime` 与可用空间探测原本在 Unix 上缺失，改用 `utimensat`（`UTIME_OMIT`）与 `statvfs`。
- **空间（L-8）**：可用空间不足只显示不拦截，改为 `ensure_space()` 返回 `FailReason::InsufficientSpace`，CLI 与 GUI 都据此拦。
- **RAR（L-9）**：单条 512 MiB 整条读进内存，降到 16 MiB。
- **注释（L-10）**：`raw_name` 的注释与实际用途不符（写着「用于落盘」），改为「原始字节，仅供诊断；落盘用 `display`」。

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
