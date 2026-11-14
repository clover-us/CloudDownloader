# CloudDownloader 0.6.7 验证记录

验证日期：2026-11-15，Asia/Shanghai。发布版本：[v0.6.7](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.7)，提供 Windows 安装版、便携版与 SHA-256 校验文件。上一版验证见 [0.6.6 更新说明](docs/release-0.6.6.md)，本版更新内容见 [0.6.7 更新说明](docs/release-0.6.7.md)。

## 本次修改

- 任务分组：`Task` 新增 `batch` 字段与 `folder_batch` 生成器（`src-tauri/src/model.rs`），网盘、分享与百度个人盘的文件夹入队时为同一文件夹的子任务写入稳定的组 id（`src-tauri/src/cloud_commands.rs`、`drive_commands.rs`、`lib.rs`）；下载与上传列表按组渲染为一行，聚合进度并支持展开、文件夹级暂停/继续/移除（`src/App.vue`、`src/UploadPanel.vue`、`src/style.css`）。
- 批量操作：新增 `task_batch_action` 与 `upload_batch_action` 命令，按状态过滤适用任务并返回完成/失败清单（`src-tauri/src/lib.rs`、`src-tauri/src/drive_commands.rs`）；前端表头提供全选、批量暂停/继续/移除与清除已完成。
- 文件夹下载解析：`start_task` 复用批次共享的网盘会话，`resolve_tasks_in_background` 以 3 路并发解析；解析完成时在同一把存储锁内核对任务是否已被暂停或移除，避免旧结果覆盖新状态（`src-tauri/src/lib.rs`）。
- 暂停与继续：暂停先落库再调用引擎，忽略 `cannot be paused now`；继续改为 `aria2.unpause`，引擎不支持时回退重建任务；轮询忽略引擎短暂的 `completedLength=0`，避免进度条清零（`src-tauri/src/lib.rs`）。
- 我的网盘搜索：新增 `cloud_search` 命令，非百度网盘以并发 4 路遍历当前目录及子目录（上限 400 个目录、2 万项、1000 条结果）（`src-tauri/src/cloud_commands.rs`）；前端统一为输入框 + 搜索按钮（`src/DriveExplorer.vue`、`src/style.css`）。
- 上传任务：`UploadRecord` 新增 `batch`，文件夹上传写入组信息（`src-tauri/src/upload.rs`、`drive_commands.rs`）；上传列表新增「下载中」筛选与分组渲染（`src/UploadPanel.vue`、`src/cloud.ts`）。
- 设置：六个数字输入框居中，新增框内悬浮上下箭头，支持单击、按住连续加减与 `PageUp` / `PageDown`（`src/SettingsPanel.vue`）。
- 界面：统一三个任务列表的表头与行内边距、图标列与文件夹箭头位置，任务卡片圆角与表头贴合，滚动条不再压住表头右上角（`src/style.css`、`src/App.vue`）。

## 功能验证

- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`：**79 passed / 0 failed / 1 ignored**（忽略项为需要真实账号的 `providers::tests::live_share_listing`）。新增 `model::tests::folder_batch_is_stable_per_folder_and_directory`，校验同一文件夹与目录生成相同组 id、不同下载目录生成不同组 id。
- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check --all-targets` 通过。
- `npm run build`（`vue-tsc --noEmit` + Vite 生产构建）通过，产物 `dist/assets/index-*.js` 约 233 kB。
- 浏览器自动化 `node harness/test.mjs`：**61 项全部通过**，覆盖任务列表分组、展开/收起、聚合进度、全选与批量暂停、清除已完成、移除确认弹窗、上传任务批量与「下载中」筛选、网盘统一搜索框与搜索调用、920px 窄窗无横向溢出，以及整页无控制台错误。
- 桌面端手动验证（Windows，真实账号）：
  - 在「我的网盘」（迅雷网盘）下载一个含 3 个文件的文件夹，暂停后三个子任务全部进入暂停、提示「已暂停 3 个任务」且无错误横幅；继续后恢复。
  - 在「我的网盘」（百度网盘）下载含 2 个文件的文件夹，在地址解析阶段点击暂停，两个子任务均被暂停且未启动下载；继续后小文件完成、大文件恢复传输。
  - 使用本机限速 HTTP 服务验证断点续传：任务在 33.3% 暂停后继续，每 300 ms 采样进度为 `33.3 → 33.5 → 34.0 → … → 37.3`，无清零；任务记录的 `gid` 与已下载字节在继续前后保持连续。
  - 迅雷 3 文件文件夹的继续/暂停、百度文件夹在解析阶段暂停均在真实界面复现验证。
- 设置箭头在真实界面通过 UI Automation 点击验证：同时下载数 3 → 4 → 3，全局限速 0 → 0.5 → 0，数据库设置项随之更新；验证结束后恢复为 0 与 3。

## 验证范围

- 协议层仍未调用真实接口的部分与 0.6.6 相同；本次改动集中在本地任务编排、界面与搜索遍历，不改变各网盘取链协议。
- 真实账号覆盖当前登录的百度网盘与迅雷网盘；夸克、115、123、139、UC 及蓝奏系的递归搜索与文件夹分组沿用同一实现，未逐账号回归。
- 迅雷网盘个别文件仍返回「迅雷未返回可用的下载地址，请刷新后重试」，该提示来自服务端取链结果，非本版回归。
- 历史任务记录不含 `batch` 字段，升级后按单个任务显示；本版只对升级后新发起的文件夹任务分组。

## 构建与交付

- 本版代码已通过上述格式、静态检查、单元测试、浏览器自动化与前端生产构建，并由 `scripts/build.ps1` 生成 Windows 安装包、便携包、签名文件与 `latest.json`。
- 安装包与便携包发布在 [v0.6.7 Release](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.7)，校验文件为 `SHA256SUMS-0.6.7.txt`。

## 历史验证

0.6.6 及更早版本的验证记录见对应版本的更新说明与 [v0.6.6 Release](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.6)。