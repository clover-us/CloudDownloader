const DEFAULTS = { enabled: true, port: 17890, minSizeMb: 0, sites: [] };
const $ = (id) => document.getElementById(id);

async function load() {
  const saved = { ...DEFAULTS, ...(await chrome.storage.local.get(DEFAULTS)) };
  $("enabled").checked = saved.enabled;
  $("port").value = saved.port;
  $("minSizeMb").value = saved.minSizeMb;
  $("sites").value = saved.sites.join("\n");
}

function collect() {
  const sites = $("sites")
    .value.split(/\r?\n/)
    .map((line) => line.trim().toLowerCase())
    .filter(Boolean)
    .map((line) => line.replace(/^https?:\/\//, "").replace(/\/.*$/, ""));
  return {
    enabled: $("enabled").checked,
    port: Math.min(65535, Math.max(1, Number($("port").value) || 17890)),
    minSizeMb: Math.max(0, Number($("minSizeMb").value) || 0),
    sites: [...new Set(sites)],
  };
}

$("save").addEventListener("click", async () => {
  const saved = collect();
  await chrome.storage.local.set(saved);
  $("status").textContent = "已保存";
  $("status").className = "ok";
});

// The app answers /ping only for extension origins, so a reply means both the
// app and this extension are wired up correctly.
$("test").addEventListener("click", async () => {
  const { port } = collect();
  $("status").textContent = "正在连接…";
  $("status").className = "";
  try {
    const response = await fetch(`http://127.0.0.1:${port}/ping`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}",
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const body = await response.json();
    if (body.app !== "clouddownloader") throw new Error("端口上不是 CloudDownloader");
    $("status").textContent = "已连接";
    $("status").className = "ok";
  } catch (error) {
    $("status").textContent = `连不上：${error.message}。请先打开 CloudDownloader。`;
    $("status").className = "bad";
  }
});

void load();