// Hands browser downloads to CloudDownloader.
//
// A page download cannot be started by another program, so this worker watches
// chrome.downloads, asks the app to take the URL (with the cookies, referrer and
// user agent the browser had), and only cancels the browser copy once the app
// has accepted it. If the app is not running the download simply continues.

const DEFAULTS = { enabled: true, port: 17890, minSizeMb: 0, sites: [] };

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
  if (!/^https?:/i.test(url)) throw new Error("只支持 HTTP/HTTPS 直链");
  const cookies = await chrome.cookies.getAll({ url });
  const response = await fetch(`http://127.0.0.1:${port}/add`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      url,
      filename: filename || undefined,
      referrer: referrer || undefined,
      cookie: cookies.length ? cookieHeader(cookies) : undefined,
      userAgent: navigator.userAgent,
    }),
  });
  if (!response.ok) {
    const detail = await response.text().catch(() => "");
    throw new Error(`CloudDownloader 拒绝了请求（HTTP ${response.status}）${detail}`);
  }
  return response.json();
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
    await handOff(url, item.filename, item.referrer);
    // The app owns it now; stop the browser copy so the file is not fetched twice.
    await chrome.downloads.cancel(item.id);
    await chrome.downloads.erase({ id: item.id });
  } catch (error) {
    // Keep the browser download: losing it without a replacement is worse.
    console.warn("CloudDownloader 接力失败，交给浏览器继续下载", error);
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
    console.warn("CloudDownloader 接力失败", error);
  }
});