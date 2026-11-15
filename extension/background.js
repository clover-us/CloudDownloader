// Hands browser downloads to CloudDownloader.
//
// A page download cannot be started by another program, so this worker watches
// chrome.downloads, asks the app to take the URL (with the cookies, referrer and
// user agent the browser had), and only cancels the browser copy once the app
// has accepted it. If the app is not running the download simply continues.

const DEFAULTS = {
  enabled: true,
  port: 17890,
  minSizeMb: 0,
  sites: [],
  notifyOnFailure: true,
};

async function settings() {
  return { ...DEFAULTS, ...(await chrome.storage.local.get(DEFAULTS)) };
}

/** Empty list means every site; otherwise only these hosts and their subdomains. */
function siteAllowed(url, sites) {
  if (!sites.length) return true;
  try {
    const host = new URL(url).hostname;
    return sites.some((site) => host === site || host.endsWith(`.${site}`));
  } catch {
    return false;
  }
}

function cookieHeader(cookies) {
  return cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join("; ");
}

async function handOff(url, filename, referrer) {
  const { port } = await settings();
  // A blob:/data: download has no URL the app could fetch, so it never gets here.
  // A magnet link cannot be fetched either and carries no cookies, but the app
  // accepts it as a BT task, so it is forwarded without header enrichment.
  const isMagnet = /^magnet:/i.test(url);
  if (!isMagnet && !/^https?:/i.test(url))
    throw new Error("只支持 HTTP/HTTPS 直链与磁力链接");
  const cookies = isMagnet ? [] : await chrome.cookies.getAll({ url });
  let response;
  try {
    response = await fetch(`http://127.0.0.1:${port}/add`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url,
        filename: filename || undefined,
        referrer: isMagnet ? undefined : referrer || undefined,
        cookie: cookies.length ? cookieHeader(cookies) : undefined,
        userAgent: isMagnet ? undefined : navigator.userAgent,
      }),
    });
  } catch {
    throw new Error(`连不上 127.0.0.1:${port}，CloudDownloader 可能没有运行`);
  }
  if (!response.ok) {
    const detail = (await response.text().catch(() => "")).trim();
    throw new Error(
      detail
        ? `应用拒绝了这次请求：${detail}`
        : `应用拒绝了这次请求（HTTP ${response.status}）`,
    );
  }
  return response.json();
}

// A failed hand-off must not be silent: the browser keeps the download, but the
// user should learn why the app did not take it. Throttled, because a broken
// bridge would otherwise pop a notification for every file on the page.
let lastNotice = 0;
async function notifyFailure(error) {
  console.warn("CloudDownloader 接力失败，交给浏览器继续下载", error);
  const { notifyOnFailure } = await settings();
  if (!notifyOnFailure) return;
  const now = Date.now();
  if (now - lastNotice < 20000) return;
  lastNotice = now;
  chrome.notifications.create(`handoff-${now}`, {
    type: "basic",
    iconUrl: "icon128.png",
    title: "这次下载仍由浏览器完成",
    message: `${String(error?.message || error)}。浏览器会继续下载这份文件。`,
  });
}

const handing = new Set();

chrome.downloads.onCreated.addListener(async (item) => {
  const { enabled, minSizeMb, sites } = await settings();
  if (!enabled || handing.has(item.id)) return;
  const url = item.finalUrl || item.url || "";
  if (!/^https?:/i.test(url) && !/^magnet:/i.test(url)) return;
  if (!siteAllowed(item.referrer || url, sites)) return;
  if (
    minSizeMb > 0 &&
    item.fileSize > 0 &&
    item.fileSize < minSizeMb * 1024 * 1024
  )
    return;
  handing.add(item.id);
  try {
    const result = await handOff(url, item.filename, item.referrer);
    // The app said this link should stay in the browser (it opened it itself,
    // or the user chose the browser): leave the download running.
    if (result && result.taken === false) return;
    // The app owns it now; stop the browser copy so the file is not fetched twice.
    await chrome.downloads.cancel(item.id);
    await chrome.downloads.erase({ id: item.id });
  } catch (error) {
    await notifyFailure(error);
  } finally {
    handing.delete(item.id);
  }
});

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "clouddownloader-link",
    title: "用 CloudDownloader 下载",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== "clouddownloader-link" || !info.linkUrl) return;
  try {
    await handOff(info.linkUrl, "", info.pageUrl);
  } catch (error) {
    await notifyFailure(error);
  }
});

// The desktop app cannot look inside the browser, so this worker says hello on a
// timer. The title-bar plugin button treats a recent hello as "installed and
// working" and silence as "not installed, disabled, or the app is not running".
async function hello() {
  const { port } = await settings();
  try {
    await fetch(`http://127.0.0.1:${port}/ping`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ version: chrome.runtime.getManifest().version }),
    });
  } catch {
    // Nothing to report: the app is simply not running right now.
  }
}

function beat() {
  chrome.alarms.create("heartbeat", { periodInMinutes: 1 });
  void hello();
}

chrome.runtime.onInstalled.addListener(beat);
chrome.runtime.onStartup.addListener(beat);
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "heartbeat") void hello();
});
beat();