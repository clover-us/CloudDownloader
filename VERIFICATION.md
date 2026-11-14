# CloudDownloader 0.6.4 验证记录

验证日期：2026-11-14，Asia/Shanghai。发布版本：[v0.6.4](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.4)，提供 Windows 安装版、便携版与 SHA-256 校验文件。上一版验证见 [0.6.3 更新说明](docs/release-0.6.3.md)，本版更新内容见 [0.6.4 更新说明](docs/release-0.6.4.md)。

## 本次修改

- 将「我的网盘」重构为统一的文件管理器面板（`src/DriveExplorer.vue`），百度与其余网盘共用同一套界面与交互，按各网盘能力显示 / 置灰功能。
- 新增批量选择栏、行悬浮操作（下载 / 分享 / 重命名 / 删除）、单行左对齐并可折叠的面包屑，以及列表 / 网格视图切换。
- 新增上传菜单（上传文件 / 上传文件夹）、排序面板（修改时间 / 创建时间 / 文件名称 / 文件大小，双向）、筛选面板（按文件类型多选）。
- 新增后端聚合接口 `dir_list_all`，支持跨分页对整个目录排序与筛选（上限 2 万项，超出标记截断）。
- 上传文件夹保留本地子目录结构：逐级创建云端目录并按目录分组上传，新增目录级进度卡片；百度与迅雷均支持新建文件夹。
- 为 `DriveFile` 补充 `created`（创建时间）字段，用于显示与排序；个别网盘接口不返回创建时间时回退为修改时间。
- 下载文件夹改为「先建任务、后台解析」：`drive_enqueue` / `cloud_enqueue` 保存任务后立即返回，由 `resolve_tasks_in_background` 在后台逐个解析下载地址，任务列表实时更新。

## 功能验证

- 自动化界面回归（`harness/test.mjs`，Playwright + 本机 Edge，1180×822）：共 48 项检查全部通过，覆盖账号对话框、网盘切换、文件列表加载、批量选择栏、行悬浮操作显隐、上传 / 排序 / 筛选菜单、排序与筛选生效、目录级上传进度、深层面包屑折叠等。
- 关键截图：`harness/shots/05c-drive-baidu.png`（工具栏与列表）、`05e-drive-grid.png`（网格视图）、`05g-drive-sort-created.png`（按创建时间倒序）、`05d-drive-baidu-hover.png`（行悬浮操作）、`06-deep-crumbs.png`（多级面包屑折叠）、`07-folder-progress.png`（上传文件夹目录级进度）。
- 真实账号手动验证：使用真实的百度网盘与迅雷网盘账号打开「我的网盘」，确认文件浏览、目录进入、面包屑返回、批量选择与下载入口正常；点击文件夹下载后任务立即出现在列表，不再长时间转圈。

## 验证范围

- 真实账号仅覆盖百度网盘与迅雷网盘；夸克、115、123、139、UC、蓝奏云与优享版使用模拟数据验证界面与交互，未做真实账号回归。
- 上传文件夹、新建文件夹与目录级进度通过模拟接口验证完整流程；真实云端创建与分片上传行为沿用既有上传验证，未在本版重复进行大规模真实上传。
- 重命名与删除因后端暂无接口，界面中置灰，未做功能验证；创建分享仅在百度网盘可用。
- 全库排序 / 筛选在单项超过 2 万条时会标记为「已截断」，此边界未用超大真实目录验证。

## 构建与交付

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check --all-targets` 通过。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`：69 passed / 0 failed / 1 ignored（需要真实分享链接的用例被忽略）。
- `npm run build`（`vue-tsc --noEmit` + Vite 生产构建）通过。
- 带签名的 Windows release 与 NSIS 构建通过，打包脚本核对了主程序版本资源（0.6.4.0），最终交付的安装包由 makensis 重编译并单独签名，`.sig` 与 `latest.json` 中的签名一致。
- 交付文件见 GitHub Release [v0.6.4](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.4)：`CloudDownloader_0.6.4_x64-setup.exe`、便携包 `CloudDownloader-0.6.4-win-x64.zip`、`SHA256SUMS-0.6.4.txt` 与 `latest.json`。

## 历史验证

0.6.3 及更早版本的验证记录见对应版本的更新说明与 [v0.6.3 Release](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.3)。
