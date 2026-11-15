# NOTICE

## 代码

MIT（见 LICENSE，若尚未添加则以仓库声明为准）。

## 美术素材

`themes/*/png/` 内的图标、头像、进度条贴图来自 `../assets/`（THzip-SpellArchive 同人自制像素素材），
以 **CC BY-NC-SA 4.0** 授权：署名、非商用、相同方式共享。

因此：

- 代码可商用（MIT），但**内置素材不可商用**；商用必须自行替换素材。
- 主题包作者对自己提供的素材负责。

## 字体

程序内嵌 **Noto Sans SC** 子集（`crates/thzip-app/assets/NotoSansSC-subset.ttf`，Regular 400，
覆盖 GB2312 + 拉丁字符 + 常用标点），独立于系统字体；子集未覆盖的生僻字仍回退系统 CJK 字体。

- 版权：Copyright 2014-2021 Adobe (http://www.adobe.com/)，Reserved Font Name 'Source'。
- 许可：SIL Open Font License 1.1，全文见 `third_party/noto-sans-sc-OFL.txt`。
- 子集由 `NotoSansSC-VF.ttf` 经 `fonttools varLib.instancer`（wght=400）与 `pyftsubset` 生成。

## 第三方依赖

见 `Cargo.lock`。

RAR 读取通过 `unrar-rs` 实现。该 crate 是纯 Rust 的 RAR 解码器，其 RAR 引擎
源自 UnRAR 程序，因此沿用 UnRAR 许可条款：

- `unrar-rs` 的许可为 Apache License 2.0，附加一条 UnRAR 条件，
  全文见 `third_party/unrar-rs-license.txt`。
- 该条件允许免费用于处理 RAR 归档（含商业用途），但**禁止**用于开发
  RAR（WinRAR）兼容压缩器或逆向 RAR 压缩算法。
- 二进制分发必须附带该许可文件。打包脚本已把整个 `third_party/` 复制进
  `dist/`，因此 `unrar-rs-license.txt` 会随包分发。

除 RAR 外的格式均为纯 Rust 实现，不依赖 7z.dll 等外部运行库。

## 同人声明

THzip-SpellArchive 是东方 Project 二次同人作品，与上海爱丽丝幻乐团、ZUN 无关联，
遵循「東方Project 二次創作ガイドライン」，禁止商用。
