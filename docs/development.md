# 开发与构建

以下命令均在仓库根目录执行。

环境：Windows x64、Node.js、Rust MSVC、Visual Studio C++ Build Tools、WebView2 Runtime。

```powershell
npm ci
npm run tauri dev -- --no-watch
```

实际账号验证时使用 `--no-watch`，避免编辑 Rust 源码时重启登录窗口。普通开发可移除该选项。

从托盘菜单退出开发版后运行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
```

脚本执行格式检查、Rust 测试、前端和 Tauri NSIS 发布构建，并生成中文安装包、便携版 ZIP 和 SHA-256 校验文件。所有产物位于 `package/`；主程序为 `package/CloudDownloader.exe`，引擎与许可资料位于 `package/bin/`。安装版采用相同的 bin 布局，安装与升级保留原账号资料和任务。卸载默认保留这些资料，只有主动勾选清除应用数据才会删除。打包脚本核对主程序及安装包的公司、版权、英文功能说明、四段版本、原始文件名和英语（美国）资源语言。

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

测试包含真实 aria2 与本地 HTTP 服务的多连接并发、暂停、引擎重启、更换地址续传、非分段服务器回退和逐字节一致性验证，以及上传分片重试、源文件变化检测、检查点恢复、旧数据库迁移、网盘识别、错误处理、下载域名边界和 Windows 文件名处理。真实百度上传、续传、下载和分享的验证结果见[验证记录](../VERIFICATION.md)；自动测试不会使用网盘账号。设置相关的测试覆盖越界值回退、SQLite 往返持久化，以及真实 aria2 上「启动参数生效 + 运行时 `changeGlobalOption`」两条路径。

## 发布与自动更新

安装版通过 `tauri-plugin-updater` 从 GitHub Releases 的 `latest.json` 检查更新，安装包使用 minisign 签名校验。

首次准备签名密钥（私钥只留在本机，公钥写入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/updater-key.ps1
```

发布新版本的步骤：

1. 同步升级 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 中的版本号，并在 `docs/release-<版本>.md` 写明更新说明。
2. 执行 `scripts/build.ps1`。脚本会读取 `~/.tauri/clouddownloader.key`（或 `TAURI_SIGNING_PRIVATE_KEY`），构建 NSIS 安装包，对 `package/` 中最终交付的安装包重新签名，并生成 `.sig` 与 `package/latest.json`；清单里的更新说明默认取自 `docs/release-<版本>.md`。
3. 在 GitHub 创建 `v<版本>` Release，上传 `package/` 下的 `CloudDownloader_<版本>_x64-setup.exe`、同名 `.sig`、`latest.json`，以及 `CloudDownloader-<版本>-win-x64.zip` 与 `SHA256SUMS-<版本>.txt`。
4. `latest.json` 中的 `url` 指向该 Release 的安装包地址，客户端只会接受与公钥匹配的签名。

注意事项：

- 安装包在打包时会被重新编译以修正版本资源，字节与 Tauri 的原始产物不同，所以签名必须针对最终文件：`package.ps1` 已自动完成这一步，手工发布时不要改用 `src-tauri/target/release/bundle/nsis/` 里的中间产物。
- 自动更新只对 0.6.0 及之后的 **安装版** 生效；便携版和开发模式（`tauri dev`）不会自我替换。
- 私钥或密码丢失后无法再发布可被旧版本接受的更新，只能让用户手动安装新包并重新分发公钥。
- 换用新密钥时，需要同时更新 `tauri.conf.json` 的 `pubkey` 并在 Release 中重新签名安装包。

## 目录

- `src/`：Vue 界面与样式。
- `src-tauri/src/providers/`：多网盘路由与百度接口。
- `src-tauri/src/quark.rs`：夸克接口。
- `src-tauri/src/session.rs`：独立登录窗口、后台会话读取。
- `src-tauri/src/migration.rs`：旧资料迁移。
- `src-tauri/src/lib.rs`：任务调度和 Tauri 命令。
- `src-tauri/src/engine.rs`：aria2 进程、RPC 和传输测试。
- `src/DownloadBox.vue`：统一下载输入、文件预览与 BT 参数界面。
- `src-tauri/src/bt.rs`、`download_commands.rs`：链接分类、种子校验、磁力元数据和 BT 任务。
- `src-tauri/src/handoff.rs`：浏览器 `clouddownloader://` 协议的解析、链接队列与窗口唤醒。
- `src-tauri/src/bridge.rs`：浏览器扩展用的本机 HTTP 接口（仅扩展来源，接收链接与请求头）。
- `extension/`：Chrome/Edge 扩展，接管浏览器下载并把 Cookie、来源页面、浏览器标识交给桌面端。
- `src-tauri/src/engine_bt_tests.rs`：真实本地 Tracker／aria2 节点、选择下载及重启续传测试。
- `src-tauri/src/direct.rs`、`direct_commands.rs`：普通链接检测、文件名与续传保护、创建下载任务。
- `src-tauri/src/tray.rs`：托盘图标、恢复窗口和退出菜单。
- `src-tauri/src/store.rs`：SQLite 持久化。
- `src-tauri/src/settings.rs`：偏好设置的结构、校验、持久化与 aria2 全局选项映射。
- `src/SettingsPanel.vue`、`src/settings.ts`：设置页界面与前端类型。
- `src/updater.ts`：自动更新的检查、下载、安装与重启流程。
- `src-tauri/src/providers/baidu_drive.rs`：百度个人网盘、上传与分享接口。
- `src-tauri/src/drive_commands.rs`：个人文件和上传任务命令。
- `src-tauri/src/upload.rs`：分片并发、重试、暂停续传和文件校验。
- `src-tauri/src/verification.rs`：仅开发构建启用的真实账号验证程序；只上传生成的测试文件。
- `third-party/notices/`：第三方许可资料。
- `scripts/updater-key.ps1`、`scripts/latest-json.ps1`：自动更新签名密钥与发布清单生成。
- `scripts/pack-extension.ps1`：把 `extension/` 打成可上传商店或解压安装的 zip（`package/clouddownloader-extension-<版本>.zip`）。
- `extension/`：浏览器扩展（MV3），监听 `chrome.downloads.onCreated` 并通过 `127.0.0.1:17890` 与 `src-tauri/src/bridge.rs` 通信。

aria2 的固定版本和校验值见 `third-party/notices/README.txt`。本项目未复制或运行 LinkSwift 脚本。

[返回项目首页](../README.md) · [使用指南](usage.md) · [验证记录](../VERIFICATION.md)
