// End-to-end demo: system Edge + the unpacked extension + the running app.
//
// Serves a downloadable file, launches Edge with the extension loaded, clicks
// the link, then leaves the window open so the app's hand-off can be seen.
// Run: node harness/extension-demo.mjs
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";

const EXT = path.resolve("extension");
const SIZE = 16 * 1024 * 1024;

const server = http.createServer((req, res) => {
  if (req.url.startsWith("/file.bin")) {
    res.writeHead(200, {
      "Content-Type": "application/octet-stream",
      "Content-Disposition": 'attachment; filename="demo.bin"',
      "Content-Length": String(SIZE),
    });
    res.end(Buffer.alloc(SIZE, 7));
    return;
  }
  res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
  res.end(`<!doctype html><meta charset="utf-8"><title>接力演示</title>
<p>点下面的链接，浏览器应该把下载交给 CloudDownloader。</p>
<a id="go" href="/file.bin" download>下载 demo.bin</a>`);
});
await new Promise((resolve) => server.listen(8123, "127.0.0.1", resolve));
console.log("serving http://127.0.0.1:8123/");

const context = await chromium.launchPersistentContext("", {
  channel: "msedge",
  headless: false,
  args: [
    `--disable-extensions-except=${EXT}`,
    `--load-extension=${EXT}`,
    "--no-first-run",
  ],
});
const page = await context.newPage();
await page.goto("http://127.0.0.1:8123/");
await page.waitForTimeout(1500);
await page.click("#go");
console.log("clicked the link; the app should wake up in a moment");
await page.waitForTimeout(8000);
console.log("demo window stays open for 90s");
await page.waitForTimeout(90_000);
await context.close();
server.close();