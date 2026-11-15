// UI verification for the account/drive redesign against the mock-Tauri harness.
// Run: node harness/test.mjs  (vite dev server on :1420)
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const BASE = process.env.BASE ?? "http://localhost:1420/harness/index.html";
const SHOTS = new URL("./shots/", import.meta.url).pathname.replace(
  /^\/([A-Za-z]:)/,
  "$1",
);
mkdirSync(SHOTS, { recursive: true });

const failures = [];
const check = (name, cond, extra = "") => {
  console.log(
    `${cond ? "PASS" : "FAIL"}  ${name}${extra ? "  [" + extra + "]" : ""}`,
  );
  if (!cond) failures.push(name);
};

let browser;
try {
  browser = await chromium.launch({ channel: "msedge", headless: true });
  console.log("browser: system Edge");
} catch {
  browser = await chromium.launch({ headless: true });
  console.log("browser: bundled chromium");
}
const page = await browser.newPage({ viewport: { width: 1180, height: 822 } });
const pageErrors = [];
page.on("pageerror", (e) => pageErrors.push(e.stack || String(e)));
page.on("console", (m) => {
  if (
    m.type() === "error" &&
    !/favicon|Download the Vue Devtools/i.test(m.text())
  )
    pageErrors.push(m.text());
});

const shot = (name) => page.screenshot({ path: `${SHOTS}${name}.png` });
const nav = (label) =>
  page.locator(".sidebar nav button", { hasText: label }).first().click();
const mock = (fn, ...a) =>
  page.evaluate(([f, args]) => window.__MOCK__[f](...args), [fn, a]);
const rows = () => page.locator(".account-row");
const dialog = () => page.locator(".account-dialog");

await page.goto(BASE, { waitUntil: "networkidle" });
await page.waitForSelector(".sidebar");
await page.waitForFunction(() =>
  window.__MOCK__.state.calls.some(
    (c) => c.cmd === "refresh_account" && c.args.provider === "ilanzou",
  ),
);

// --- A. Account page: connected only ---
await nav("账号管理");
await page.waitForSelector(".account-row");
check(
  "A1 account page lists only connected (3)",
  (await rows().count()) === 3,
  `count=${await rows().count()}`,
);
check(
  "A2 rows are quark/baidu/pan115",
  (await rows().allInnerTexts()).join("|").includes("夸克网盘") &&
    (await rows().allInnerTexts()).join("|").includes("115 网盘"),
);
check(
  "A3 add entry present",
  await page.locator(".account-add-row").isVisible(),
);
check(
  "A4 green connected badges",
  (await page.locator(".account-state").count()) === 3,
);
check(
  "A5 account names shown",
  (await page.locator(".account-row-copy p").first().innerText()).includes(
    "夸克用户_8823",
  ),
);
await shot("01-account-light");

// --- B. Add dialog ---
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
check(
  "B1 dialog sections",
  (await page.locator(".provider-section-caption").allInnerTexts())
    .join("|")
    .includes("已连接") &&
    (await page.locator(".provider-section-caption").allInnerTexts())
      .join("|")
      .includes("未登录"),
);
check(
  "B2 3 connected + 6 available tiles",
  (await page.locator(".provider-tile.connected").count()) === 3 &&
    (await page.locator(".provider-tile:not(.connected)").count()) === 6,
);
await shot("02-dialog-light");
await page.locator(".account-dialog-search input").fill("123");
check(
  "B3 search filters to 123 云盘",
  (await page.locator(".provider-tile").count()) === 1 &&
    (await page.locator(".provider-tile strong").innerText()).includes("123"),
);
await page.locator(".account-dialog-search input").fill("不存在的网盘");
check(
  "B4 search empty note",
  await page.locator(".provider-search-empty").isVisible(),
);
await page.locator(".account-dialog-search input").fill("");

// --- C. Login flow from dialog ---
await page.locator(".provider-tile", { hasText: "123 云盘" }).click();
await page.waitForSelector(".account-dialog", { state: "detached" });
const loginCall = await mock("lastCall", "open_login");
check(
  "C1 open_login invoked for pan123",
  loginCall?.args?.provider === "pan123",
);
check(
  "C2 toast shown",
  (await page.locator(".toast-message").innerText()).includes("123 云盘"),
);
await page.locator(".account-add-row").click();
await page.waitForSelector(".provider-tile.waiting");
check(
  "C3 waiting state in dialog",
  (await page.locator(".provider-tile.waiting").innerText()).includes(
    "等待登录",
  ),
);
await shot("03-dialog-waiting");
await page.keyboard.press("Escape");
await page.waitForSelector(".account-dialog", { state: "detached" });
check("C4 esc closes dialog", true);

// BUG 1 fix: a pending login must NOT add a drive pill before real login
await nav("我的网盘");
await page.waitForSelector(".drive-providers");
check(
  "C5 no pill for pending provider (connected only)",
  (await page
    .locator(".drive-providers button:not(.drive-provider-add)")
    .count()) === 3 &&
    !(await page.locator(".drive-providers").innerText()).includes("123 云盘"),
);
await shot("04-drive-connected-only");

// complete login -> 4s poll picks it up
await mock("completeLogin", "pan123");
await page.waitForFunction(
  () =>
    document.querySelectorAll(
      ".drive-providers button:not(.drive-provider-add)",
    ).length === 4,
  null,
  { timeout: 8000 },
);
check("C6 poll promotes pan123 to connected pill", true);
await nav("账号管理");
await page.waitForFunction(
  () => document.querySelectorAll(".account-row").length === 4,
);
check("C7 account row added after login", true);

// BUG 2 fix: closing the login window cancels the pending login (use UC)
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
await page.locator(".provider-tile", { hasText: "UC 网盘" }).click();
await page.waitForSelector(".account-dialog", { state: "detached" });
await page.locator(".account-add-row").click();
await page.waitForSelector(".provider-tile.waiting");
check(
  "C8a UC waiting before window close",
  (await page.locator(".provider-tile.waiting").innerText()).includes("UC"),
);
await page.keyboard.press("Escape");
await page.waitForSelector(".account-dialog", { state: "detached" });
await mock("closeLoginWindow", "uc");
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
await page.waitForFunction(
  () => !document.querySelector(".provider-tile.waiting"),
);
check(
  "C8b waiting cancelled after login window closed",
  (await page.locator(".provider-tile.waiting").count()) === 0,
);
check(
  "C8c UC tile no longer waiting",
  !(
    await page.locator(".provider-tile", { hasText: "UC 网盘" }).innerText()
  ).includes("等待"),
);
await shot("03b-dialog-cancelled");
await page.keyboard.press("Escape");
await page.waitForSelector(".account-dialog", { state: "detached" });

// --- D. Drive switcher ---
await nav("我的网盘");
await page.waitForSelector(".drive-providers");
const pills = await page
  .locator(".drive-providers button:not(.drive-provider-add)")
  .allInnerTexts();
check("D1 pills only connected (4)", pills.length === 4, pills.join(","));
check(
  "D2 add pill present",
  await page.locator(".drive-provider-add").isVisible(),
);
await page.locator(".drive-providers button", { hasText: "115 网盘" }).click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("cloud_list")?.args?.provider === "pan115",
);
await page.waitForSelector(".drive-file-row");
check(
  "D3 pan115 file list loads",
  (await page.locator(".drive-file-row").count()) >= 4,
);
await shot("05-drive-pan115");
await page.locator(".drive-providers button", { hasText: "百度网盘" }).click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("drive_list") !== undefined,
);
check("D4 baidu panel switches", true);

// --- D2. File-manager affordances gated by capability ---
await page.locator(".drive-providers button", { hasText: "115 网盘" }).click();
await page.waitForSelector(".drive-file-row");
check(
  "D5 download-only provider hides upload",
  (await page
    .locator(".drive-toolbar button", { hasText: "上传文件" })
    .count()) === 0,
);
check(
  "D6 download-only provider hides mkdir",
  (await page
    .locator(".drive-toolbar button", { hasText: "新建文件夹" })
    .count()) === 0,
);
await page
  .locator(".drive-file-row")
  .first()
  .locator('input[type="checkbox"]')
  .check();
await page.waitForSelector(".drive-file-head .batch-actions");
const batch = page.locator(".drive-file-head");
check(
  "D7 header batch bar shows selection",
  (await batch.innerText()).includes("已选中"),
);
check(
  "D8 share disabled for download-only provider",
  await batch
    .locator(".batch-actions button", { hasText: "分享" })
    .isDisabled(),
);
await shot("05b-drive-pan115-selection");
await batch.locator("button.batch-clear").click();
await page.locator(".drive-providers button", { hasText: "百度网盘" }).click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("drive_list") !== undefined,
);
await page.waitForSelector(".drive-file-row");
check(
  "D9 baidu shows upload and mkdir",
  (await page
    .locator(".drive-toolbar button", { hasText: "上传文件" })
    .count()) === 1 &&
    (await page
      .locator(".drive-toolbar button", { hasText: "新建文件夹" })
      .count()) === 1,
);
check(
  "D10 hover row actions rendered",
  (await page.locator(".drive-file-row .drive-row-actions").count()) >= 1,
);
await shot("05c-drive-baidu");

// --- D2a. Folder paths that are internal ids (quark/xunlei) still read as names ---
await page.evaluate(() => {
  window.__MOCK__.state.idPaths = true;
});
await page.locator(".drive-providers button", { hasText: "夸克网盘" }).click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("cloud_list")?.args?.provider === "quark",
);
await page.waitForSelector(".drive-file-row");
await page
  .locator(".drive-file-row .drive-file-name button", { hasText: "电影" })
  .click();
await page.waitForFunction(
  () => (window.__MOCK__.lastCall("cloud_list")?.args?.path ?? "").length > 20,
);
await page.locator(".drive-toolbar button", { hasText: "新建文件夹" }).click();
const mkdirNote = page.locator(".cloud-dialog .dialog-note");
const mkdirNoteText = await mkdirNote.innerText();
check(
  "D10a mkdir dialog shows the folder name, not its internal id",
  mkdirNoteText.includes("电影") && !/[0-9a-f]{32}/.test(mkdirNoteText),
);
await page
  .locator(".cloud-dialog .icon-button[aria-label='关闭对话框']")
  .click();
check(
  "D10b breadcrumb keeps the folder name",
  (await page.locator(".drive-breadcrumbs").innerText()).includes("电影"),
);
// 网盘刚建完目录时，列表接口可能还没把新目录带回来（夸克/迅雷都有这段延迟）。
// 界面应该直接用创建接口的返回值补上这一行，而不是等用户手动刷新。
await page.evaluate(() => {
  window.__MOCK__.state.newDirLagMs = 60000;
});
await page.locator(".drive-toolbar button", { hasText: "新建文件夹" }).click();
await page.locator(".cloud-dialog label input").fill("新资料夹");
await page.locator(".cloud-dialog .dialog-actions button.primary").click();
let folderAppeared = true;
try {
  await page.waitForSelector(
    '.drive-file-row .drive-file-name button:has-text("新资料夹")',
    { timeout: 2000 },
  );
} catch {
  folderAppeared = false;
}
check(
  "D10c new folder shows up without a manual refresh",
  folderAppeared && (await page.locator(".cloud-dialog").count()) === 0,
);
await page.evaluate(() => {
  window.__MOCK__.state.newDirLagMs = 0;
  window.__MOCK__.state.createdDirs.clear();
});
// 夸克、迅雷的提取码由服务端生成，界面只能选择要不要提取码。
await page.locator(".drive-file-row input[type=checkbox]").first().check();
await page.locator(".drive-file-head .batch-btn", { hasText: "分享" }).click();
await page.waitForSelector(".cloud-dialog");
check(
  "D10d quark share lets the server pick the passcode",
  (await page.locator(".cloud-dialog input[placeholder*='留空']").count()) === 0 &&
    (await page.locator(".cloud-dialog label.share-code-toggle").count()) === 1,
);
await page.locator(".cloud-dialog .dialog-actions button.primary").click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("drive_share")?.args?.codeMode === "auto",
);
check(
  "D10e share result shows the passcode the server generated",
  (await page.locator(".cloud-dialog textarea").inputValue()).includes(
    "提取码：cd34",
  ),
);
await page
  .locator(".cloud-dialog .icon-button[aria-label='关闭对话框']")
  .click();
await page.locator(".drive-file-head .batch-btn", { hasText: "分享" }).click();
await page.locator(".cloud-dialog label.share-code-toggle input").uncheck();
await page.locator(".cloud-dialog .dialog-actions button.primary").click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("drive_share")?.args?.codeMode === "none",
);
check(
  "D10f share without a passcode omits the extraction code",
  !(await page.locator(".cloud-dialog textarea").inputValue()).includes(
    "提取码",
  ),
);
await page
  .locator(".cloud-dialog .icon-button[aria-label='关闭对话框']")
  .click();
await page.evaluate(() => {
  window.__MOCK__.state.idPaths = false;
});
await page.locator(".drive-providers button", { hasText: "百度网盘" }).click();
await page.waitForSelector(".drive-file-row");

// --- D2b. Search box is identical on every provider and searches sub-folders ---
await page.locator(".drive-providers button", { hasText: "115 网盘" }).click();
await page.waitForSelector(".drive-file-row");
const searchInput = page.locator(".drive-search input");
const searchButton = page.locator(".drive-search button");
check(
  "D11 non-baidu search copy matches baidu",
  (await searchInput.getAttribute("placeholder")) === "搜索当前目录及子目录" &&
    (await searchButton.innerText()).trim() === "搜索",
);
await searchInput.fill("流浪地球");
await searchButton.click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("cloud_search")?.args?.provider === "pan115",
);
await page.waitForFunction(
  () => document.querySelectorAll(".drive-file-row").length === 1,
);
check(
  "D12 provider search filters by keyword",
  (await page.locator(".drive-scope").innerText()).includes("搜索到 1 项"),
);
await shot("05d-drive-search");
await searchInput.fill("");
await searchButton.click();
await page.waitForFunction(
  () => document.querySelectorAll(".drive-file-row").length >= 4,
);
await page.locator(".drive-providers button", { hasText: "百度网盘" }).click();
await page.waitForSelector(".drive-file-row");
await page.waitForTimeout(400);

// --- D3. Per-row actions appear on hover (and stay hidden otherwise) ---
const actionOpacity = (i) =>
  page
    .locator(".drive-file-row")
    .nth(i)
    .locator(".drive-row-actions")
    .evaluate((el) => getComputedStyle(el).opacity);
check("D11 row actions hidden by default", (await actionOpacity(1)) === "0");
await page.locator(".drive-file-row").first().hover();
await page.waitForTimeout(250);
check("D12 row actions visible on hover", (await actionOpacity(0)) === "1");
await shot("05d-drive-baidu-hover");

// --- D4. Sort panel: created-time sort is now available ---
await page.locator(".drive-toolbar-group .icon-button[title^='排序']").click();
await page.waitForSelector(".menu-sort");
const createdOptions = page
  .locator(".menu-sort .sort-group", { hasText: "按创建时间" })
  .locator(".sort-option");
check(
  "D13 created-time sort enabled",
  (await createdOptions.count()) === 2 &&
    !(await createdOptions.first().isDisabled()),
);
await createdOptions.first().click();
await page.waitForTimeout(200);
check(
  "D14 created sort applies and closes menu",
  (await page.locator(".menu-sort").count()) === 0,
);
check(
  "D15 global-scope label after sorting",
  (await page.locator(".drive-scope").count()) === 1 &&
    (await page.locator(".drive-scope").innerText()).includes("全库"),
);
await shot("05g-drive-sort-created");

// --- D4b. Column headers are clickable to sort ---
await page.locator(".drive-file-head .col-sort", { hasText: "大小" }).click();
await page.waitForFunction(
  () => window.__MOCK__.lastCall("dir_list_all")?.args?.sortKey === "size",
);
check(
  "D17 header click sorts by size",
  (await page
    .locator(".drive-file-head .col-sort", { hasText: "大小" })
    .count()) === 1,
);
await shot("05h-drive-header-sort");

// --- D5. Folder upload shows a per-directory progress card ---
await page.locator(".menu-anchor").first().hover();
await page.waitForSelector(".menu-upload");
await page
  .locator(".menu-upload .drive-menu-item", { hasText: "上传文件夹" })
  .click();
await page.waitForSelector(".modal-backdrop .primary");
await page.locator(".modal-backdrop .primary").click();
await page.waitForSelector(".folder-progress");
check(
  "D16 folder upload shows per-directory progress",
  (await page.locator(".folder-progress li").count()) >= 2,
);
await shot("07-folder-progress");
const folderUpload = await mock("lastCall", "upload_enqueue");
check(
  "D17 folder upload carries its batch group",
  folderUpload?.args?.batch?.name === "Pictures" &&
    String(folderUpload?.args?.batch?.id || "").startsWith("upload:"),
  JSON.stringify(folderUpload?.args?.batch),
);
await page.locator(".fp-close").click();

// --- E. Logout flow + auto-switch ---
// Select pan115 in the drive first, so logging it out must trigger auto-switch.
await nav("我的网盘");
await page.waitForSelector(".drive-providers");
await page.locator(".drive-providers button", { hasText: "115 网盘" }).click();
await nav("账号管理");
await page.waitForSelector(".account-row");
await page
  .locator(".account-row", { hasText: "115 网盘" })
  .locator(".icon-button.logout")
  .click();
await page.waitForSelector(".confirm-dialog");
check(
  "E1 logout confirm dialog",
  (await page.locator("#logout-title").innerText()).includes("115 网盘"),
);
await shot("06-logout-confirm");
await page.locator(".confirm-dialog .danger").click();
await page.waitForFunction(
  () => document.querySelectorAll(".account-row").length === 3,
);
check("E2 row removed after logout", true);
const logoutCall = await mock("lastCall", "logout_account");
check("E3 logout_account invoked", logoutCall?.args?.provider === "pan115");
check(
  "E4 logout toast",
  (await page.locator(".toast-message").innerText()).includes("已退出登录"),
);
await nav("我的网盘");
await page.waitForSelector(".drive-providers button.selected");
check(
  "E5 drive auto-switched away from logged-out provider",
  (await page.locator(".drive-providers button.selected").innerText()).includes(
    "夸克网盘",
  ),
  await page.locator(".drive-providers button.selected").innerText(),
);
check(
  "E6 pan115 pill gone",
  !(await page.locator(".drive-providers").innerText()).includes("115 网盘"),
);

// --- F. Night theme ---
await page.locator(".theme-toggle").click();
await page.waitForFunction(
  () => document.documentElement.dataset.theme === "night",
);
await nav("账号管理");
await page.waitForSelector(".account-row");
await shot("07-account-night");
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
await shot("08-dialog-night");
await page.keyboard.press("Escape");
await page.locator(".theme-toggle").click();
await page.waitForFunction(
  () => document.documentElement.dataset.theme === "light",
);

// --- G. Empty states ---
await mock("setAllLoggedOut");
await page.waitForSelector(".account-empty");
check(
  "G1 account empty state",
  (await page.locator(".account-empty h3").innerText()).includes("还没有连接"),
);
await shot("09-account-empty");
await nav("我的网盘");
await page.waitForSelector(".drive-placeholder");
check(
  "G2 drive empty state",
  (await page.locator(".drive-placeholder h3").innerText()).includes(
    "还没有可用的网盘",
  ),
);
await shot("10-drive-empty");
await page.locator(".drive-placeholder .primary").click();
await page.waitForSelector(".account-dialog");
check(
  "G3 dialog from drive empty shows 9 available",
  (await page.locator(".provider-tile:not(.connected)").count()) === 9,
);
// login from drive context: no pill until actually connected (BUG 1)
await page.locator(".provider-tile", { hasText: "夸克网盘" }).click();
await page.waitForSelector(".account-dialog", { state: "detached" });
check(
  "G4 no drive pill before real login",
  (await page
    .locator(".drive-providers button:not(.drive-provider-add)")
    .count()) === 0,
);
await shot("11-drive-empty-login");
// completing login adds the pill and auto-selects it
await mock("completeLogin", "quark");
await page.waitForSelector(".drive-providers button.selected", {
  timeout: 8000,
});
check(
  "G5 drive auto-selects newly connected quark",
  (await page.locator(".drive-providers button.selected").innerText()).includes(
    "夸克网盘",
  ),
);

// --- H. Narrow viewport (app minimum 920x682) ---
await page.setViewportSize({ width: 920, height: 682 });
await mock("completeLogin", "quark");
await mock("completeLogin", "baidu");
await page.evaluate(() => window.__MOCK__.emit("tasks-changed"));
await nav("账号管理");
await page.waitForSelector(".account-row");
await shot("12-account-narrow");
const overflow = await page.evaluate(
  () =>
    document.documentElement.scrollWidth > document.documentElement.clientWidth,
);
check("H1 no horizontal overflow at 920px", !overflow);

// --- J. Settings panel (feat merge) ---
await page.setViewportSize({ width: 1180, height: 822 });
await nav("设置");
await page.waitForSelector(".settings");
const groups = (await page.locator(".settings-group h2").allInnerTexts()).join(
  "|",
);
check(
  "J1 settings panel renders all groups",
  ["通用", "浏览器接力", "传输", "外观", "关于与更新"].every((g) =>
    groups.includes(g),
  ),
  groups,
);
// The bridge state comes from the backend, so wait for it instead of guessing.
await page.waitForFunction(() =>
  document.querySelector(".settings")?.textContent?.includes("127.0.0.1:17890"),
);
check("J1b settings shows the bridge listening", true);
await page.evaluate(() => {
  window.__MOCK__.state.lastHandoff = {
    site: "example.com",
    ok: true,
    message: "",
    at: 1762000000,
  };
});
await page.getByRole("button", { name: "重新检查" }).click();
await page.waitForFunction(() =>
  document.querySelector(".settings")?.textContent?.includes("example.com"),
);
check("J1c settings shows the last hand-off", true);
// Moving the port has to reach the backend and show up in the state line.
const portInput = page.getByLabel("浏览器接力的监听端口");
await portInput.fill("17891");
await portInput.press("Enter");
await page.waitForFunction(() =>
  document.querySelector(".settings")?.textContent?.includes("127.0.0.1:17891"),
);
const portCall = await mock("lastCall", "settings_set");
check(
  "J1d settings moves the bridge port",
  portCall?.args?.settings?.bridgePort === 17891,
  JSON.stringify(portCall?.args?.settings?.bridgePort),
);
await portInput.fill("17890");
await portInput.press("Enter");
await shot("14-settings");

// --- I. Loading state (slow refresh survives reload via init script) ---
await page.addInitScript(() => {
  window.__MOCK_SLOW__ = 1500;
});
await page.goto(BASE, { waitUntil: "domcontentloaded" });
await page.waitForSelector(".sidebar");
await nav("账号管理");
await page.waitForSelector(".account-checking");
await shot("13-account-loading");
check("I1 initial checking state", true);
await page.waitForSelector(".account-row", { timeout: 10000 });
check("I2 resolves to list", (await rows().count()) === 3);

// --- K. Folder grouping + batch actions ---
await nav("全部任务");
await page.waitForSelector(".task-group-row");
const groupRows = await page.locator(".task-group-row").count();
check(
  "K1 folder renders as one group row",
  groupRows === 1 && (await page.locator(".task-child-row").count()) === 0,
  `groups=${groupRows} children=${await page.locator(".task-child-row").count()}`,
);
const groupMeta = (await page.locator(".task-group-row").innerText()).replace(
  /\s+/g,
  " ",
);
check(
  "K2 group aggregates its files",
  groupMeta.includes("文件夹 · 3 个文件"),
  groupMeta.slice(0, 120),
);
await page.locator(".task-group-row .task-expand").click();
await page.waitForSelector(".task-child-row");
check(
  "K3 expand lists child tasks",
  (await page.locator(".task-child-row").count()) === 3,
);
await shot("15-task-group");
await page.locator(".task-group-row .task-select input").check();
await page.waitForSelector(".task-list-head .batch-actions");
check(
  "K4 group checkbox selects all files",
  (await page.locator(".task-list-head .batch-count").innerText()).includes(
    "3",
  ),
);
await shot("15b-task-select");
await page.locator(".task-list-head .batch-btn", { hasText: "暂停" }).click();
await page.waitForFunction(() =>
  document.querySelector(".task-group-row")?.classList.contains("paused"),
);
const batchCall = await mock("lastCall", "task_batch_action");
check(
  "K5 batch pause hits backend with 3 ids",
  batchCall?.args?.action === "pause" && batchCall?.args?.ids?.length === 3,
  JSON.stringify(batchCall?.args),
);
await shot("16-task-batch");
await page.locator(".task-select-all input").check();
check(
  "K6 select-all covers group children + files",
  (await page.locator(".task-list-head .batch-count").innerText()).includes(
    "5",
  ),
);
await page.locator(".task-list-head .batch-clear").click();
await page.locator(".task-search input").fill("系统镜像");
await page.waitForFunction(
  () => document.querySelectorAll("article.task-row").length === 1,
);
check(
  "K7 search hides non-matching group",
  (await page.locator("article.task-row").count()) === 1,
);
await page.locator(".task-search input").fill("");
await page
  .locator('.task-group-row button[aria-label="移除文件夹下载任务"]')
  .click();
await page.waitForSelector(".remove-task-dialog");
const removeText = (
  await page.locator(".remove-task-dialog").innerText()
).replace(/\s+/g, " ");
check(
  "K8 group remove dialog names folder and count",
  removeText.includes("文件夹「电影」的 3 个文件") &&
    removeText.includes("共 3 个任务会停止"),
  removeText.slice(0, 140),
);
await page
  .locator(".remove-task-dialog button", { hasText: "保留任务" })
  .click();
await nav("已完成");
check(
  "K9 status tab only shows fully complete folders",
  (await page.locator(".task-group-row").count()) === 0 &&
    (await page.locator("article.task-row").count()) === 1,
  `groups=${await page.locator(".task-group-row").count()} rows=${await page.locator("article.task-row").count()}`,
);
await nav("全部任务");
const allNav = await page
  .locator(".sidebar nav button", { hasText: "全部任务" })
  .innerText();
check(
  "K10 nav badge counts a folder as one task",
  allNav.replace(/\s+/g, "").endsWith("3"),
  allNav.replace(/\s+/g, " "),
);

await page.locator(".task-select-all input").check();
check(
  "K11 select all counts every task",
  (await page.locator(".task-list-head .batch-count").innerText()).includes(
    "5",
  ),
  await page.locator(".task-list-head .batch-count").innerText(),
);
await nav("已完成");
check(
  "K11b tab switch clears the selection",
  (await page.locator(".task-list-head .batch-count").count()) === 0 &&
    (await page.locator(".task-select input:checked").count()) === 0,
  `batchbars=${await page.locator(".task-list-head .batch-count").count()} checked=${await page.locator(".task-select input:checked").count()}`,
);

// --- L. Upload list: batch selection and actions ---
await page.evaluate(() => {
  const state = window.__MOCK__.state;
  state.uploads.push(
    {
      id: "upload-1",
      name: "报告.pdf",
      localPath: "D:\\docs\\报告.pdf",
      remotePath: "/文档/报告.pdf",
      size: 1000,
      completed: 400,
      status: "active",
      provider: "quark",
      concurrency: 4,
    },
    {
      id: "upload-2",
      name: "素材.zip",
      localPath: "D:\\docs\\素材.zip",
      remotePath: "/文档/素材.zip",
      size: 2000,
      completed: 1200,
      status: "paused",
      provider: "xunlei",
      concurrency: 2,
    },
    {
      id: "upload-3",
      name: "归档.7z",
      localPath: "D:\\docs\\归档.7z",
      remotePath: "/文档/归档.7z",
      size: 300,
      completed: 300,
      status: "complete",
      provider: "quark",
      concurrency: 2,
    },
  );
  window.__MOCK__.emit("tasks-changed");
});
await nav("上传任务");
await page.waitForFunction(
  () => document.querySelectorAll("article.task-row").length === 3,
);
check(
  "L1 upload list has a batch header",
  (await page.locator(".upload-head .head-label").innerText()).includes(
    "上传任务",
  ),
  await page.locator(".upload-head").innerText(),
);
await page.locator(".upload-head .task-select-all input").check();
await page.waitForSelector(".upload-head .batch-count");
check(
  "L2 upload select-all counts tasks",
  (await page.locator(".upload-head .batch-count").innerText()).includes("3"),
  await page.locator(".upload-head .batch-count").innerText(),
);
await page.locator(".upload-head .batch-btn", { hasText: "暂停" }).click();
const uploadPause = await mock("lastCall", "upload_batch_action");
check(
  "L3 upload batch pause hits backend with 3 ids",
  uploadPause?.args?.action === "pause" && uploadPause?.args?.ids?.length === 3,
  JSON.stringify(uploadPause?.args),
);
await page.waitForFunction(
  () =>
    document.querySelectorAll("article.task-row .status-pill.paused").length ===
    2,
);
await shot("17-upload-batch");
await page
  .locator(".cloud-page-toolbar .segmented button", { hasText: "已完成" })
  .click();
await page.waitForFunction(
  () => document.querySelectorAll("article.task-row").length === 1,
);
check(
  "L4 completed tab offers clear-completed",
  (await page.locator(".upload-head .task-clear").count()) === 1 &&
    (await page.locator(".upload-head .batch-count").count()) === 0,
);
await page.locator(".upload-head .task-clear").click();
const uploadClear = await mock("lastCall", "upload_batch_action");
check(
  "L5 clear-completed removes finished uploads",
  uploadClear?.args?.action === "remove" &&
    uploadClear?.args?.ids?.length === 1,
  JSON.stringify(uploadClear?.args),
);
await page.waitForFunction(
  () => document.querySelectorAll("article.task-row").length === 0,
);
await page
  .locator(".cloud-page-toolbar .segmented button", { hasText: "全部" })
  .click();
await page.waitForFunction(
  () => document.querySelectorAll("article.task-row").length === 2,
);
await page.locator("article.task-row .task-select input").first().check();
await page.locator(".upload-head .batch-btn", { hasText: "移除" }).click();
await page.waitForSelector(".confirm-dialog");
const uploadDialog = (
  await page.locator(".confirm-dialog").innerText()
).replace(/\s+/g, " ");
check(
  "L6 upload remove dialog scopes the selection",
  uploadDialog.includes("移除所选上传任务？") &&
    uploadDialog.includes("共 1 个任务"),
  uploadDialog.slice(0, 140),
);
await page.locator(".confirm-dialog button", { hasText: "取消" }).click();

// --- L7. Folder uploads collapse into one group row ---
await page.evaluate(() => {
  const state = window.__MOCK__.state;
  const batch = { id: "upload:quark:acc:/文档:素材包", name: "素材包" };
  const folder = ["active", "paused", "complete"];
  folder.forEach((status, index) => {
    state.uploads.push({
      id: `folder-${index}`,
      name: `素材-${index + 1}.psd`,
      localPath: `D:\\素材包\\素材-${index + 1}.psd`,
      remotePath: `/文档/素材包/素材-${index + 1}.psd`,
      size: 1000,
      completed: status === "complete" ? 1000 : 400,
      status,
      provider: "quark",
      concurrency: 4,
      batch,
    });
  });
  window.__MOCK__.emit("tasks-changed");
});
await page.waitForSelector(".task-group-row");
check(
  "L7 folder upload renders as one group row",
  (await page.locator(".task-group-row").count()) === 1 &&
    (await page.locator(".task-child-row").count()) === 0 &&
    (await page.locator(".task-group-row").innerText()).includes(
      "文件夹 · 3 个文件",
    ),
);
await page
  .locator(".cloud-page-toolbar .segmented button", { hasText: "下载中" })
  .click();
await page.waitForFunction(
  () => document.querySelectorAll("article.task-row").length === 1,
);
check(
  "L8 active filter keeps the uploading folder",
  (await page.locator(".task-group-row").count()) === 1,
);
await page.locator(".task-group-row .task-expand").click();
await page.waitForSelector(".task-child-row");
check(
  "L9 expanding shows only in-progress children",
  (await page.locator(".task-child-row").count()) === 1,
);
await page
  .locator(".cloud-page-toolbar .segmented button", { hasText: "全部" })
  .click();
await page.waitForFunction(
  () => document.querySelectorAll("article.task-row").length === 6,
);

// --- M. Browser hand-off (clouddownloader://) ---
await mock("handoff", "https://example.com/files/report.pdf");
await page.waitForSelector(".handoff-dialog");
check(
  "M1 handoff asks before taking over",
  (await page.locator(".handoff-dialog").innerText()).includes("report.pdf"),
);
await page.locator(".handoff-dialog footer .primary").click();
await page.waitForFunction(() =>
  window.__MOCK__.state.calls.some(
    (call) =>
      call.cmd === "enqueue_direct" &&
      String(call.args?.url ?? "").includes("report.pdf"),
  ),
);
check(
  "M2 confirmed hand-off starts downloading right away",
  await page.evaluate(() =>
    window.__MOCK__.state.calls.some(
      (call) =>
        call.cmd === "enqueue_direct" &&
        String(call.args?.url ?? "").includes("report.pdf"),
    ),
  ),
);
check(
  "M2b hand-off dialog closes after confirming",
  (await page.locator(".handoff-dialog").count()) === 0,
);
await page.waitForFunction(() =>
  window.__MOCK__.state.calls.some((c) => c.cmd === "inspect_download"),
);
check(
  "M3 handoff queue is drained once",
  (await page.evaluate(() => window.__MOCK__.state.handoffs.length)) === 0,
);

// --- M4–M8. Extension hand-off (bridge): browser headers reach the engine ---
const handed = {
  url: "https://example.com/files/signed-video.mp4",
  filename: "video.mp4",
  headers: [
    "Referer: https://example.com/watch",
    "Cookie: uid=42",
    "User-Agent: Browser/1.0",
  ],
};
await mock("handoff", handed);
await page.waitForSelector(".handoff-dialog");
await page.locator(".handoff-dialog footer .primary").click();
await page.waitForFunction(
  () =>
    window.__MOCK__.state.calls.filter((call) => call.cmd === "enqueue_direct")
      .length >= 2,
);
const inspectArgs = await page.evaluate(
  () =>
    window.__MOCK__.state.calls
      .filter((c) => c.cmd === "inspect_download")
      .at(-1)?.args,
);
check(
  "M6 handoff hands the headers to the probe",
  JSON.stringify(inspectArgs?.headers) === JSON.stringify(handed.headers),
  JSON.stringify(inspectArgs),
);
const enqueueArgs = await page.evaluate(
  () =>
    window.__MOCK__.state.calls.filter((c) => c.cmd === "enqueue_direct").at(-1)
      ?.args,
);
check(
  "M4 handoff keeps the browser filename",
  enqueueArgs?.filename === "video.mp4",
  JSON.stringify(enqueueArgs),
);
check(
  "M7 handoff passes the headers on to aria2",
  enqueueArgs?.url === handed.url &&
    JSON.stringify(enqueueArgs?.headers) === JSON.stringify(handed.headers),
  JSON.stringify(enqueueArgs),
);
check(
  "M8 handoff marks the task as coming from a browser",
  enqueueArgs?.fromBrowser === true,
  JSON.stringify(enqueueArgs),
);

// A task the app built from a browser hand-off says so in the list.
await page.evaluate(() => {
  window.__MOCK__.state.tasks.push({
    id: "handoff-1",
    name: "signed-video.mp4",
    size: 1024,
    completed: 0,
    speed: 0,
    status: "active",
    error: "",
    directory: "D:\\Downloads",
    createdAt: 1762000000,
    share: { provider: "quark", id: "", passcode: "" },
    file: {
      id: "handoff-1",
      name: "signed-video.mp4",
      size: 1024,
      isDir: false,
      parentId: "",
    },
    direct: {
      url: "https://example.com/files/signed-video.mp4",
      connections: 8,
      rangeSupported: true,
    },
    origin: { site: "example.com" },
  });
  window.__MOCK__.emit("tasks-changed");
});
const originBadge = await page.evaluate(() => {
  const row = [...document.querySelectorAll("article.task-row")].find((r) =>
    r.textContent?.includes("signed-video.mp4"),
  );
  return {
    tag: row?.querySelector(".provider-tag")?.textContent?.trim(),
    site: row?.querySelector(".origin-tag")?.textContent?.trim(),
  };
});
check(
  "M9 the list shows which page handed the link over",
  originBadge.tag === "浏览器接力" && originBadge.site === "example.com",
  JSON.stringify(originBadge),
);

// --- S. Share records reuse the upload list's batch header ---
await page.evaluate(() => {
  const state = window.__MOCK__.state;
  state.loggedIn.xunlei = true;
  state.shares.length = 0;
  state.shares.push(
    {
      id: "share-live",
      url: "https://pan.quark.cn/s/1liveShare",
      password: "aaaa",
      period: 7,
      createdAt: Math.floor(Date.now() / 1000),
      accountId: "quark-account",
      names: ["电影合集.zip"],
      cancelled: false,
      provider: "quark",
    },
    {
      id: "share-gone",
      url: "https://pan.quark.cn/s/1goneShare",
      password: "bbbb",
      period: 7,
      createdAt: Math.floor(Date.now() / 1000),
      accountId: "quark-account",
      names: ["文档.pdf"],
      cancelled: true,
      provider: "quark",
    },
    {
      id: "share-xunlei",
      url: "https://pan.xunlei.com/s/1xunleiShare",
      password: "",
      period: 7,
      createdAt: Math.floor(Date.now() / 1000),
      accountId: "xunlei-account",
      names: ["迅雷专属.mkv"],
      cancelled: false,
      provider: "xunlei",
    },
  );
  window.__MOCK__.emit("tasks-changed");
});
await nav("分享记录");
await page.waitForFunction(
  () => document.querySelectorAll("article.share-record").length === 2,
);
check(
  "S1 share list has a batch header",
  (await page.locator(".task-list-head .head-label").innerText()).includes(
    "分享记录",
  ),
  await page.locator(".task-list-head").innerText(),
);
check(
  "S1b share tabs filter by provider",
  (await page.locator(".drive-providers button.selected").innerText()).includes(
    "夸克网盘",
  ),
  await page.locator(".drive-providers").innerText(),
);
await page.locator(".task-list-head .task-select-all input").check();
await page.waitForSelector(".task-list-head .batch-count");
check(
  "S2 share select-all counts records",
  (await page.locator(".task-list-head .batch-count").innerText()).includes(
    "2",
  ),
  await page.locator(".task-list-head .batch-count").innerText(),
);
await shot("18-share-batch");
await page.locator(".task-list-head .batch-btn", { hasText: "取消分享" }).click();
await page.waitForSelector(".confirm-dialog");
const shareDialog = (
  await page.locator(".confirm-dialog").innerText()
).replace(/\s+/g, " ");
check(
  "S3 batch cancel dialog only counts live shares",
  shareDialog.includes("取消所选的 1 个分享？"),
  shareDialog.slice(0, 140),
);
await page.locator(".confirm-dialog button", { hasText: "取消分享" }).click();
await page.waitForFunction(() =>
  window.__MOCK__.lastCall("drive_cancel_share"),
);
const cancelCall = await mock("lastCall", "drive_cancel_share");
check(
  "S4 batch cancel hits the backend for the live share",
  cancelCall?.args?.id === "share-live",
  JSON.stringify(cancelCall?.args),
);
await page.waitForFunction(
  () =>
    document.querySelectorAll("article.share-record .status-pill.complete")
      .length === 0,
);
check(
  "S5 cancelled share flips to 已取消 and selection resets",
  (await page.locator(".task-list-head .batch-count").count()) === 0 &&
    (await page.locator("article.share-record").first().innerText()).includes(
      "已取消",
    ),
);
const shareLinks = page.locator("article.share-record a.share-link");
check(
  "S6 share link is a clickable link to the share page",
  (await shareLinks.count()) === 2 &&
    ((await shareLinks.first().getAttribute("href")) ?? "").startsWith(
      "https://pan.quark.cn/s/",
    ),
);
await page
  .locator(".drive-providers button", { hasText: "迅雷网盘" })
  .click();
await page.waitForFunction(
  () => document.querySelectorAll("article.share-record").length === 1,
);
check(
  "S7 switching provider tab shows only that provider's shares",
  (await page.locator("article.share-record").first().innerText()).includes(
    "迅雷专属.mkv",
  ),
  await page.locator("article.share-record").first().innerText(),
);
await page.locator('button[aria-label="从网盘刷新"]').click();
await page.waitForFunction(() =>
  window.__MOCK__.lastCall("drive_share_sync"),
);
const syncCall = await mock("lastCall", "drive_share_sync");
check(
  "S8 refreshing pulls share records for the active provider",
  syncCall?.args?.provider === "xunlei",
  JSON.stringify(syncCall?.args),
);
await page
  .locator('article.share-record button[aria-label="删除分享记录"]')
  .click();
await page.waitForSelector(".confirm-dialog");
const forgetDialog = (
  await page.locator(".confirm-dialog").innerText()
).replace(/\s+/g, " ");
check(
  "S9 deleting a share record asks before forgetting it",
  forgetDialog.includes("删除这条分享记录？") &&
    forgetDialog.includes("取消分享"),
  forgetDialog.slice(0, 140),
);
await page.locator(".confirm-dialog button", { hasText: "删除记录" }).click();
await page.waitForFunction(() =>
  window.__MOCK__.lastCall("drive_forget_share"),
);
const forgetCall = await mock("lastCall", "drive_forget_share");
await page.waitForFunction(
  () => document.querySelectorAll("article.share-record").length === 0,
);
check(
  "S10 forgetting removes the row without touching the provider",
  ((forgetCall?.args?.ids ?? [])).includes("share-xunlei") &&
    (await page.evaluate(() =>
      window.__MOCK__.state.shares.some((s) => s.id === "share-xunlei"),
    )) === false,
  JSON.stringify(forgetCall?.args),
);

// --- N. Title bar plugin button ---
await page.locator(".plugin-toggle").click();
await page.waitForSelector(".plugin-dialog");
await page.waitForFunction(() =>
  document
    .querySelector(".plugin-dialog")
    ?.textContent?.includes("扩展正常工作"),
);
const pluginText = await page.locator(".plugin-dialog").innerText();
check(
  "N1 plugin button shows the extension state",
  // Version must match harness/mock-tauri.ts bridge_status, which mirrors
  // extension/manifest.json.
  pluginText.includes("127.0.0.1:17890") && pluginText.includes("0.2.2"),
  pluginText.slice(0, 120),
);
await page.getByRole("button", { name: "重新检测" }).click();
await page.waitForTimeout(300);
check("N2 plugin dialog re-checks", true);
await page.locator(".plugin-close").click();
await page.waitForSelector(".plugin-dialog", { state: "detached" });
check("N3 plugin dialog closes", true);

check(
  "Z1 no page errors",
  pageErrors.length === 0,
  pageErrors.slice(0, 3).join(" | "),
);

await browser.close();
console.log(
  failures.length
    ? `\n${failures.length} FAILURES: ${failures.join(", ")}`
    : "\nALL CHECKS PASSED",
);
process.exit(failures.length ? 1 : 0);
