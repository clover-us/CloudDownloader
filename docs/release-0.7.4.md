# CloudDownloader 0.7.4

0.7.4 修掉插件面板「打开扩展文件夹」一直报 `Not allowed to open path` 的问题。

## 修复

- **打开扩展文件夹不再走 opener 插件**：插件的路径 scope 只对少数根目录放行，装到 `D:\softwares\CloudDownloader` 这类自定义路径时会被拒绝，去掉 `\\?\` 前缀也不够。现在改为由应用直接调用系统资源管理器打开目录，不再经过插件的路径校验。
- 插件面板新增「扩展目录」一行，直接显示扩展所在位置，方便手动去 `edge://extensions` 加载。

## 说明

- 安装与升级沿用 0.7.2 的改动：引擎从 `%LOCALAPPDATA%\CloudDownloader\engine\` 运行，安装目录里的 `bin\aria2c.exe` 不再被占用。
- 扩展没有变化，仍是 0.2.0。