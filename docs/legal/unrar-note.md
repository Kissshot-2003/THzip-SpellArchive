# RAR 支持的法务边界

## 现状

`thzip-core` 依赖 `unrar-rs`（纯 Rust 的 RAR 解码器）。它的 RAR 引擎源自
UnRAR 程序的 C++ 源码，Apache-2.0 许可之上附加一条 UnRAR 条件。
RAR 解压是产品的重要能力，RAR 不在设计文档 v0.1 的核心范围内
（原计划 v0.3 可选），实际已做成核心只读格式。

## 需要注意的点

1. **UnRAR 许可不是 OSI 许可。** 它允许免费使用与分发（含商业用途），但禁止
   使用其源码反向工程 RAR 压缩算法（例如拿它做出一个 RAR 压缩器）。
   `unrar-rs` 的 `license` 字段是自定义的（`license-file`），`cargo deny`
   只能看到「非标准许可」这个标记，**看不到具体条款**——必须人工核对。
2. **再分发义务。** UnRAR 段落明确要求：二进制分发必须附带许可文件。
   `third_party/unrar-rs-license.txt` 已随 `third_party/` 复制进 `dist/`，
   属于**已合规**（不再是待补项）。
3. **只读边界。** 项目只做 RAR 解压、不做 RAR 创建，这与 UnRAR 许可的限制方向一致。

## 变更记录

### 2026-11：unrar-ng → unrar-rs

原先依赖 `unrar-ng`（C++ UnRAR 源码的 Rust 封装，MIT OR Apache-2.0 声明）。
换成 `unrar-rs` 的原因不是许可而是能力：

| | unrar-ng 0.7 | unrar-rs 0.15 |
|---|---|---|
| 实现 | 编译 C++ UnRAR 源码 | 纯 Rust |
| 条目读取 | `read()` 返回整条 `Vec<u8>`；超大条目需先 `extract_to` 落临时文件 | `copy_to(writer)` 解码产物直接写进调用方的 sink |
| 内存占用 | 与条目大小成正比（临时文件兜底时仍需额外一次落盘 + 读回） | 与条目大小无关 |
| 构建 | 需要 C++ 工具链 | 纯 Rust（`crypto-rust` feature 连加密后端也不需要 NASM） |
| 许可 | MIT OR Apache-2.0 + 捆绑 UnRAR 源码 | Apache-2.0 + UnRAR 条件（条款写进 LICENSE 文件） |

许可方面两者约束相同（都继承 UnRAR 条款），`unrar-rs` 的优势是条款明确写在
许可文件里而非藏在捆绑源码中，更容易审计。

> 注意：`unrar-rs` 默认 feature 走 `aws-lc-rs`，需要 NASM 才能编译。
> 本项目固定使用 `default-features = false, features = ["crypto-rust"]`，
> 走纯 Rust 的 aes/cbc/hmac。

## 待办

- [ ] 关于对话框列出第三方许可（`unrar-rs` 的 UnRAR 条款已随包分发，
      但 UI 里还没有展示入口）
- [ ] 若未来要加 RAR 创建：必须换实现或取得授权，不能基于当前源码

## 决策记录

维持「RAR 只读 + 捆绑 UnRAR 引擎」，理由是用户明确需要 RAR 支持，且只读用法
不触碰 UnRAR 许可的核心限制。2026-11 从 `unrar-ng` 切到 `unrar-rs`，
主要动机是流式解码能力（见上表），许可约束不变。此项需在正式对外发布前
由法务确认。
