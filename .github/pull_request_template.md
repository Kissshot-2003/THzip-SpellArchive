## 变更了什么

<!-- 做了什么，为什么 -->

## 相关 issue

<!-- Closes #123 -->

## 检查清单

- [ ] `cargo fmt --all --check` 通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 通过
- [ ] `cargo test --workspace` 通过
- [ ] 新增行为有对应测试
- [ ] 界面文案走 i18n，中英文都补齐
- [ ] 体积未超门禁（GUI ≤ 27 MiB、CLI ≤ 8 MiB）
- [ ] 未破坏 `thzip-core` 的零 UI 依赖与 `deny(unsafe_code)`

<!--
涉及安全层的改动（safety / backend / limits）请在描述里说明
威胁模型上的考虑。
-->
