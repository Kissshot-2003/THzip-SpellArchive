# 代码签名

## 现状

**签名证书还没有**——这是 1.0 发布前唯一需要付费采购的外部依赖。
其余所有阻塞（性能、格式覆盖、跨平台）都是工程问题，已经或可以靠代码解决。

证书到位之前，Windows SmartScreen 会对下载的 exe 弹「未知发布者」警告，
用户需要点「更多信息 → 仍要运行」。这不影响功能，但影响分发体验。

## 已经铺好的路

签名通道已经接好，**有证书就能直接用，不需要改任何代码**：

- `scripts/sign.ps1` —— 本地签名脚本，支持 PFX 文件与证书存储指纹两种方式，
  带 RFC 3161 时间戳，签完自动 verify 一遍。
- `.github/workflows/release.yml` —— Release 工作流里已插入「Sign」步骤。
  没配置证书时它会打印说明并跳过（不阻塞发布）；配置了就会签。

## 本地签名

```powershell
# PFX 文件
./scripts/sign.ps1 -Pfx D:\certs\thzip.pfx -Password "你的密码"

# Windows 证书存储里的证书
./scripts/sign.ps1 -Thumbprint 0123456789ABCDEF0123456789ABCDEF01234567
```

也可以用环境变量，避免密码出现在命令行历史里：

```powershell
$env:THZIP_SIGN_PFX = "D:\certs\thzip.pfx"
$env:THZIP_SIGN_PASSWORD = "你的密码"
./scripts/sign.ps1
```

`signtool.exe` 来自 Windows SDK。CI 的 `windows-latest` 自带，
本地没有的话装一个 Windows SDK 即可。

## CI 配置

在仓库 Settings → Secrets 里加两个 secret：

| Secret | 内容 |
|---|---|
| `SIGN_PFX_BASE64` | 证书文件的 base64 编码 |
| `SIGN_PFX_PASSWORD` | 证书密码 |

base64 编码（在 PowerShell 里）：

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("D:\certs\thzip.pfx"))
```

证书以 base64 存进 Secret，避免二进制文件直接进仓库。
工作流会把它还原成临时文件、签完立刻删除。

## 采购建议

| 证书类型 | 说明 |
|---|---|
| OV（组织验证）代码签名证书 | 约 $200–400/年，验证公司主体，SmartScreen 信誉积累后警告会消失 |
| EV（扩展验证）代码签名证书 | 约 $400–900/年，即时获得 SmartScreen 信誉，但个人与小团队通常拿不到 |

本项目是个人/小团队作品，**OV 证书足够**。注意：

- 证书必须支持 **Authenticode**（代码签名），不能是 TLS 证书。
- 私钥建议带 **硬件令牌**（YubiKey 等）或云 HSM。
- 申请需要法人身份证明；个人开发者可以选「个人代码签名证书」，
  但部分地区不发放。

## 时间戳

签名脚本默认走 `http://timestamp.digicert.com` 做 RFC 3161 时间戳。
**这一步不能省**：没有时间戳，证书过期后签名立即失效，
用户还得重新下载安装包。

## 与其他阻塞的关系

`ROADMAP.md` Phase 5 的退出标准里，签名是唯一未完成项；
迁移回归与干净机安装测试属于工程验证，可以自行完成。
