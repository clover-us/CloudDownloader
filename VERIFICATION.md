# CloudDownloader 0.6.0 验证记录

验证日期：2026-11-14，Asia/Shanghai。发布版本：[v0.6.0](https://github.com/sclliang/CloudDownloader/releases/tag/v0.6.0)，提供 Windows 安装版、便携版与 SHA-256 校验文件。上一版验证见 [0.5.7](docs/verification-0.5.7.md)，更新内容见 [0.6.0 更新说明](docs/release-0.6.0.md)。

## 本次修改

- 新增「设置」页，分为通用、传输、外观、关于与更新四组；可从侧栏进入，设置项包括下载目录、关闭窗口行为、全局限速、同时下载数、默认连接数、界面主题与自动检查更新。
- 设置写入任务数据库的 `settings` 表，启动时读入并作为 aria2 启动参数；修改限速与同时下载数后立即通过 `aria2.changeGlobalOption` 应用到运行中的引擎，不中断已有任务。
- 关闭窗口行为可配置：默认收进托盘继续下载；选择「退出程序」时先保存 115 会话，再退出主程序与 aria2 子进程。
- 新增自动更新：启动时静默检查 GitHub Releases 上的 `latest.json`，发现新版本在侧栏「设置」显示圆点，可在设置页下载、安装并重启；更新包使用 minisign 公钥校验签名。
- 构建产出 `.sig` 签名与 `latest.json` 清单，发布脚本自动读取本地私钥，并在打包后生成清单。
- 版本号统一升级到 0.6.0；同时修复 rustc 1.100 下 clippy 报出的既有告警（多余借用、循环内重复编译正则、手写除法检查等），不改变运行行为。

## 功能验证

- 设置页在真实 WebView2 窗口中逐项操作通过：四组卡片渲染正确，下载目录显示当前路径且「更改／打开」可用；关闭行为分段切换时说明文案同步变化；限速数字框与 1／5／10 MiB/s 快捷项联动；同时下载数只能取 1–10；默认连接数只在 1／4／8／16 中选择；日间与 Night 切换后窗口背景、侧栏、卡片与控件配色一致。
- 即时生效：设为 5 MiB/s 后 `aria2.getGlobalOption` 返回 `max-overall-download-limit=5242880`；同时下载数改为 4 后返回 4；改回不限速与 3 后返回 0 与 3。全程没有中断已存在的任务。
- 持久化：重启程序后设置页仍显示上次的选择，aria2 启动参数为 `--max-overall-download-limit=5120K` 与 `--max-concurrent-downloads=3`，且不再出现重复参数。
- 关闭行为：选择「退出程序」后关闭窗口，主程序与 aria2 子进程均退出；选择「最小化到托盘」后进程保留、窗口隐藏，再次启动程序时由单实例机制恢复原窗口。
- 自动更新：未发布清单时点击「检查更新」提示「尚未发布可用的更新清单，请稍后再试。」；启动时的静默检查失败不改变界面状态，设置页保持「尚未检查更新」。
- 主题：Night 与日间来回切换，标题栏按钮、窗口背景与设置页控件同步变化，选择保存在本地存储。
- 未新增或修改下载协议逻辑；0.5.7 的直链、网盘与 BT 端到端能力沿用既有测试。

## 验证范围

- 未使用真实网盘账号做登录与下载回归；各网盘的账号与传输能力沿用 0.5.7 的验证记录。
- 自动更新的「下载—安装—重启」完整链路需要先发布 0.6.0，并在已安装的 0.6.0 上由后续版本触发；本轮验证到检查更新、签名产物与清单生成为止。
- 未做旧版本覆盖安装验证；任务数据库结构未变，0.5.x 的任务与账号资料理论上可直接沿用。
- 界面检查基于本机 1196×831 窗口与 100% 缩放，未覆盖多视口布局矩阵。

## 构建与交付

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 与 `cargo test --lib` 通过：69 passed / 0 failed / 1 ignored（需要真实分享链接的用例被忽略）。
- `npm run build`（`vue-tsc` + Vite 生产构建）通过。
- 带签名的 Windows release 与 NSIS 构建通过，打包脚本核对了主程序与安装包的版本资源；最终交付的安装包是重新编译并改写版本资源的那一份，脚本对它单独签名，`CloudDownloader_0.6.0_x64-setup.exe.sig` 与 `latest.json` 中的签名一致。
- 交付文件：package/CloudDownloader.exe（11.0 MiB）、package/CloudDownloader_0.6.0_x64-setup.exe（5.65 MiB）、package/CloudDownloader_0.6.0_x64-setup.exe.sig、package/CloudDownloader-0.6.0-win-x64.zip（7.4 MiB）、package/SHA256SUMS-0.6.0.txt、package/latest.json；SHA-256 校验文件覆盖安装包与 ZIP。
- 本轮未安装或卸载用户已有的 0.5.x 应用，升级路径留给正式发布后的自动更新验证。
