# 贡献指南

感谢你愿意为 THzip-SpellArchive 出力。这份文档说明怎么把改动送上来的流程、
本地怎么跑起来、以及哪些地方容易被改坏。

## 快速上手

需要 Rust 1.93 或更新版本（`rust-toolchain.toml` 里锁的是 stable + edition 2024）。

```powershell
git clone <仓库地址>
cd THzip-SpellArchive
cargo test --workspace          # 212 个测试，约十几秒
cargo run -p thzip-app          # 启动 GUI
```

Windows 上开发体验最完整。Linux / macOS 可以编译与跑测试，但 GUI 的托盘、
资源管理器右键菜单、单实例这些是 Windows 专属实现，在别的平台上是空实现。

## 代码结构

| Crate | 职责 | 硬约束 |
|---|---|---|
| `thzip-core` | 归档读写、安全解压、限额、任务线程池 | **零 UI 依赖**；`#![deny(unsafe_code)]` |
| `thzip-app` | egui 界面、主题、i18n、托盘、右键菜单 | 只通过 `ArchiveReader` / `ArchiveWriter` / `TaskEvent` 与 core 通信 |
| `thzip-cli` | 命令行入口 | 复用 core，不自己实现归档逻辑 |

依赖方向是单向的：`app` → `core`，`cli` → `core`。`core` 永远不能反过来依赖
`app` 或 `cli`——这是靠 Cargo 的 crate 边界强制的，不要试图绕过。

## 不能破坏的约束

这几条写在 `ROADMAP.md` 的第 0 节，任何改动都不能违反：

1. **`thzip-core` 零 UI 依赖**，所有写盘必须经过 `safety` 层，禁止直接
   `File::create` 目标文件。
2. **后台任务必须走 `task::worker::guarded`**，release profile 保持 `panic = "unwind"`。
3. **文件编码统一 UTF-8**。不要在 PowerShell 5.1 里对源码做 `Get-Content` +
   `Set-Content` 往返——它会破坏编码和换行。要改脚本请用 `WriteAllText` 并显式
   指定 UTF-8 + CRLF。
4. **归档条目一律用 `EntryId` 定位**，不用文件名。同名条目在包内是合法的。
5. **代码 MIT、内置素材 CC BY-NC-SA 4.0**，商用需替换素材。见 `NOTICE.md`。

## 安全相关改动

这个项目处理的是**不可信输入**——压缩包里的文件名、路径穿越、压缩炸弹、
畸形头部，全都在攻击面内。涉及 `safety/`、`backend/`、`limits.rs` 的改动请特别注意：

- **路径净化不修正、只拒绝**。静默修正会掩盖攻击，也会让用户困惑于「文件去哪了」。
- **写盘一律走 `AtomicFile`**（临时文件 + 原子替换），不要为了「快一点」直接写目标。
- **限额是三道防线**：条目数、单条大小、压缩比。新增大小的解压路径时，
  记得检查它是否也在 `Limits` 的约束内。
- **符号链接默认跳过**，不要改成跟随——跟随会引入 TOCTOU 逃逸。

如果你发现了安全问题，**请不要直接开公开 issue**，见 `SECURITY.md`。

## 提交前检查

```powershell
cargo fmt --all --check          # 格式
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check                 # 许可 / 漏洞 / 依赖来源
```

CI 会跑上面这一套，外加：

- 体积门禁（`scripts/check-size.ps1`）：GUI ≤ 27 MiB、CLI ≤ 8 MiB。
  超了要么优化，要么在脚本里显式上调并写明理由。
- `cargo deny` 的许可白名单。新增依赖前先确认它的许可。
- 特性矩阵编译：`--no-default-features`、`--no-default-features --features svg-text`、
  `--features dhat`，以及 fuzz 目标。

## 提交信息

用祈使句写清楚做了什么，body 里说明为什么。参考现有提交的风格：

```
feat(zip): support AES entries combined with LZMA and XZ

WinZip AES records the compression method as 99 and keeps the real one in
the 0x9901 extra field. The zip crate now overwrites 99 with the real
method while parsing, so encrypted entries can be handed to it whole.
```

## 加新格式支持

1. 在 `thzip-core/src/backend/` 加 `xxx.rs`，实现 `ArchiveReader`；
   要能写就再实现 `ArchiveWriter`。
2. 在 `backend/mod.rs` 的 `classify()` 里加魔数识别——**不要只靠扩展名**。
3. 在 `types.rs` 的 `ArchiveKind` 加变体。
4. 写测试：读一个真实归档、解出来内容对得上；再写一个畸形输入，确认是**报错**
   而不是 panic。
5. 如果格式有已知的安全问题（比如路径穿越），在 `safety/` 里处理，不要指望
   上游库已经防住了。

测试夹具放在 `crates/thzip-core/tests/fixtures/`。如果是自己构造的样本，
把生成脚本一起放进 `fixtures/generate/`，并在测试注释里写清楚怎么重新生成。

## 界面改动

- 界面文本一律走 `i18n` 的 `TextKey`，**不要硬编码中文或英文**。
  新增文案要同时补中英两份，`cargo test` 里有漏翻检测。
- 主题资源按 `docs/theme_spec.md` 的 ResKey 命名，改主题不需要动 Rust 代码。
- 新增交互要能被键盘操作——这是无障碍要求，不是可选项。

## 同人素材

内置像素素材是东方 Project 二次同人作品，**禁止商用**。代码是 MIT，可以商用。

如果你的改动涉及素材，注意：

- 不要引入现成的同人素材包（版权状态不清）。
- 自己画的素材同样按 CC BY-NC-SA 4.0 授权，并注明作者。
- 第三方主题包由制作者自行承担版权责任。

## 提问

遇到不确定的事，先看 `HANDOVER.md`——里面记录了现状、架构、已知坑和每个决定的
理由，比 issue 里的往返快得多。
