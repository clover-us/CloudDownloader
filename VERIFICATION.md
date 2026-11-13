# CloudDownloader 0.6.1 验证记录

验证日期：2026-11-14，Asia/Shanghai。发布版本：[v0.6.1](https://github.com/clover-us/CloudDownloader/releases/tag/v0.6.1)，提供 Windows 安装版、便携版与 SHA-256 校验文件。上一版验证见 [0.6.0](docs/verification-0.6.0.md)，更新内容见 [0.6.1 更新说明](docs/release-0.6.1.md)。

## 本次修改

- 修复网盘登录窗口显示不完整：之前八个网盘共用固定 1000 × 740 的登录窗口，而各家桌面版页面的布局宽度在 1000–1275 像素之间，导致其中七家的右侧内容被窗口边缘裁掉。
- 登录窗口统一注入一段初始化脚本（`src-tauri/src/session.rs` 的 `FIT_TO_WIDTH`，通过 `.initialization_script()` 挂到每个 provider 的窗口）：页面加载后测量 `html.clientWidth` 与实际内容宽度，按 `viewport / needed` 等比缩小整页（最小 0.5 倍），并清空上一次的缩放值后重新测量。
- 缩放在 `DOMContentLoaded`、`load`、窗口尺寸变化时触发，并在加载后 300 ms、1200 ms、3000 ms 复测，覆盖字体与图片晚到造成的布局变化。
- 所有网盘走同一段脚本与同一套判定，不再依赖各站点自己的横向滚动条样式，因此不会再出现「有的有滚动条、有的直接截断」。
- 窗口尺寸、登录流程、Cookie 与会话读取逻辑均未改动；设置中心、自动更新等 0.6.0 功能保持不变。

## 功能验证

修复前（固定 1000 px 窗口，读取页面 Document 的横向视图占比换算出的布局宽度）：

| 网盘 | 横向视图占比 | 实际布局宽度 | 结果 |
| --- | --- | --- | --- |
| 蓝奏云 | 100% | 1000 | 完整显示 |
| 139 云盘 | 97.66% | 1024 | 右侧被裁 |
| 夸克网盘 | 95.06% | 1052 | 右侧被裁 |
| 115 网盘 | 91.32% | 1095 | 右侧被裁 |
| 123 云盘 | 82.30% | 1215 | 右侧被裁 |
| 蓝奏云优享版 | 82.30% | 1215 | 右侧被裁 |
| 百度网盘 | 79.37% | 1260 | 右侧被裁 |
| UC 网盘 | 78.43% | 1275 | 右侧被裁 |

修复后在同一个安装包（`package/CloudDownloader.exe`，0.6.1）上逐个打开八个登录窗口，读取同一个可访问性属性：

- 八个窗口全部为 `HorizontalViewSize = 100%`、`HorizontallyScrollable = False`，`Document` 尺寸仍为 1000 × 740。
- 截图确认夸克与 UC（修复前需求宽度最大的两家）的右侧菜单、表格列与二维码面板完整可见，页面按比例缩小后文字与控件不变形。
- 登录窗口的标题、所属账号 profile 目录、Cookie 读取与保存路径未变；修复只影响渲染缩放。

## 验证范围

- 未使用真实网盘账号完成登录后的网盘浏览与下载回归；本次只验证登录窗口的渲染与尺寸适配。账号与传输能力沿用 0.6.0 / 0.5.7 的验证记录。
- 界面检查基于本机 96 DPI、100% 缩放；未覆盖系统缩放 125% / 150% 与多显示器混合 DPI。
- 缩放上限固定为 0.5 倍、下限为 1 倍（内容比窗口窄时不放大），若之后某个网盘改版后布局宽度超过 2000 像素，会触到 0.5 倍下限并重新出现横向滚动。
- 自动更新的「下载—安装—重启」完整链路仍未在真实旧版本上跑通；签名产物与清单的匹配关系已在本地核对。

## 构建与交付

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 与 `cargo test --lib` 通过：69 passed / 0 failed / 1 ignored（需要真实分享链接的用例被忽略）。
- `npm run build`（`vue-tsc` + Vite 生产构建）通过：1917 modules，CSS 33.66 kB、JS 179.68 kB。
- 带签名的 Windows release 与 NSIS 构建通过，打包脚本核对了主程序版本资源（0.6.1.0）；最终交付的安装包由 makensis 重编译并单独签名，`.sig` 与 `latest.json` 中的签名一致。
- 交付文件：package/CloudDownloader.exe（11.01 MiB）、package/CloudDownloader_0.6.1_x64-setup.exe（5.66 MiB）、package/CloudDownloader_0.6.1_x64-setup.exe.sig、package/CloudDownloader-0.6.1-win-x64.zip（7.38 MiB）、package/SHA256SUMS-0.6.1.txt、package/latest.json。
- 安装包 SHA-256 `d457f2b8a29abd798c130c8f9bef125ee774b92dfec920c716adbf429069859f`，便携 ZIP SHA-256 `6807a0eace064aacb2ada147ef0f659bfe87fb38dc2a33a42a2c3a60ba1faba4`，与 SHA256SUMS-0.6.1.txt 一致。
- 本轮未安装或卸载用户已有的 0.6.0 应用，升级路径留给发布后的自动更新验证。
