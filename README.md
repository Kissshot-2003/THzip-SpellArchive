# SpellArchive（工程骨架）

东方 Project 同人主题归档管理器。当前为可编译的**框架骨架**：

- `spell-core`：零 UI 依赖的核心（类型、trait、安全解压、限额、任务事件）
- `spell-app`：egui 三栏界面（主题、i18n、自绘进度条、虚拟化列表）
- `spell-cli`：复用 core 的命令行入口（格式嗅探已可用）

## 运行

```powershell
cargo run -p spell-app          # 启动 GUI
cargo run -p spell-cli -- sniff <文件>   # 格式嗅探
cargo test -p spell-core        # 安全解压等单元测试
```

## 当前状态

| 模块 | 状态 |
|---|---|
| 格式嗅探（magic bytes） | 可用 |
| 路径净化 / 显示名防护 / 限额计数 / 原子覆盖 | 可用，含测试 |
| 7z / zip / tar 后端 | 接口与占位实现，待接入 sevenz-rust2、zip、tar |
| 任务线程池 | 类型与 `catch_unwind` 保护已就位，调度待实现 |
| GUI | 可运行：三栏布局、虚拟化列表、符阵进度、主题切换、i18n 回退 |
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
