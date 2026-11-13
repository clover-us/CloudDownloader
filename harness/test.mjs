// UI verification for the account/drive redesign against the mock-Tauri harness.
// Run: node harness/test.mjs  (vite dev server on :1420)
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const BASE = "http://localhost:1420/harness/index.html";
const SHOTS = new URL("./shots/", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
mkdirSync(SHOTS, { recursive: true });

const failures = [];
const check = (name, cond, extra = "") => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? "  [" + extra + "]" : ""}`);
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
page.on("pageerror", (e) => pageErrors.push(String(e)));
page.on("console", (m) => {
  if (m.type() === "error" && !/favicon|Download the Vue Devtools/i.test(m.text()))
    pageErrors.push(m.text());
});

const shot = (name) => page.screenshot({ path: `${SHOTS}${name}.png` });
const nav = (label) => page.locator(".sidebar nav button", { hasText: label }).first().click();
const mock = (fn, ...a) => page.evaluate(([f, args]) => window.__MOCK__[f](...args), [fn, a]);
const rows = () => page.locator(".account-row");
const dialog = () => page.locator(".account-dialog");

await page.goto(BASE, { waitUntil: "networkidle" });
await page.waitForSelector(".sidebar");
await page.waitForFunction(() => window.__MOCK__.state.calls.some((c) => c.cmd === "refresh_account" && c.args.provider === "ilanzou"));

// --- A. Account page: connected only ---
await nav("账号管理");
await page.waitForSelector(".account-row");
check("A1 account page lists only connected (3)", (await rows().count()) === 3, `count=${await rows().count()}`);
check("A2 rows are quark/baidu/pan115", (await rows().allInnerTexts()).join("|").includes("夸克网盘") && (await rows().allInnerTexts()).join("|").includes("115 网盘"));
check("A3 add entry present", await page.locator(".account-add-row").isVisible());
check("A4 green connected badges", (await page.locator(".account-state").count()) === 3);
check("A5 account names shown", (await page.locator(".account-row-copy p").first().innerText()).includes("夸克用户_8823"));
await shot("01-account-light");

// --- B. Add dialog ---
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
check("B1 dialog sections", (await page.locator(".provider-section-caption").allInnerTexts()).join("|").includes("已连接") && (await page.locator(".provider-section-caption").allInnerTexts()).join("|").includes("未登录"));
check("B2 3 connected + 5 available tiles", (await page.locator(".provider-tile.connected").count()) === 3 && (await page.locator(".provider-tile:not(.connected)").count()) === 5);
await shot("02-dialog-light");
await page.locator(".account-dialog-search input").fill("123");
check("B3 search filters to 123 云盘", (await page.locator(".provider-tile").count()) === 1 && (await page.locator(".provider-tile strong").innerText()).includes("123"));
await page.locator(".account-dialog-search input").fill("不存在的网盘");
check("B4 search empty note", await page.locator(".provider-search-empty").isVisible());
await page.locator(".account-dialog-search input").fill("");

// --- C. Login flow from dialog ---
await page.locator(".provider-tile", { hasText: "123 云盘" }).click();
await page.waitForSelector(".account-dialog", { state: "detached" });
const loginCall = await mock("lastCall", "open_login");
check("C1 open_login invoked for pan123", loginCall?.args?.provider === "pan123");
check("C2 toast shown", (await page.locator(".toast-message").innerText()).includes("123 云盘"));
await page.locator(".account-add-row").click();
await page.waitForSelector(".provider-tile.waiting");
check("C3 waiting state in dialog", (await page.locator(".provider-tile.waiting").innerText()).includes("等待登录"));
await shot("03-dialog-waiting");
await page.keyboard.press("Escape");
await page.waitForSelector(".account-dialog", { state: "detached" });
check("C4 esc closes dialog", true);

// BUG 1 fix: a pending login must NOT add a drive pill before real login
await nav("我的网盘");
await page.waitForSelector(".drive-providers");
check("C5 no pill for pending provider (connected only)", (await page.locator(".drive-providers button:not(.drive-provider-add)").count()) === 3 && !(await page.locator(".drive-providers").innerText()).includes("123 云盘"));
await shot("04-drive-connected-only");

// complete login -> 4s poll picks it up
await mock("completeLogin", "pan123");
await page.waitForFunction(() => document.querySelectorAll(".drive-providers button:not(.drive-provider-add)").length === 4, null, { timeout: 8000 });
check("C6 poll promotes pan123 to connected pill", true);
await nav("账号管理");
await page.waitForFunction(() => document.querySelectorAll(".account-row").length === 4);
check("C7 account row added after login", true);

// BUG 2 fix: closing the login window cancels the pending login (use UC)
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
await page.locator(".provider-tile", { hasText: "UC 网盘" }).click();
await page.waitForSelector(".account-dialog", { state: "detached" });
await page.locator(".account-add-row").click();
await page.waitForSelector(".provider-tile.waiting");
check("C8a UC waiting before window close", (await page.locator(".provider-tile.waiting").innerText()).includes("UC"));
await page.keyboard.press("Escape");
await page.waitForSelector(".account-dialog", { state: "detached" });
await mock("closeLoginWindow", "uc");
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
await page.waitForFunction(() => !document.querySelector(".provider-tile.waiting"));
check("C8b waiting cancelled after login window closed", (await page.locator(".provider-tile.waiting").count()) === 0);
check("C8c UC tile no longer waiting", !(await page.locator(".provider-tile", { hasText: "UC 网盘" }).innerText()).includes("等待"));
await shot("03b-dialog-cancelled");
await page.keyboard.press("Escape");
await page.waitForSelector(".account-dialog", { state: "detached" });

// --- D. Drive switcher ---
await nav("我的网盘");
await page.waitForSelector(".drive-providers");
const pills = await page.locator(".drive-providers button:not(.drive-provider-add)").allInnerTexts();
check("D1 pills only connected (4)", pills.length === 4, pills.join(","));
check("D2 add pill present", await page.locator(".drive-provider-add").isVisible());
await page.locator(".drive-providers button", { hasText: "115 网盘" }).click();
await page.waitForFunction(() => window.__MOCK__.lastCall("cloud_list")?.args?.provider === "pan115");
await page.waitForSelector(".drive-file-row");
check("D3 pan115 file list loads", (await page.locator(".drive-file-row").count()) >= 4);
await shot("05-drive-pan115");
await page.locator(".drive-providers button", { hasText: "百度网盘" }).click();
await page.waitForFunction(() => window.__MOCK__.lastCall("drive_list") !== undefined);
check("D4 baidu panel switches", true);

// --- E. Logout flow + auto-switch ---
// Select pan115 in the drive first, so logging it out must trigger auto-switch.
await nav("我的网盘");
await page.waitForSelector(".drive-providers");
await page.locator(".drive-providers button", { hasText: "115 网盘" }).click();
await nav("账号管理");
await page.waitForSelector(".account-row");
await page.locator(".account-row", { hasText: "115 网盘" }).locator(".icon-button.logout").click();
await page.waitForSelector(".confirm-dialog");
check("E1 logout confirm dialog", (await page.locator("#logout-title").innerText()).includes("115 网盘"));
await shot("06-logout-confirm");
await page.locator(".confirm-dialog .danger").click();
await page.waitForFunction(() => document.querySelectorAll(".account-row").length === 3);
check("E2 row removed after logout", true);
const logoutCall = await mock("lastCall", "logout_account");
check("E3 logout_account invoked", logoutCall?.args?.provider === "pan115");
check("E4 logout toast", (await page.locator(".toast-message").innerText()).includes("已退出登录"));
await nav("我的网盘");
await page.waitForSelector(".drive-providers button.selected");
check("E5 drive auto-switched away from logged-out provider", (await page.locator(".drive-providers button.selected").innerText()).includes("夸克网盘"), await page.locator(".drive-providers button.selected").innerText());
check("E6 pan115 pill gone", !(await page.locator(".drive-providers").innerText()).includes("115 网盘"));

// --- F. Night theme ---
await page.locator(".theme-toggle").click();
await page.waitForFunction(() => document.documentElement.dataset.theme === "night");
await nav("账号管理");
await page.waitForSelector(".account-row");
await shot("07-account-night");
await page.locator(".account-add-row").click();
await page.waitForSelector(".account-dialog");
await shot("08-dialog-night");
await page.keyboard.press("Escape");
await page.locator(".theme-toggle").click();
await page.waitForFunction(() => document.documentElement.dataset.theme === "light");

// --- G. Empty states ---
await mock("setAllLoggedOut");
await page.waitForSelector(".account-empty");
check("G1 account empty state", (await page.locator(".account-empty h3").innerText()).includes("还没有连接"));
await shot("09-account-empty");
await nav("我的网盘");
await page.waitForSelector(".drive-placeholder");
check("G2 drive empty state", (await page.locator(".drive-placeholder h3").innerText()).includes("还没有可用的网盘"));
await shot("10-drive-empty");
await page.locator(".drive-placeholder .primary").click();
await page.waitForSelector(".account-dialog");
check("G3 dialog from drive empty shows 8 available", (await page.locator(".provider-tile:not(.connected)").count()) === 8);
// login from drive context: no pill until actually connected (BUG 1)
await page.locator(".provider-tile", { hasText: "夸克网盘" }).click();
await page.waitForSelector(".account-dialog", { state: "detached" });
check("G4 no drive pill before real login", (await page.locator(".drive-providers button:not(.drive-provider-add)").count()) === 0);
await shot("11-drive-empty-login");
// completing login adds the pill and auto-selects it
await mock("completeLogin", "quark");
await page.waitForSelector(".drive-providers button.selected", { timeout: 8000 });
check("G5 drive auto-selects newly connected quark", (await page.locator(".drive-providers button.selected").innerText()).includes("夸克网盘"));

// --- H. Narrow viewport (app minimum 920x682) ---
await page.setViewportSize({ width: 920, height: 682 });
await mock("completeLogin", "quark");
await mock("completeLogin", "baidu");
await page.evaluate(() => window.__MOCK__.emit("tasks-changed"));
await nav("账号管理");
await page.waitForSelector(".account-row");
await shot("12-account-narrow");
const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
check("H1 no horizontal overflow at 920px", !overflow);

// --- I. Loading state (slow refresh survives reload via init script) ---
await page.addInitScript(() => { window.__MOCK_SLOW__ = 1500; });
await page.goto(BASE, { waitUntil: "domcontentloaded" });
await page.waitForSelector(".sidebar");
await nav("账号管理");
await page.waitForSelector(".account-checking");
await shot("13-account-loading");
check("I1 initial checking state", true);
await page.waitForSelector(".account-row", { timeout: 10000 });
check("I2 resolves to list", (await rows().count()) === 3);

check("Z1 no page errors", pageErrors.length === 0, pageErrors.slice(0, 3).join(" | "));

await browser.close();
console.log(failures.length ? `\n${failures.length} FAILURES: ${failures.join(", ")}` : "\nALL CHECKS PASSED");
process.exit(failures.length ? 1 : 0);
