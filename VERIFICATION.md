# CloudDownloader 0.6.5 验证记录

验证日期：2026-11-14，Asia/Shanghai。发布版本：[v0.6.5](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.5)，提供 Windows 安装版、便携版与 SHA-256 校验文件。上一版验证见 [0.6.4 更新说明](docs/release-0.6.4.md)，本版更新内容见 [0.6.5 更新说明](docs/release-0.6.5.md)。

## 本次修改

- 选中态批量操作由独立的整行批量栏改为并入文件列表表头行：左侧「全选 / 已选中 N 项 / 取消选中 / 下载 / 分享 / 删除 / 更多」，右侧仍保留「大小」「修改时间」列头。
- 普通态与选中态表头使用同一背景色并固定高度 48px，切换选中不改变表头尺寸，仅有内容差异。
- 文件名称、大小、修改时间三个列头可点击排序：首次点击按默认方向（名称升序，大小 / 修改时间降序），再次点击切换升降序，显示 ⇅ / ↑ / ↓ 指示；排序走后端全库接口 `dir_list_all`。
- 网格视图无表头，选中时顶部保留批量栏。
- 同步调整自动化用例：`D7/D8` 改为断言表头内的批量操作，新增 `D17` 表头点击排序。

## 功能验证

- 自动化界面回归（`harness/test.mjs`，Playwright + 本机 Edge，1180×822）：共 52 项检查全部通过，覆盖账号对话框、网盘切换、文件列表加载、表头内批量操作、行悬浮操作显隐、上传 / 排序 / 筛选菜单、列头点击排序、全库排序筛选、目录级上传进度、深层面包屑折叠等。
- 关键截图：`harness/shots/05b-drive-pan115-selection.png`（选中态表头内的批量操作）、`harness/shots/05h-drive-header-sort.png`（点击「大小」列头后的排序指示）、`harness/shots/live-drive-header.png`（真实窗口：选中态与普通态表头同高同宽同底色）。
- 尺寸实测：在 1180 宽下，普通态与选中态表头均为 884×48，切换选中不跳动。
- 真实账号手动验证：使用迅雷网盘账号打开「我的网盘」，选中列表项后表头内出现批量操作，表头高度与普通态一致；点击列头切换排序正常。

## 验证范围

- 真实账号仅覆盖当前登录的迅雷网盘；其余网盘使用模拟数据验证界面与交互，未做真实账号回归。
- 本版仅前端界面与交互调整，未改动网盘后端接口；重命名与删除仍因后端暂无接口而置灰。

## 构建与交付

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check --all-targets` 通过。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`：69 passed / 0 failed / 1 ignored（需要真实分享链接的用例被忽略）。
- `npm run build`（`vue-tsc --noEmit` + Vite 生产构建）通过。
- 带签名的 Windows release 与 NSIS 构建通过，打包脚本核对了主程序版本资源（0.6.5.0），最终交付的安装包由 makensis 重编译并单独签名，`.sig` 与 `latest.json` 中的签名一致。
- 交付文件见 GitHub Release [v0.6.5](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.5)：`CloudDownloader_0.6.5_x64-setup.exe`、便携包 `CloudDownloader-0.6.5-win-x64.zip`、`SHA256SUMS-0.6.5.txt` 与 `latest.json`。

## 历史验证

0.6.4 及更早版本的验证记录见对应版本的更新说明与 [v0.6.4 Release](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.4)。
