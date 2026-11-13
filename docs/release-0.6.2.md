# CloudDownloader 0.6.2

0.6.2 重做了账号管理与「我的网盘」两个页面：现在只列出已登录的网盘，不再把八个网盘全部铺出来；需要新网盘时从统一的「添加」入口选择。同时补上了退出登录，并修复了登录过程中的两处状态问题。

## 更新内容

- 账号管理页只展示已连接的网盘：每行显示网盘、账号名与连接状态，可直接进入网盘、刷新登录状态或退出登录；未登录时给出空状态与添加入口。
- 新增「添加网盘账号」弹窗：搜索 + 按「已连接 / 未登录」分组列出全部网盘，点击未登录的网盘即打开官方登录窗口，登录成功后自动连接。
- 「我的网盘」同样只显示已连接的网盘，切换条末尾提供「添加网盘」入口；登录成功后自动切换到新连接的网盘。
- 新增退出登录：清除该网盘在本机的登录信息（115 同时清除会话缓存），已有下载任务不受影响。
- 修复：点开登录窗口但尚未真正登录时，网盘不再提前出现在切换条里。
- 修复：关闭登录窗口后，「等待登录」状态会及时取消，不再一直等待。
- 设置中心、自动更新、登录窗口按站点宽度开窗等 0.6.x 功能保持不变。

## 下载

- [Windows 安装版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.2/CloudDownloader_0.6.2_x64-setup.exe)：适合日常使用，自动创建快捷方式，支持应用内更新。
- [Windows 便携版](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.2/CloudDownloader-0.6.2-win-x64.zip)：解压后运行根目录的 `CloudDownloader.exe`，保留完整目录结构。
- [SHA-256 校验文件](https://github.com/clover-us/CloudDownloader/releases/download/v0.6.2/SHA256SUMS-0.6.2.txt)。

需要 Windows 10/11 x64 和 Microsoft Edge WebView2 Runtime。0.6.0 及之后的安装版可以通过应用内的自动更新升级，登录资料与任务记录保留。

## 说明

退出登录只清除对应网盘的登录资料，不影响其他网盘，也不会删除已创建的下载任务。