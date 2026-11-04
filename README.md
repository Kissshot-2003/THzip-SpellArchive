# SpellArchive

东方 Project 同人主题归档管理器。当前为**zip 已可在 GUI 中打开、解压、测试、创建、追加与 AES 加密**的阶段：

- `spell-core`：零 UI 依赖的核心（类型、trait、安全解压、限额、任务线程池、zip 读写）
- `spell-app`：egui 三栏界面（主题、i18n、自绘进度条、虚拟化列表、打开/预检/解压/测试）
- `spell-cli`：复用 core 的命令行入口（zip 的 list / test / extract / pack / 追加可用）

## 运行

```powershell
cargo run -p spell-app                           # 启动 GUI
cargo run -p spell-app -- <归档路径>              # 启动并直接打开归档
cargo run -p spell-cli -- sniff <文件>            # 格式嗅探
cargo run -p spell-cli -- list <zip>             # 列出条目
cargo run -p spell-cli -- test <zip>             # CRC 完整性测试
cargo run -p spell-cli -- extract <zip> <目录>    # 安全解压（--force 覆盖）
cargo run -p spell-cli -- pack <输出.zip> <输入...>  # 创建 / 追加（--append / --password）
cargo test --workspace                           # 47 个单元 / 集成 / zip 攻击用例
```

## 当前状态

| 模块 | 状态 |
|---|---|
| 格式嗅探（magic bytes） | 可用 |
| 路径净化 / 显示名防护 / 限额计数 / 原子覆盖 / junction 防护 | 可用，含测试 |
| zip 读 | **可用**：列目录、预检、安全解压、CRC 校验、AES 解压；同名自动 `~1` 改名 |
| zip 写 | **可用**：Store / Deflate、AES-256、目录递归、归档名净化、重名拒绝、追加入现有包、原子替换、zip64 大文件 |
| 7z / tar 后端 | 接口与占位实现，待接入 sevenz-rust2、tar |
| CLI | `sniff` / `list` / `test` / `extract` / `pack` 可用（zip） |
| 任务线程池 | **可用**：2 条后台 worker、`crossbeam-channel` 事件、进度节流、取消、panic 兜底 |
| GUI | **可用**：打开 zip、真实条目列表、解压预检、覆盖策略、密码重试、完整性测试、任务取消、创建 zip |
| 主题引擎 | 可扫描 `themes/*/theme.toml`，载入 PNG 纹理与配色；缺资源回退内置主题 |
| 字体 | 自动加载系统 CJK 字体（simhei / Deng） |

## 主题与素材

`themes/{reimu,marisa,sanae,cirno}/` 是从 `../assets/` 复制并按 `theme.toml` 组织好的四个主题：
头像、功能图标、底图 / 填充进度条。修改主题只需改 `theme.toml` 的路径与配色，
不需要改 Rust 代码。资源 Key 与回退规则见 `docs/theme_spec.md`。

素材授权：CC BY-NC-SA 4.0（禁止商用），代码 MIT；边界见 `NOTICE.md`。

## 交接与后续

接手开发请先读 **`HANDOVER.md`**：现状、完成度矩阵、已知问题、下一步接入点都在里面。

## 设计文档

`../SpellArchive 开发方案（Rust 版）v2.1.md`
