# CloudDownloader 0.6.1

0.6.1 修复网盘登录窗口显示不完整的问题。八个网盘的登录页现在共用同一套宽度适配逻辑，页面按窗口宽度等比缩放，右侧菜单与表格列不再被裁掉，也不再出现各家站点滚动条表现不一致的情况。

## 更新内容

- 登录窗口对所有网盘统一适配：页面加载完成后测量实际布局宽度，按比例缩小到刚好铺满窗口，右侧内容完整可见。
- 之前只有蓝奏云刚好放得下，夸克、百度、115、123、UC、139 与蓝奏云优享版的页面都比 1000 像素的窗口宽，导致右侧被裁；各家站点自己的 `overflow` 样式不同，于是有的出现横向滚动条、有的直接截断。现在统一由程序缩放，不再依赖站点自身的滚动条行为。
- 缩放在页面加载、字体与图片到位以及窗口尺寸变化时重新计算，最小保留 0.5 倍，窗口尺寸与之前保持一致。
- 设置中心、自动更新、关闭窗口行为等 0.6.0 功能保持不变；已开启自动更新的 0.6.0 安装版可以直接在应用内升级。

## 下载

- [Windows 安装版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.1/CloudDownloader_0.6.1_x64-setup.exe)：适合日常使用，自动创建快捷方式，支持应用内更新。
- [Windows 便携版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.1/CloudDownloader-0.6.1-win-x64.zip)：解压后运行根目录的 `CloudDownloader.exe`，保留完整目录结构。
- [SHA-256 校验文件](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.1/SHA256SUMS-0.6.1.txt)。

需要 Windows 10/11 x64 和 Microsoft Edge WebView2 Runtime。0.6.0 及之后的安装版可以通过应用内的自动更新升级，登录资料与任务记录保留。

## 说明

登录窗口仍然是固定尺寸的独立窗口（1000 × 740），本次通过缩放页面内容适配，不改动窗口尺寸；如果之后某个网盘改版后页面更宽，缩放会在加载时自动重新计算。
