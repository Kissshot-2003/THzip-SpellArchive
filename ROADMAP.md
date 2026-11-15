# THzip-SpellArchive 长任务路线图

> 面向后续接手的开发者和 AI 代理。本文件是可执行的长任务清单，不是宣传文档。
> 当前基线：`0.2.0`，70 次提交，`cargo test --workspace` 204 通过（core 141 / app 59 / cli 4），clippy / fmt 干净。
> 最后更新：2026-11-15（Phase 0 启动）。

## 0. 约束（任何阶段都不得破坏）

1. `thzip-core` 零 UI 依赖；UI 只通过 `ArchiveReader` / `ArchiveWriter` / `TaskEvent` 通信。
2. 所有写盘必须经过 `safety` 层；禁止直接 `File::create` 目标文件。
3. 后台任务必须走 `task::worker::guarded`；release profile 保持 `panic = "unwind"`。
4. 文件编码统一 UTF-8，不在 PowerShell 5.1 里对源码做 `Get-Content` + `Set-Content` 往返。
5. 归档条目一律用 `EntryId` 定位，不用文件名。
6. 代码 MIT、内置素材 CC BY-NC-SA 4.0，商用需替换素材。

## 1. 阶段总览

| 阶段 | 目标 | 预估 | 依赖 | 状态 |
|---|---|---|---|---|
| Phase 0 | 基线固化：远端、CI、版本单一来源、可复现打包、CHANGELOG | 1 天 | 无 | 已完成 |
| Phase 1 | 质量护栏：fuzz、差分测试、基准、GUI 冒烟 | 3–5 天 | Phase 0 | 基本完成（自动化 GUI 冒烟待补） |
| Phase 2 | 产品功能收尾：便携模式、注释、主题导出、任务模板、预览 | 1–2 周 | Phase 0 | 已完成 |
| Phase 3 | 格式与 CLI 增强：`--json`、zip 加密 LZMA/XZ、7z 固实进度、分卷创建 | 1–2 周 | Phase 1 | 部分完成（2 项上游阻塞） |
| Phase 4 | 性能与架构：单文件懒加载、RAR 大条目、拆分 `app.rs`、handoff IPC | 1–2 周 | Phase 1 | 进行中 |
| Phase 5 | 发布 1.0：签名、安装包实测、升级/迁移回归、文档定稿 | 1 周 | Phase 2–4 | 进行中（签名阻塞） |

优先级：**P0 = Phase 0 + 1**（不做会出事）；**P1 = Phase 2**（用户可感知）；**P2 = Phase 3 + 4**。

## 2. Phase 0：基线固化

- [x] 写入本路线图
- [x] `.gitattributes` 固定换行/编码/二进制类型
- [x] 版本单一来源：`Cargo.toml` → `build.rs`（已有）→ `package.ps1` → Inno `/D`
- [x] `package.ps1` 同时构建 app 与 CLI，校验版本，重建 `dist`，生成 `SHA256SUMS.txt`
- [x] `CHANGELOG.md`
- [x] `.github/workflows/ci.yml`（test / clippy / fmt）
- [x] `.github/workflows/release.yml`（tag 触发打包并上传 Release）
- [x] 创建远端私有仓库并推送 `main`
- [x] 验证：`scripts/package.ps1` 产出的版本与 `Cargo.toml` 一致，`dist` 含两个 exe

**退出标准**：干净机器上一条命令产出与源码同版本的 `dist`；推送后 CI 全绿。

## 3. Phase 1：质量护栏

- [x] `cargo-fuzz` 4 个 target：`classify`、zip 中央目录、7z 头、tar 头（stable 另有 `fuzz_smoke` 确定性变异）
- [x] 差分测试：固定 SHA-256 的 7-Zip 26.04，双向列出/解压、AES zip、AES 7z、7z 分卷
- [x] `criterion` 基准：10k 条目列表、16 MiB 解压、4 MiB 7z 打包（10 万条目与内存峰值待补）
- [ ] `egui_kittest` 冒烟：打开 → 预检 → 解压 → 取消
- [x] 把基准结果写入 README；`bench.yml` 可手动跑并上传 criterion 报告（10k 列表 340ms 待优化）
- [x] 顺带修复：`sevenz-rust2` 0.20.2 把目录写成 K_ANTI 反条目；升级 0.24.2 并写入 Windows 属性

**退出标准**：fuzz 连续 72 小时无崩溃；差分语料 20 个全过；基准有前后对比数据。

## 4. Phase 2：产品功能收尾

- [x] 便携模式：`portable.flag` 或同目录 `themes/` → 配置写 `./config/`（含首次迁移）
- [x] 归档注释读写（zip：打包对话框写入，标题旁提示；7z/tar 不适用）
- [x] 主题包导出（设置页导出当前主题目录为 zip，复用打包任务）
- [x] 任务模板 TOML（prefs `pack_templates`，保存格式 + 压缩参数，不含密码）
- [x] 预览窗格最小版：双击或工具栏按钮读取前 512 KiB，文本 / PNG / JPEG / GIF / BMP 弹窗预览，超限截断
- [x] i18n 补齐上述全部文案，中英完整

**退出标准**：HANDOVER 手工验收清单新增项全过；无硬编码文案。

## 5. Phase 3：格式与 CLI 增强

- [x] CLI `--json`：`sniff` / `list` / `test` / `extract` / `pack` 机读输出（提前完成）
- [ ] zip 加密 LZMA/XZ 读取（阻塞：zip 4.6 的解密流是私有 API，自定义 LZMA/XZ 解码拿不到明文流；需自行实现 AES-CTR/HMAC，暂不做）
- [ ] 7z 固实包按压缩字节上报进度（阻塞：sevenz-rust2 0.24.2 无压缩字节回调）
- [x] 分卷 `.7z.001` 创建（core 字节切分、CLI `--volume-size`、GUI 分卷下拉）
- [x] 评估加密包追加：结论为不做——需用密码重加密整个包，失败窗口与密码处理风险不成比例

**退出标准**：与 7-Zip 26.04 交叉验证矩阵补齐（差分测试已覆盖 zip / 7z / tar）；错误码全部结构化。

## 6. Phase 4：性能与架构

- [x] 10k 条目 zip 打开 328 ms → 8 ms：≤ 8 MiB 包整包读入内存，避免逐条目 seek
- [x] 单文件流尺寸：gz 读尾部 ISIZE、zstd/lz4 读帧头；bz2/xz 保留完整解码回退（HANDOVER 24）
- [x] RAR 超大条目流式中转：后端换成 `unrar-rs`（纯 Rust），`copy_to` 把解码产物直接写进 sink——
      不再整条读进内存，也不再落临时文件再读回。旧的 16 MiB 内存上限与临时文件中转全部移除。
      与 `unrar-ng` 逐字节比对一致（普通包 / 固实包 / 加密包三种夹具哈希相同）。
- [x] 拆分 `app.rs`：类型 → `app/state.rs`，搜索 → `filter.rs`，打包 → `pack.rs`，改名 → `relocate.rs`，测试 → `tests.rs`
- [x] 主文件从 2627 行降到约 1380 行（目标 < 1500 已达成）
- [ ] 单实例 handoff 轮询改 `WM_COPYDATA`（阻塞：winit 持有 WndProc，需要消息窗口 + 子类化与不安全代码；当前 250/500 ms 轮询成本可接受）
- [x] 拆分后 142 个测试不做修改仍全绿

**退出标准**：冷启动耗时与峰值内存有前后对比；`app.rs` 单文件低于 800 行。

## 7. Phase 5：发布 1.0

- [ ] 代码签名证书接入 CI（**通道已就绪**，只差证书本身：`scripts/sign.ps1` + release.yml 的
      Sign 步骤 + `docs/legal/signing.md`；无证书时 CI 明确跳过并打印说明。OV 证书约 $200–400/年）
- [x] Inno 安装包实测：静默安装 / 启动 / 卸载通过；安装版正确使用 `%APPDATA%`（非便携）
- [ ] 迁移回归：旧 `SpellArchive` → `THzip-SpellArchive` 配置与注册表
- [x] 文档定稿：README 能力表、HANDOVER、本路线图、CHANGELOG 与测试计数同步
- [ ] 干净 Win10 / Win11 各装一遍，SmartScreen 无拦截（依赖上面的证书）

**退出标准**：无管理员权限可安装；卸载无残留；日志与崩溃落盘正常。

## 8. 明确不做

| 项 | 原因 |
|---|---|
| RAR 写入 | UnRAR 许可证只允许解压；实现成本与法务风险不划算 |
| zip 分卷 `.z01` | 收益低，非目标用户场景 |
| 增量归档（基准包 + 补丁） | 等价 rsync 差分，风险收益不成比例 |
| zip 加密 LZMA/XZ | 需要自行实现 WinZip AES 解密才能接自定义解码器 |
| 7z 固实压缩进度 | 上游库未暴露压缩字节回调 |
| 加密包追加 | 需整包密码重加密，失败窗口与密码处理风险高 |
| 7z.dll 后端 | 纯 Rust 已覆盖目标格式，COM vtable 维护成本高 |
| Linux 首发 | 用户与素材生态在 Windows，暂列观望 |

## 9. 推进节奏

- 每个阶段单独分支 + PR，CI 全绿后合并。
- 阶段内每完成一项就勾选本文件，不攒到最后。
- 阶段结束更新 `HANDOVER.md` 的现状、测试数量与已知问题表。
- 版本号遵循 SemVer；破坏兼容的 prefs/schema 变更必须带迁移函数。
