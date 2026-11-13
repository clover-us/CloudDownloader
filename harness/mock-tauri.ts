// Mock Tauri IPC for browser-only UI testing. Loaded before /src/main.ts.
// Never touches the real app or its data: everything lives in this page.
type Provider =
  | "quark"
  | "baidu"
  | "pan115"
  | "pan123"
  | "c139"
  | "uc"
  | "lanzou"
  | "ilanzou";

const ALL: Provider[] = [
  "quark",
  "baidu",
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
  refreshDelayMs: Number((window as any).__MOCK_SLOW__ || 0),
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
    md5: "",
  },
  {
    id: `${prefix}-dir-2`,
    path: "/文档",
    name: "文档",
    size: 0,
    isDir: true,
    modified: 1762100000,
    md5: "",
  },
  {
    id: `${prefix}-file-1`,
    path: "/流浪地球3.2027.2160p.mkv",
    name: "流浪地球3.2027.2160p.mkv",
    size: 8_589_934_592,
    isDir: false,
    modified: 1762200000,
    md5: "abc",
  },
  {
    id: `${prefix}-file-2`,
    path: "项目资料.zip",
    name: "项目资料.zip",
    size: 524_288_000,
    isDir: false,
    modified: 1762300000,
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
  if (cmd === "plugin:dialog|open") return null;
  if (cmd === "snapshot") {
    return {
      accounts: ALL.map(account),
      tasks: [],
      downloadDir: "D:\\Downloads",
      engineRunning: true,
      engineError: "",
      uploads: [],
      shares: [],
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
    return {
      accountId: "baidu-acc",
      path: args.path,
      page: args.page,
      hasMore: false,
      files: driveFiles("baidu"),
    };
  }
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
