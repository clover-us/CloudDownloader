# CloudDownloader 0.6.1

0.6.1 修复网盘登录窗口显示不完整的问题。八个网盘的登录页现在按各自桌面版页面的实际布局宽度开窗，页面保持 100% 渲染，右侧菜单、表格列与二维码区域不再被裁掉；滚动条也统一成同一套细圆角样式。

## 更新内容

- 登录窗口按站点实际布局宽度开窗：蓝奏云 1024、139 云盘 1044、夸克 1072、115 1116、123 与蓝奏云优享版 1236、百度 1280、UC 1296，高度仍为 740；窗口宽度还会按屏幕可用宽度收敛，避免小屏溢出。
- 之前八个网盘共用 1000 像素宽的窗口，而各家页面需要 1000–1275 像素，只有蓝奏云刚好放得下，其余七家右侧被裁。上一版尝试整页等比缩小，但 139 云盘这类「居中 + 固定宽度」布局会被缩放带偏、左侧文案反而被裁，本版改为按需开窗，不再缩放页面。
- 统一滚动条样式：10 像素宽、透明轨道、圆角半透明滑块、隐藏两端箭头。样式通过 `adoptedStyleSheets` 注入，绕开站点 CSP 对 `<style>` 的拦截；站点若用 `!important` 强行改写自己的滚动条，则保留站点外观。
- 设置中心、自动更新、关闭窗口行为等 0.6.0 功能保持不变；已开启自动更新的 0.6.0 安装版可以直接在应用内升级。

## 下载

- [Windows 安装版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.1/CloudDownloader_0.6.1_x64-setup.exe)：适合日常使用，自动创建快捷方式，支持应用内更新。
- [Windows 便携版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.1/CloudDownloader-0.6.1-win-x64.zip)：解压后运行根目录的 `CloudDownloader.exe`，保留完整目录结构。
- [SHA-256 校验文件](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.1/SHA256SUMS-0.6.1.txt)。

需要 Windows 10/11 x64 和 Microsoft Edge WebView2 Runtime。0.6.0 及之后的安装版可以通过应用内的自动更新升级，登录资料与任务记录保留。

## 说明

登录窗口仍是独立窗口，尺寸随网盘不同（1044–1296 × 740），不再统一为 1000 × 740。如果之后某个网盘改版后页面更宽，可以调整 `src-tauri/src/session.rs` 中 `login_size()` 的数值。
