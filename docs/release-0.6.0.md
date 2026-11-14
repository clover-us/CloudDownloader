# CloudDownloader 0.6.0

新增设置中心与自动更新，下载参数、窗口行为和版本升级都可以在应用内完成。

- 新增「设置」页：通用（下载目录、关闭窗口时收进托盘或退出程序）、传输（全局限速、同时下载数、默认连接数）、外观（日间 / Night）与关于四组。
- 设置改动即时保存：限速与同时下载数通过 aria2 的 `changeGlobalOption` 立即生效，无需重启；新任务的默认连接数也随之更新。
- 全局限速支持 0（不限速）与自定义 KiB/s，并提供 1 / 5 / 10 MiB/s 快捷项；同时下载数限制在 1–10。
- 关闭窗口的行为可配置，默认仍收进托盘继续下载；选择「退出程序」时会在保存 115 会话后退出。
- 新增自动更新：启动时静默检查 GitHub Releases 上的 `latest.json`，发现新版本在「设置」上显示提示，可下载、安装并自动重启；更新包使用 minisign 公钥校验签名。
- 安装包同时产出 `.sig` 签名与 `latest.json` 清单，发布流程见[开发文档](development.md)。
- 版本号统一升级到 0.6.0，任务数据库与账号资料保持兼容，升级不会要求重新登录。

## 下载

- [Windows 安装版](https://github.com/sclliang/CloudDownloader/releases/download/v0.6.0/CloudDownloader_0.6.0_x64-setup.exe)：适合日常使用，自动创建快捷方式。
- [Windows 便携版](https://github.com/sclliang/CloudDownloader/releases/download/v0.6.0/CloudDownloader-0.6.0-win-x64.zip)：解压后运行根目录的 `CloudDownloader.exe`，保留完整目录结构。
- [SHA-256 校验文件](https://github.com/sclliang/CloudDownloader/releases/download/v0.6.0/SHA256SUMS-0.6.0.txt)。

需要 Windows 10/11 x64 和 Microsoft Edge WebView2 Runtime。安装版会在缺少运行时时引导下载。从 0.6.0 起的安装版可以直接通过应用内的自动更新升级，之后的版本会保留原有登录资料与任务。

## 验证

Rust 单元测试覆盖设置校验、越界值回退、SQLite 往返持久化，以及真实 aria2 上「启动即生效 + 运行时修改全局选项」的两段路径；前端通过 `vue-tsc` 与生产构建；桌面端实际打开设置页、修改限速与关闭行为、重启后确认持久化。

## 说明

自动更新只对 0.6.0 及之后的安装版生效；便携版仍需手动替换程序文件。更新源为 GitHub Releases，首次检查需要能够访问 GitHub。
