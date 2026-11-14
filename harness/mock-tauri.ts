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

const state = {
  loggedIn: { quark: true, baidu: true, pan115: true } as Record<string, boolean>,
  names: {
    quark: "夸克用户_8823",
    baidu: "百度网友ab12",
    pan115: "115会员_2046",
  } as Record<string, string>,
  calls: [] as { cmd: string; args: unknown }[],
  uploads: [] as Record<string, unknown>[],
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
      tasks: [],
      downloadDir: "D:\\Downloads",
      engineRunning: true,
      engineError: "",
      uploads: state.uploads,
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
      };
      state.uploads.push(record);
      return record;
    });
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
    const names = (args.files as { name: string }[] | undefined)?.map((f) => f.name) ?? [];
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
  // feat merge: settings + in-app updater.
  if (cmd === "settings_get") return { ...settings };
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
  lastCall(cmd: string) {
    return [...state.calls].reverse().find((c) => c.cmd === cmd);
  },
};
