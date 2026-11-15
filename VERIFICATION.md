# CloudDownloader 验证记录

本文档随应用一起分发给用户（安装包内与便携包根目录）。当前验证对象为 0.7.9 代码库，验证日期 2026-11-15，Asia/Shanghai。

## 本版加固

本次改动集中在稳定性、安全边界与清理，不改变各网盘的取链协议与界面布局：

- **浏览器接力**：本机 HTTP 桥对同时接入的连接数设上限（16），超限直接返回 503 而不是继续创建线程（`src-tauri/src/bridge.rs`）；`/add` 只接受本应用扩展的 Origin，未安装或身份不符时返回 403 并记录原因（比对 `extension/id.txt`）；请求体超过 64 KiB 返回 413，不再静默截断。
- **地址边界**：浏览器接力与 `clouddownloader://add` 拒绝回环、内网、链路本地、CGNAT 与单标签主机名，避免应用被用作本机端口探测器（`src-tauri/src/direct.rs`、`handoff.rs`）。手动在下载框粘贴的地址不受此限制，与既有行为一致。
- **锁与主线程**：移除任务时先在锁内取快照、出锁后再删文件；「打开目录」出锁后才调用系统打开器；轮询每轮只读一次任务表，写库前按 `id` 单行复核并仅在字段变化时落库（`src-tauri/src/lib.rs`）。
- **引擎自愈**：aria2 进程退出后 `rpc()` 快速失败，轮询触发一次受 30 秒节流的重启，重启成功把进行中的任务转为已暂停并提示恢复，不再出现"引擎已死但界面还在转圈"（`src-tauri/src/lib.rs`）。
- **数据一致性**：任务表整表反序列化改为逐行容错，单条坏记录只跳过该条；保存设置包在 `BEGIN IMMEDIATE` 事务里；`uploads`/`shares` 的建表只在打开数据库时执行一次（`src-tauri/src/store.rs`、`settings.rs`）。
- **磁力解析**：解析任务不再跨 `tellStatus` 与清理过程持锁，取消操作立即返回（`src-tauri/src/download_commands.rs`）。
- **本地文件夹选择**：`collect_local_files` 补齐主窗口校验，跟随 junction 时用规范化路径去重，并限制深度 32 层、2 万个文件（`src-tauri/src/drive_commands.rs`）。
- **前端竞态**：首次加载失败不再让账户列表与 4 秒轮询停摆；文件夹轮询在组件卸载后不再续排；搜索请求自增版本号，迟到的结果不会覆盖新目录；全选只作用于列表上可见的行，页面计数与列表单位一致（`src/App.vue`、`src/DriveExplorer.vue`）。
- **浏览器扩展**：磁力链接接力改为真正可用（跳过 cookie 与 referer 采集），此前会被自身校验拦下并误报失败（`extension/background.js`）。
- **清理**：移除未被引用的 `src/DrivePanel.vue`、`src/CloudDrivePanel.vue`、`harness/extension-demo.mjs` 与移动端图标模板；删除孤儿命令 `settings_get`；便携包补入 `extension/` 目录（此前便携版的「扩展目录」与 `/update.xml` 会 404）；去掉无来源的 `--force-renderer-accessibility` 启动参数。

## 功能验证

- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`：**99 passed / 0 failed / 1 ignored**（忽略项为需要真实账号的 `providers::tests::live_share_listing`）。新增 `bridge::tests::connection_slots_are_bounded`，校验连接名额占满后拒绝接入、释放后可重新获取。
- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check --all-targets` 通过。
- `npm run build`（`vue-tsc --noEmit` + Vite 生产构建）通过，产物 `dist/assets/index-*.js` 约 245 kB（gzip 约 80 kB）。
- `npm run test:ui`（`harness/` 的 mock 后端 + Playwright 驱动本机 Edge）：**91 项检查全部通过**，末尾 `Z1 no page errors` 确认页面没有脚本报错。覆盖账户与登录态、网盘浏览与搜索、任务列表与文件夹分组、批处理、上传、设置与浏览器接力面板、接力确认窗等界面路径。
- 开发版真机核对（`npm run tauri dev`，Windows）：应用与 aria2 1.37.0 子进程正常启动；桥监听 `127.0.0.1:17890`，真实扩展 Origin 取 `/update.xml` 返回 200、伪造 Origin 返回 403；`/add` 对 `127.0.0.1` 与 `169.254.169.254` 返回 400、对真实 Origin 的公网链接返回 200；杀掉 aria2 后约 8 秒自动重启且 RPC 可用；`clouddownloader://` 深链接不产生第二个实例；`tasks.sqlite3` 建表与设置默认值符合预期。

## 验证范围

- 未覆盖九个网盘的登录与真实取链（需要真实账号）、桌面窗口上的点击级端到端下载（无头环境无法操作窗口）、安装包升级路径，以及 DNS rebinding 防护。
- 协议层与 0.6.7 相同；本次改动不触及各网盘取链协议。
- DNS rebinding 未做防护：接力拒绝的是字面地址与主机名，无法阻止解析到内网 IP 的公网域名。

## 历史验证

0.6.7 及更早版本的逐项验证记录见各版本更新说明（如 [0.6.7 更新说明](docs/release-0.6.7.md)）与对应 Release。