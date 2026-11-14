// Mock Tauri IPC for browser-only UI testing. Loaded before /src/main.ts.
// Never touches the real app or its data: everything lives in this page.
type Provider =
  | "quark"
  | "baidu"
  | "xunlei"
  | "pan115"
  | "pan123"
  | "c139"
  | "uc"
  | "lanzou"
  | "ilanzou";

const ALL: Provider[] = [
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

const folderBatch = {
  id: "cloud:quark:quark-acc:/电影:D:\\Downloads",
  name: "电影",
  path: "/电影",
};

const task = (
  id: string,
  name: string,
  status: string,
  size: number,
  completed: number,
  speed: number,
  batch: typeof folderBatch | null = null,
  error = "",
) => ({
  id,
  name,
  size,
  completed,
  speed,
  status,
  error,
  directory: "D:\\Downloads",
  createdAt: 1762000000,
  share: { provider: "quark", id: "", passcode: "" },
  file: { id, name, size, isDir: false, parentId: "" },
  ...(batch ? { batch } : {}),
});

const state = {
  tasks: [
    task(
      "movie-1",
      "流浪地球3.2027.2160p.mkv",
      "active",
      8_589_934_592,
      4_294_967_296,
      12_582_912,
      folderBatch,
    ),
    task(
      "movie-2",
      "满江红.2023.1080p.mkv",
      "waiting",
      2_147_483_648,
      0,
      0,
      folderBatch,
    ),
    task(
      "movie-3",
      "深海.2023.2160p.mkv",
      "paused",
      6_442_450_944,
      2_147_483_648,
      0,
      folderBatch,
    ),
    task("iso-1", "系统镜像.iso", "complete", 5_368_709_120, 5_368_709_120, 0),
    task(
      "broken-1",
      "损坏的视频.mp4",
      "error",
      1_073_741_824,
      0,
      0,
      null,
      "下载地址已过期",
    ),
  ],
  loggedIn: { quark: true, baidu: true, pan115: true } as Record<
    string,
    boolean
  >,
  names: {
    quark: "夸克用户_8823",
    baidu: "百度网友ab12",
    pan115: "115会员_2046",
  } as Record<string, string>,
  calls: [] as { cmd: string; args: unknown }[],
  uploads: [] as Record<string, unknown>[],
  handoffs: [] as {
    url: string;
    filename?: string | null;
    headers?: string[];
  }[],
  lastHandoff: null as {
    site: string;
    ok: boolean;
    message: string;
    at: number;
  } | null,
  refreshDelayMs: Number((window as any).__MOCK_SLOW__ || 0),
};

// Mirrors src/settings.ts defaultSettings (feat merge).
const settings = {
  downloadDir: "D:\\Downloads",
  maxOverallDownloadLimit: 0,
  maxConcurrentDownloads: 3,
  defaultConnections: 16,
  closeAction: "tray",
  autoCheckUpdates: true,
  uploadChunkMib: 8,
  uploadConcurrency: 2,
  uploadPartConcurrency: 4,
  minSplitMib: 1,
  browserBridge: true,
  bridgePort: 17890,
};

const account = (p: Provider) =>
  state.loggedIn[p]
    ? {
        provider: p,
        loggedIn: true,
        name: state.names[p] || `${p}用户`,
        message: "账号已连接",
      }
    : {
        provider: p,
        loggedIn: false,
        name: "未登录",
        message: "登录账号后开始下载",
      };

const driveFiles = (prefix: string) => [
  {
    id: `${prefix}-dir-1`,
    path: "/电影",
    name: "电影",
    size: 0,
    isDir: true,
    modified: 1762000000,
    created: 1761000000,
    md5: "",
  },
  {
    id: `${prefix}-dir-2`,
    path: "/文档",
    name: "文档",
    size: 0,
    isDir: true,
    modified: 1762100000,
    created: 1761100000,
    md5: "",
  },
  {
    id: `${prefix}-file-1`,
    path: "/流浪地球3.2027.2160p.mkv",
    name: "流浪地球3.2027.2160p.mkv",
    size: 8_589_934_592,
    isDir: false,
    modified: 1762200000,
    created: 1761200000,
    md5: "abc",
  },
  {
    id: `${prefix}-file-2`,
    path: "项目资料.zip",
    name: "项目资料.zip",
    size: 524_288_000,
    isDir: false,
    modified: 1762300000,
    created: 1761300000,
    md5: "def",
  },
];

let callbackId = 0;
const callbacks = new Map<number, (payload: unknown) => void>();
let eventId = 0;
const listeners = new Map<string, { id: number; handler: number }[]>();

function emit(event: string, payload: unknown = null) {
  for (const l of listeners.get(event) || []) {
    const cb = callbacks.get(l.handler);
    if (cb) cb({ event, id: l.id, payload });
  }
}

async function invoke(cmd: string, args: any = {}): Promise<unknown> {
  state.calls.push({ cmd, args });
  if (cmd === "plugin:event|listen") {
    const id = ++eventId;
    const list = listeners.get(args.event) || [];
    list.push({ id, handler: args.handler });
    listeners.set(args.event, list);
    return id;
  }
  if (cmd === "plugin:event|unlisten") {
    const id = args.eventId ?? args.id;
    listeners.set(
      args.event,
      (listeners.get(args.event) || []).filter((l) => l.id !== id),
    );
    return null;
  }
  if (cmd.startsWith("plugin:window|")) {
    if (cmd === "plugin:window|is_maximized") return false;
    return null;
  }
  if (cmd.startsWith("plugin:webview|")) return null;
  if (cmd === "plugin:dialog|open")
    return args?.options?.directory ? "D:\\Pictures" : null;
  if (cmd === "snapshot") {
    return {
      accounts: ALL.map(account),
      tasks: state.tasks.map((task) => ({ ...task })),
      downloadDir: "D:\\Downloads",
      engineRunning: true,
      engineError: "",
      uploads: state.uploads.map((upload) => ({ ...upload })),
      shares: [],
      settings: { ...settings },
    };
  }
  if (cmd === "refresh_account") {
    if (state.refreshDelayMs)
      await new Promise((r) => setTimeout(r, state.refreshDelayMs));
    return account(args.provider);
  }
  if (cmd === "open_login") {
    return null;
  }
  if (cmd === "logout_account") {
    state.loggedIn[args.provider] = false;
    return account(args.provider);
  }
  if (cmd === "cloud_list") {
    if (!state.loggedIn[args.provider]) throw "未登录，请先连接账号";
    return {
      accountId: `${args.provider}-acc`,
      path: args.path,
      files: driveFiles(args.provider),
      nextCursor: null,
    };
  }
  if (cmd === "cloud_search") {
    if (!state.loggedIn[args.provider]) throw "未登录，请先连接账号";
    const key = String(args.keyword ?? "")
      .trim()
      .toLowerCase();
    const files = driveFiles(args.provider).filter((file) =>
      file.name.toLowerCase().includes(key),
    );
    return {
      accountId: `${args.provider}-acc`,
      keyword: args.keyword,
      files,
      total: files.length,
      truncated: false,
    };
  }
  if (cmd === "drive_list") {
    if (!state.loggedIn.baidu) throw "未登录，请先连接账号";
    const base = args.path && args.path !== "/" ? args.path : "";
    return {
      accountId: "baidu-acc",
      path: args.path,
      page: args.page,
      hasMore: false,
      files: driveFiles("baidu").map((file) => ({
        ...file,
        path: file.isDir ? `${base}/${file.name}` : file.path,
      })),
    };
  }
  if (cmd === "dir_list_all") {
    if (args.provider === "baidu" && !state.loggedIn.baidu)
      throw "未登录，请先连接账号";
    if (args.provider !== "baidu" && !state.loggedIn[args.provider])
      throw "未登录，请先连接账号";
    const base = args.path && args.path !== "/" ? args.path : "";
    const files = driveFiles(args.provider).map((file) => ({
      ...file,
      path: file.isDir ? `${base}/${file.name}` : file.path,
    }));
    return {
      accountId: `${args.provider}-acc`,
      path: args.path,
      files,
      total: files.length,
      truncated: false,
    };
  }
  if (cmd === "cloud_enqueue" || cmd === "drive_enqueue") {
    const files = Array.isArray(args.files) ? args.files : [];
    return files.map(() => ({ status: "downloading", error: "" }));
  }
  if (cmd === "upload_enqueue") {
    const paths = Array.isArray(args.paths) ? args.paths : [];
    const directory = String(args.directory ?? "/");
    return paths.map((value) => {
      const localPath = String(value);
      const name = localPath.split(/[\\/]/).pop() || "file";
      const base = directory === "/" ? "" : directory;
      const index = state.uploads.length;
      const status = index % 2 === 0 ? "complete" : "uploading";
      const record = {
        id: `upload-${index}`,
        name,
        localPath,
        remotePath: `${base}/${name}`,
        size: 100,
        completed: status === "complete" ? 100 : 30,
        status,
        provider: args.provider,
        concurrency: 2,
        batch: args.batch ?? null,
      };
      state.uploads.push(record);
      return record;
    });
  }
  if (cmd === "upload_batch_action") {
    const ids = new Set(((args.ids as string[]) ?? []).map(String));
    const action = String(args.action ?? "");
    if (action === "remove") {
      const before = state.uploads.length;
      state.uploads = state.uploads.filter((u) => !ids.has(String(u.id)));
      return { done: before - state.uploads.length, failed: [] };
    }
    let done = 0;
    for (const upload of state.uploads) {
      if (!ids.has(String(upload.id))) continue;
      const status = String(upload.status);
      if (
        action === "pause" &&
        ["waiting", "hashing", "active"].includes(status)
      ) {
        upload.status = "paused";
        done += 1;
      } else if (action === "resume" && ["paused", "error"].includes(status)) {
        upload.status = "waiting";
        done += 1;
      }
    }
    return { done, failed: [] };
  }
  if (cmd === "upload_action") {
    const id = String(args.id ?? "");
    const action = String(args.action ?? "");
    const upload = state.uploads.find((u) => String(u.id) === id);
    if (!upload) throw new Error("上传任务不存在");
    if (action === "remove") {
      state.uploads = state.uploads.filter((u) => String(u.id) !== id);
    } else {
      upload.status = action === "pause" ? "paused" : "waiting";
    }
    return null;
  }
  if (cmd === "collect_local_files") {
    const root = String(args.root ?? "");
    return [
      { path: `${root}\\a.txt`, relative: "" },
      { path: `${root}\\sub\\b.txt`, relative: "sub" },
      { path: `${root}\\sub\\deep\\c.txt`, relative: "sub/deep" },
    ];
  }
  if (cmd === "drive_mkdir" || cmd === "drive_mkdir_at") {
    return {
      id: `dir-${Date.now()}`,
      path: `${args.parent === "/" ? "" : args.parent}/${args.name}`,
      name: args.name,
      size: 0,
      isDir: true,
      modified: Math.floor(Date.now() / 1000),
      created: Math.floor(Date.now() / 1000),
      md5: "",
    };
  }
  if (cmd === "drive_share") {
    const names =
      (args.files as { name: string }[] | undefined)?.map((f) => f.name) ?? [];
    return {
      id: `mock-${args.provider ?? "baidu"}-share`,
      url: `https://${args.provider ?? "baidu"}.example.com/s/1mockShare`,
      password: args.password ?? "abcd",
      period: args.period,
      createdAt: Math.floor(Date.now() / 1000),
      accountId: args.accountId,
      names,
      cancelled: false,
    };
  }
  if (cmd === "drive_rename_at") {
    const file = args.file as Record<string, unknown>;
    return { ...file, name: args.newName };
  }
  if (cmd === "drive_delete_at") {
    return null;
  }
  if (cmd === "task_action") {
    const index = state.tasks.findIndex((t) => t.id === args.id);
    if (index < 0) throw "下载任务不存在";
    if (args.action === "remove") state.tasks.splice(index, 1);
    else if (args.action === "pause")
      state.tasks[index] = {
        ...state.tasks[index],
        status: "paused",
        speed: 0,
      };
    else if (args.action === "resume")
      state.tasks[index] = { ...state.tasks[index], status: "waiting" };
    return null;
  }
  if (cmd === "task_batch_action") {
    const ids = new Set<string>(Array.isArray(args.ids) ? args.ids : []);
    if (args.action === "remove")
      state.tasks = state.tasks.filter((t) => !ids.has(t.id));
    else
      state.tasks = state.tasks.map((t) =>
        ids.has(t.id)
          ? {
              ...t,
              status: args.action === "pause" ? "paused" : "waiting",
              speed: 0,
            }
          : t,
      );
    return { done: ids.size, failed: [] };
  }
  // Browser hand-off: the backend queues the link, then emits a wake-up event.
  if (cmd === "take_handoff") {
    const links = [...state.handoffs];
    state.handoffs.length = 0;
    return links;
  }
  if (cmd === "inspect_download") {
    const link = String(args.text ?? "").trim();
    const name =
      String(args.filename ?? "").trim() ||
      link.split("/").pop()?.split("?")[0] ||
      "file.bin";
    return {
      kind: "direct",
      url: link,
      name,
      size: 1024,
      rangeSupported: true,
    };
  }
  // Starting a download returns the task the backend created; the real command
  // answers with the same shape, so the list only refreshes afterwards.
  if (cmd === "enqueue_direct" || cmd === "enqueue_bt") {
    const name =
      String(args.filename ?? "").trim() ||
      String(args.url ?? "")
        .split("/")
        .pop()
        ?.split("?")[0] ||
      "下载任务";
    return task(`created-${state.calls.length}`, name, "resolving", 0, 0, 0);
  }
  // feat merge: settings + in-app updater.
  if (cmd === "settings_get") return { ...settings };
  if (cmd === "bridge_status")
    return {
      listening: true,
      port: settings.bridgePort,
      enabled: settings.browserBridge,
      last: state.lastHandoff,
    };
  if (cmd === "settings_set") {
    Object.assign(settings, args.settings);
    return { ...settings };
  }
  if (cmd === "open_data_directory") return null;
  if (cmd === "plugin:updater|check") return null; // no update available
  if (cmd.startsWith("plugin:process|")) return null;
  return null;
}

(window as any).__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main" },
  },
  transformCallback(cb: (payload: unknown) => void) {
    const id = ++callbackId;
    callbacks.set(id, cb);
    return id;
  },
  unregisterCallback(id: number) {
    callbacks.delete(id);
  },
  convertFileSrc: (p: string) => p,
  invoke,
};

// Injected by the real event plugin's init script; needed by listen()'s unlisten.
(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener(event: string, id: number) {
    listeners.set(
      event,
      (listeners.get(event) || []).filter((l) => l.id !== id),
    );
  },
};

(window as any).__MOCK__ = {
  state,
  emit,
  completeLogin(p: Provider) {
    state.loggedIn[p] = true;
    state.names[p] = `${p}新账号`;
  },
  // Mirrors the backend emitting "login-window-closed" when the user dismisses
  // the login WebView (session.rs CloseRequested handler).
  closeLoginWindow(p: Provider) {
    emit("login-window-closed", p);
  },
  setAllLoggedOut() {
    for (const p of ALL) state.loggedIn[p] = false;
    emit("tasks-changed");
  },
  // Mirrors the backend: the link is queued before the event is emitted, so a
  // late listener still sees it through take_handoff. A plain string is the
  // clouddownloader:// form; an object is what the extension bridge delivers.
  handoff(
    link:
      string | { url: string; filename?: string | null; headers?: string[] },
  ) {
    state.handoffs.push(typeof link === "string" ? { url: link } : link);
    emit("handoff", null);
  },
  lastCall(cmd: string) {
    return [...state.calls].reverse().find((c) => c.cmd === cmd);
  },
};
