# NOTICE

## 代码

MIT（见 LICENSE，若尚未添加则以仓库声明为准）。

## 美术素材

`themes/*/png/` 内的图标、头像、进度条贴图来自 `../assets/`（SpellArchive 同人自制像素素材），
以 **CC BY-NC-SA 4.0** 授权：署名、非商用、相同方式共享。

因此：

- 代码可商用（MIT），但**内置素材不可商用**；商用必须自行替换素材。
- 主题包作者对自己提供的素材负责。

## 字体

程序运行时加载系统字体（simhei / Deng），不随二进制分发字体文件。

## 第三方依赖

见 `Cargo.lock`。

RAR 读取通过 `unrar-ng` / `unrar-ng-sys` 使用 UnRAR 源码：

- `unrar-ng` 包装层为 MIT OR Apache-2.0。
- UnRAR 源码适用 UnRAR 许可证，全文见 `third_party/unrar-license.txt`。
  允许用于解压 RAR；**不得**用于开发 RAR 兼容压缩器。分发二进制时需保留该许可证。
- 除 RAR 外的格式均为纯 Rust 实现，不依赖 7z.dll 等外部运行库。

## 同人声明

SpellArchive 是东方 Project 二次同人作品，与上海爱丽丝幻乐团、ZUN 无关联，
遵循「東方Project 二次創作ガイドライン」，禁止商用。
