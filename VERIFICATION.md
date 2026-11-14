# CloudDownloader 0.6.6 验证记录

验证日期：2026-11-14，Asia/Shanghai。发布版本：[v0.6.6](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.6)，提供 Windows 安装版、便携版与 SHA-256 校验文件。上一版验证见 [0.6.5 更新说明](docs/release-0.6.5.md)，本版更新内容见 [0.6.6 更新说明](docs/release-0.6.6.md)。

## 本次修改

- 补齐「我的网盘」预留能力：百度、迅雷、夸克支持重命名与删除；夸克新增上传、新建文件夹与创建分享；迅雷新增创建分享（`src-tauri/src/drive_commands.rs`、`providers/{cloud,baidu_drive,xunlei}.rs`、`src/DriveExplorer.vue`、`src/providers.ts`）。
- 上传内核重做：流式实时进度、同名自动追加序号、重复提交拦截；迅雷/夸克大文件对象存储分片与断点续传，分片失败回退整文件上传（`src-tauri/src/upload.rs`）。
- 上传分片大小（1–32 MiB）与并发（同时上传数、单文件分片并发）可配置；分片/进度/续传账本对百度、迅雷、夸克统一（`settings.rs`、`src/SettingsPanel.vue`）。
- 下载侧：「默认连接数」经 `Engine::add_cloud` 下发到网盘任务，新增「最小分段大小」；全局限速单位改为 MiB/s；设置表单仅在值变化时同步（`engine.rs`、`lib.rs`）。

## 功能验证

- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`：**78 passed / 0 failed / 1 ignored**。新增协议用例覆盖夸克新建文件夹、夸克/迅雷/百度重命名与删除、迅雷创建分享、夸克创建分享；用例通过本地 mock 服务器断言请求路径、查询参数与请求体（`src-tauri/src/providers/protocol_tests.rs`）。
- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check --all-targets` 通过。
- `npm run build`（`vue-tsc --noEmit` + Vite 生产构建）通过，产物 `dist/assets/index-*.js` 约 209 kB。
- 运行桌面端手动验证：在「我的网盘」（迅雷网盘）确认重命名、删除、创建分享入口与弹窗可用；在「设置 → 传输」确认新增「最小分段大小 / 上传分片大小 / 同时上传数 / 单文件分片并发」四项与「MiB/s」单位正常显示（截图 `screenshot14.png`）。
- 上传实测：通过界面提交文件，确认进度条随已传字节实时增长、同名文件自动追加序号、重复提交被拦截；`285.8 MiB` 大文件在放宽超时后不再报「连接迅雷云端存储失败」。

## 验证范围

- 协议层使用 mock 服务器验证，未调用真实网盘接口；重命名/删除/分享的请求结构以公开实现为参考（百度 PCS、夸克云盘、迅雷云盘）。
- 真实账号仅覆盖当前登录的迅雷网盘；百度与夸克的重命名/删除/创建分享未在真实账号上回归。
- 夸克 `x-oss-hash-ctx` 多分片与迅雷 OSS 分片未经完整真机回归；分片失败会回退整文件上传，最坏情况退化为 0.6.5 的整文件行为。
- 自动化界面回归（`harness/test.mjs`）本版未重跑：它需要 Playwright 与占用 1420 端口的 vite mock 服务，与开发窗口冲突；界面改动以手动验证与生产构建为准。

## 构建与交付

- 本版代码已通过上述格式、静态检查、单元测试、协议测试与前端生产构建。
- Windows 安装包与便携包由发布流程执行 `npm run tauri build` 生成；本记录不包含产物签名与 `latest.json` 的核对结果。

## 历史验证

0.6.5 及更早版本的验证记录见对应版本的更新说明与 [v0.6.5 Release](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.5)。
