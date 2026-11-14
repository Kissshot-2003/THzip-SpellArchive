# 模糊测试

本目录是 [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) 目标，需要 nightly 工具链：

```powershell
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run classify
cargo +nightly fuzz run zip_central_directory
cargo +nightly fuzz run sevenz_header
cargo +nightly fuzz run tar_header
```

CI 不依赖 nightly：`crates/thzip-core/tests/fuzz_smoke.rs` 用确定性变异跑同一批 fixture，
在 stable 上覆盖同样的“畸形输入不 panic”约束。两个入口互补，提交前至少跑一次 smoke。

>`fuzz/corpus` 与 `fuzz/artifacts` 不入库；有价值的崩溃输入请附到 issue 或提交到本目录下的回归用例。
