# CloudDownloader 0.6.3

0.6.3 新增「迅雷网盘」支持，并修复了多个网盘「修改时间」不显示的问题。迅雷网盘现已排在百度网盘之后，支持个人网盘浏览、文件下载、以及上传（含分享链接解析下载）。

## 更新内容

- 新增迅雷网盘：支持打开官方网页登录，登录后浏览个人网盘目录、下载文件到本地，并可将本地文件上传到迅雷网盘。网盘顺序排在百度网盘之后。
- 迅雷网盘下载支持分享链接解析，自动识别分享地址与提取码，下载走迅雷加速直链。
- 修复：123 云盘、139 云盘、迅雷网盘的「修改时间」列此前一直显示为「—」，现已按各自接口字段正确显示。
- 修复：蓝奏云、蓝奏云优享版的「修改时间」此前写死为空，现尽力按接口返回解析（若接口本身不返回时间则仍显示「—」）。
- 兼容 0.6.x 的账号管理、我的网盘、设置中心与应用内自动更新。

## 下载

- [Windows 安装版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.3/CloudDownloader_0.6.3_x64-setup.exe)：适合日常使用，自动创建快捷方式，支持应用内更新。
- [Windows 便携版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.3/CloudDownloader-0.6.3-win-x64.zip)：解压后运行根目录的 `CloudDownloader.exe`，保留完整目录结构。
- [SHA-256 校验文件](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.3/SHA256SUMS-0.6.3.txt)。

需要 Windows 10/11 x64 和 Microsoft Edge WebView2 Runtime。0.6.0 及之后的安装版可以通过应用内的自动更新升级，登录资料与任务记录保留。

## 说明

迅雷网盘使用官方网页登录并复用浏览器中的登录令牌，登录窗口关闭后账户仍保持连接。上传采用分片直传对象存储，单个文件当前会整体读入内存后再上传，超大文件请留意内存占用。
