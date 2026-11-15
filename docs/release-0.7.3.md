# CloudDownloader 0.7.3

0.7.3 修掉 0.7.2 里「打开扩展文件夹」按钮的报错：`Not allowed to open path \\?\D:\...`。

## 修复

- **打开扩展文件夹被拒绝**：Windows 的资源目录和 `canonicalize` 返回的是 verbatim 路径（`\\?\D:\...`），打开器只接受普通路径。现在把 `\\?\` 前缀（以及 `\\?\UNC\` 形式）去掉再交给前端，按钮可以正常打开扩展目录了。
- 顺带加了路径规范化的单元测试，覆盖普通盘符路径、verbatim 路径和 UNC 路径三种形式。

## 说明

- 安装与升级流程沿用 0.7.2 的改动：引擎从 `%LOCALAPPDATA%\CloudDownloader\engine\` 运行，安装目录里的 `bin\aria2c.exe` 不再被占用。
- 扩展没有变化，仍是 0.2.0。