// Regenerate the two README screenshots against the mock-Tauri harness.
// Run: npx vite --config harness/vite.config.ts   (separate terminal, :1420)
//      node harness/readme-shots.mjs
// Writes docs/images/download-light.png and docs/images/drive-night.png.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const BASE = process.env.BASE ?? "http://localhost:1420/harness/index.html";
const OUT = new URL("../docs/images/", import.meta.url).pathname.replace(
  /^\/([A-Za-z]:)/,
  "$1",
);
mkdirSync(OUT, { recursive: true });

let browser;
try {
  browser = await chromium.launch({ channel: "msedge", headless: true });
  console.log("browser: system Edge");
} catch {
  browser = await chromium.launch({ headless: true });
  console.log("browser: bundled chromium");
}
const page = await browser.newPage({
  viewport: { width: 1280, height: 880 },
  deviceScaleFactor: 1,
});
const pageErrors = [];
page.on("pageerror", (e) => pageErrors.push(e.stack || String(e)));
page.on("console", (m) => {
  if (
    m.type() === "error" &&
    !/favicon|Download the Vue Devtools/i.test(m.text())
  )
    pageErrors.push(m.text());
});

const nav = (label) =>
  page.locator(".sidebar nav button", { hasText: label }).first().click();
const shot = (name) => page.screenshot({ path: `${OUT}${name}.png` });

await page.goto(BASE, { waitUntil: "networkidle" });
await page.waitForSelector(".sidebar");
await page.waitForFunction(() =>
  window.__MOCK__.state.calls.some(
    (c) => c.cmd === "refresh_account" && c.args.provider === "ilanzou",
  ),
);

// Seed a representative 0.7.8 state: all nine drives connected, three download
// sources (direct link, BT, personal-drive file), and the README download dir.
await page.evaluate(async () => {
  const s = window.__MOCK__.state;
  const providers = [
    "quark",
    "baidu",
    "xunlei",
    "pan115",
    "pan123",
    "c139",
    "uc",
    "lanzou",
    "ilanzou",
  ];
  const names = {
    quark: "夸克用户_8823",
    baidu: "百度网友ab12",
    xunlei: "迅雷用户_7788",
    pan115: "115会员_2046",
    pan123: "123用户_3021",
    c139: "139用户_5566",
    uc: "UC用户_9900",
    lanzou: "蓝奏用户_1122",
    ilanzou: "优享用户_3344",
  };
  for (const p of providers) {
    s.loggedIn[p] = true;
    s.names[p] = names[p];
  }
  const MiB = 1048576;
  const GiB = 1073741824;
  const dir = "D:\\Downloads\\CloudDownloader";
  const file = (id, name, size) => ({ id, name, size, isDir: false, parentId: "" });
  s.tasks = [
    {
      id: "demo-zip",
      name: "示例安装包.zip",
      status: "active",
      size: 256 * MiB,
      completed: 160 * MiB,
      speed: 5 * MiB,
      error: "",
      directory: dir,
      createdAt: 1762000300,
      connections: 12,
      direct: {
        url: "https://example.com/files/setup.zip",
        connections: 16,
        rangeSupported: true,
      },
      share: { provider: "quark", id: "", passcode: "" },
      file: file("demo-zip", "示例安装包.zip", 256 * MiB),
    },
    {
      id: "demo-bt",
      name: "示例视频资源",
      status: "active",
      size: 2 * GiB,
      completed: 768 * MiB,
      speed: 3 * MiB,
      error: "",
      directory: dir,
      createdAt: 1762000200,
      connections: 24,
      uploadSpeed: 256 * 1024,
      bt: { selected: [0, 1], maxPeers: 64 },
      share: { provider: "quark", id: "", passcode: "" },
      file: file("demo-bt", "示例视频资源", 2 * GiB),
    },
    {
      id: "demo-pdf",
      name: "学习资料.pdf",
      status: "complete",
      size: 4404019,
      completed: 4404019,
      speed: 0,
      error: "",
      directory: dir,
      createdAt: 1762000100,
      share: { provider: "baidu", id: "", passcode: "" },
      own: { accountId: "baidu-acc" },
      file: file("demo-pdf", "学习资料.pdf", 4404019),
    },
  ];
  await window.__TAURI_INTERNALS__.invoke("settings_set", {
    settings: { downloadDir: dir },
  });
  window.__MOCK__.emit("tasks-changed");
});

// --- download-light.png: light theme, 全部任务 ---
await page.waitForFunction(
  () => document.documentElement.dataset.theme === "light",
);
await page.getByText("9 个网盘已连接").first().waitFor();
await nav("全部任务");
await page.waitForSelector(".task-row");
await page.waitForFunction(
  () => document.querySelectorAll(".task-row").length >= 3,
);
await page.waitForTimeout(300);
await shot("download-light");
console.log("wrote download-light.png");

// --- drive-night.png: Night theme, 我的网盘 / 百度网盘, two files selected ---
await page.locator(".theme-toggle").click();
await page.waitForFunction(
  () => document.documentElement.dataset.theme === "night",
);
await nav("我的网盘");
await page.waitForSelector(".drive-providers");
await page
  .locator(".drive-providers button", { hasText: "百度网盘" })
  .first()
  .click();
await page.waitForSelector(".drive-file-row");
await page
  .locator('input[aria-label="选择 流浪地球3.2027.2160p.mkv"]')
  .check();
await page.locator('input[aria-label="选择 项目资料.zip"]').check();
await page.getByText("已选中").first().waitFor();
await page.waitForTimeout(300);
await shot("drive-night");
console.log("wrote drive-night.png");

await browser.close();
if (pageErrors.length) {
  console.error("PAGE ERRORS:\n" + pageErrors.join("\n"));
  process.exit(1);
}
console.log("done");
