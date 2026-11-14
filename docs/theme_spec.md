# theme.toml 规范（schema = 1）

主题 = 贴图 + 配色 + 可选文案覆盖 + 可选动效。**语言不属于主题**：文案优先取内置语言包，
主题只能覆盖措辞，不能删掉信息（错误原因、路径、大小必须照常显示）。

## 目录结构

```text
themes/<主题 id>/
├── theme.toml
├── png/
└── svg/            # 仅 plain / 矢量主题使用
```

## 最小示例

```toml
schema = 1

[meta]
name = "THzip-SpellArchive - Reimu"
author = "YourName"
version = "1.0"
license = "CC-BY-NC-SA-4.0"      # 必填
description = "灵梦结界主题，像素符卡风格"
sampling_pixel = true             # true = NEAREST，像素图不糊
scale = 2                         # 素材按 2x 导出

[text_map]                        # 全部可选；缺失回退内置 zh-CN / en-US
pack = "构筑封印结界"
extract = "解除符卡封印"
task_running = "咏唱进行中…"
task_done = "符卡咏唱完成"
search_hint = "搜索符卡名…"

[effects]
particles = true            # 预留；当前仅读取 particles

[resource]
IconArchive7z = "png/icon-archive.png"
ProgressSpellBg = "png/progress-base.png"
ProgressSpellFill = "png/progress-fill-reimu.png"

[resource_nine_patch]             # 单位：1x 像素
PanelBg = [12, 12, 12, 12]
BorderNormal = [4, 4, 4, 4]
BorderWarning = [4, 4, 4, 4]

[color]
accent = "#F03D52"
accent_soft = "#3D1F33"
progress_bg = "#0E1020"
bg_window = "#171A2E"
bg_panel = "#1F213B"
bg_hover = "#292B47"
border = "#404263"
text_normal = "#F5F2E8"
text_sub = "#A6A6BD"
success = "#52E0A0"
warning = "#FFC857"
error = "#FF5E6E"
```

## 资源 Key

| Key | 建议素材 | 必需 |
|---|---|---|
| `IconArchive7z` / `IconArchiveZip` / `IconArchiveZst` | `icons/icon-archive.png` | 否 |
| `IconArchiveEncrypted` | `icons/icon-archive.png`（过渡） | 否 |
| `IconFile` / `IconFolder` / `IconFileUnknown` | `icons/icon-file.png` / `icon-folder.png` | 否 |
| `ProgressSpellBg` / `ProgressSpellFill` / `ProgressSpellFail` | `progress/*.png` | 是（缺失则纯色） |
| `BtnExtract` / `BtnPack` / `BtnAddFile` / `BtnDelete` | `icons/*.png` | 否 |
| `BtnSettings` / `BtnSearch` / `BtnPause` / `BtnResume` | `icons/*.png` | 否 |
| `BtnClose` / `BtnMinimize` / `BtnTheme` | `icons/*.png` | 否 |
| `StateOk` / `StateWarn` / `StateError` | `icons/*.png` | 否 |
| `PanelBg` / `BorderNormal` / `BorderWarning` | 九宫格切图 | 否 |
| `ParticleSparkle` / `ParticleSakura` | 透明 PNG | 否 |
| `AvatarMain` | `avatars/<角色>-256.png` | 否（工具栏/设置页头像） |

Key 的枚举与名称为 `thzip-app/src/theme/mod.rs` 的 `ResKey`；
`theme/loader.rs` 的测试会扫描 `themes/*/theme.toml`，未知 Key 或缺失文件会让测试失败。

## 解析规则（实现约束）

1. `[resource]` 先解析成 `BTreeMap<String, String>`，再匹配枚举。未知 key 警告并忽略，
   **不允许整包加载失败**。
2. 路径必须相对，禁止 `..`；`canonicalize` 后必须仍在主题目录内。
3. 单文件 ≤ 8 MiB，总资源 ≤ 64 MiB，文件数 ≤ 500。
4. `schema` 高于程序支持版本 → 只读加载并提示升级，不 panic。
5. 回退链：当前主题 → 编译进 exe 的内置 fallback → 纯色占位。`themes/` 目录不存在也能启动。
6. `[resource_nine_patch]` 缺失的面板图按拉伸处理并给出警告（宽窗口会明显变形）。
7. `scale` 必须为正整数；像素素材只允许整数倍缩放（最终物理像素为 `scale` 的整数倍）。
8. 主题包导入会执行 500 文件 / 单文件 8 MiB / 总量 64 MiB 限制，并拒绝可执行扩展名；
   直接放在 `themes/` 下的主题只受贴图解码限制（SVG ≤ 8 MiB、最长边 ≤ 2048）。

## 安全

- 主题包导入走 `ExtractDirSink`，拒绝 `.exe/.dll/.bat/.ps1/.scr` 等可执行文件；符号链接条目会拒绝导入。
- SVG 禁用外部资源引用，单文件 ≤ 8 MiB、最长边 ≤ 2048。
- 主题不允许自带字体。

## 文案 Key（与 `TextKey` 对应）

`app_title` `open` `new_archive` `extract` `pack` `test` `settings`
`local_files` `archive_entries` `task_queue` `task_running` `task_done`
`search_hint` `theme` `language` `standard_terms` `first_run_hint` `dismiss`
`empty_archive` `demo_badge` `go_up` `status_ready` `about`

主题覆盖可以包含 `{name}`、`{count}` 等占位符；必须与内置文案保持同一组占位符，
否则动态内容会被丢掉。英文与中文共用同一 key。

## 可发现性硬规则

主题可以改文案，但每个动作必须在 tooltip 中给出标准术语（例如
`解除符卡封印（解压）`），且设置页提供「显示标准术语」开关。
