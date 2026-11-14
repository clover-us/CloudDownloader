<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import DriveExplorer from "./DriveExplorer.vue";
import AccountDialog from "./AccountDialog.vue";
import {
  type Provider,
  driveSummary,
  providers,
  providerName,
} from "./providers";
import UploadPanel from "./UploadPanel.vue";
import SharePanel from "./SharePanel.vue";
import SettingsPanel from "./SettingsPanel.vue";
import DownloadBox from "./DownloadBox.vue";
import TitleBar from "./TitleBar.vue";
import { version } from "../package.json";
import { type Settings as AppSettings, defaultSettings } from "./settings";
import { checkForUpdates, updateStatus } from "./updater";
import type { UploadTask, CreatedShare } from "./cloud";
import {
  ArrowDownToLine,
  ArrowDown,
  ArrowUpRight,
  ArrowUpFromLine,
  Share2,
  Check,
  CheckCheck,
  ChevronRight,
  CircleAlert,
  CircleCheck,
  CloudDownload,
  File,
  FileArchive,
  FileImage,
  FileText,
  Music2,
  FileVideo,
  Folder,
  FolderOpen,
  Link,
  LoaderCircle,
  LogOut,
  Pause,
  Play,
  Plus,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Trash2,
  UserRound,
  X,
} from "@lucide/vue";

type Account = {
  provider: Provider;
  loggedIn: boolean;
  name: string;
  message: string;
};
type Share = { provider: Provider; id: string; passcode: string };
type TaskBatch = { id: string; name: string; path?: string };
/** Mirrors `handoff::Handoff`: the link plus what the browser knew about it. */
type Handoff = { url: string; filename?: string | null; headers?: string[] };
type Task = {
  bt?: { selected: number[]; maxPeers: number } | null;
  uploadSpeed?: number;
  direct?: { url: string; connections: number; rangeSupported: boolean } | null;
  connections?: number;
  own?: { accountId: string } | null;
  batch?: TaskBatch | null;
  share: Share;
  id: string;
  name: string;
  size: number;
  completed: number;
  speed: number;
  status: string;
  error: string;
  directory: string;
  createdAt: number;
};
type TaskGroup = {
  id: string;
  name: string;
  path: string;
  tasks: Task[];
  size: number;
  completed: number;
  speed: number;
  status: string;
  done: number;
  total: number;
  progress: number;
};
type TaskRow =
  | { key: string; kind: "group"; group: TaskGroup }
  | { key: string; kind: "task"; task: Task; child: boolean };
type RemoveTarget = { ids: string[]; name: string; directory: string };
type Snapshot = {
  accounts: Account[];
  tasks: Task[];
  downloadDir: string;
  engineRunning: boolean;
  engineError: string;
  uploads: UploadTask[];
  shares: CreatedShare[];
  settings: AppSettings;
};
const state = ref<Snapshot>({
  accounts: providers.map((p) => ({
    provider: p.id,
    loggedIn: false,
    name: "未登录",
    message: "登录账号后开始下载",
  })),
  tasks: [],
  downloadDir: "",
  engineRunning: false,
  engineError: "",
  uploads: [],
  shares: [],
  settings: { ...defaultSettings },
});
const tab = ref("all");
const driveProvider = ref<Provider>("quark");
const driveBusy = ref(false);
const downloadBox = ref<InstanceType<typeof DownloadBox> | null>(null);
const signingIn = ref<Provider[]>([]);
const refreshing = ref<Provider[]>([]);
const accountsReady = ref(false);
const addDialog = ref<"" | "account" | "drive">("");
const logoutTarget = ref<Provider | null>(null);
const loggingOut = ref(false);
const logoutError = ref("");
const accountFor = (provider: Provider) =>
  state.value.accounts.find((a) => a.provider === provider) || {
    provider,
    loggedIn: false,
    name: "未登录",
    message: "登录账号后开始下载",
  };
const connectedCount = computed(
  () => state.value.accounts.filter((a) => a.loggedIn).length,
);
const connectedProviders = computed(() =>
  providers.filter((p) => accountFor(p.id).loggedIn),
);
const driveTabs = computed(() =>
  providers.filter((p) => accountFor(p.id).loggedIn),
);
const error = ref("");
const toast = ref("");
const filter = ref("");
const pendingAction = ref<string | null>(null);
const selected = ref<string[]>([]);
const expanded = ref<string[]>([]);
const batchBusy = ref(false);
const removeTarget = ref<RemoveTarget | null>(null);
const deleteLocalFiles = ref(false);
const removing = ref(false);
const removeError = ref("");
let interval: ReturnType<typeof setInterval> | undefined;
let toastTimer: ReturnType<typeof setTimeout> | undefined;
let unlisten: UnlistenFn | undefined;
let unlistenLoginClosed: UnlistenFn | undefined;
let unlistenHandoff: UnlistenFn | undefined;
let loadingSnapshot = false;
const activeStatuses = ["active", "waiting", "resolving", "seeding"];
const completed = computed(() =>
  state.value.tasks.filter((t) => t.status === "complete"),
);
const speed = computed(() =>
  state.value.tasks.reduce((sum, t) => sum + t.speed, 0),
);
const uploadSpeed = computed(
  () =>
    state.value.tasks.reduce((sum, t) => sum + (t.uploadSpeed || 0), 0) +
    state.value.uploads.reduce((sum, t) => sum + t.speed, 0),
);
const tasks = computed(() =>
  state.value.tasks.filter(
    (t) =>
      (tab.value === "all" ||
        (tab.value === "active" && activeStatuses.includes(t.status)) ||
        (tab.value === "complete" && t.status === "complete") ||
        (tab.value === "error" && t.status === "error")) &&
      t.name.toLowerCase().includes(filter.value.toLowerCase()),
  ),
);
const title = computed(
  () =>
    ({
      all: "全部下载",
      active: "正在下载",
      complete: "已完成",
      error: "需要处理",
      account: "账号管理",
      drive: "我的网盘",
      uploads: "上传任务",
      shares: "分享记录",
      settings: "设置",
    })[tab.value],
);
const statusForGroup = (children: Task[]) => {
  if (!children.length) return "paused";
  if (children.every((t) => t.status === "complete")) return "complete";
  for (const status of ["resolving", "active", "waiting", "error", "paused"])
    if (children.some((t) => t.status === status)) return status;
  return children[0].status;
};
function buildGroup(
  id: string,
  name: string,
  path: string,
  children: Task[],
): TaskGroup {
  const size = children.reduce((sum, t) => sum + (t.size || 0), 0);
  const completed = children.reduce((sum, t) => sum + (t.completed || 0), 0);
  const done = children.filter((t) => t.status === "complete").length;
  return {
    id,
    name,
    path,
    tasks: children,
    size,
    completed,
    speed: children.reduce((sum, t) => sum + t.speed, 0),
    status: statusForGroup(children),
    done,
    total: children.length,
    progress: size
      ? Math.min(100, (completed / size) * 100)
      : (done / children.length) * 100,
  };
}
// Flat display rows: folder groups (collapsed by default) followed by their
// child tasks when expanded. Individual downloads stay one row each.
const rows = computed<TaskRow[]>(() => {
  const keyword = filter.value.trim().toLowerCase();
  const matchesName = (t: Task) =>
    !keyword || t.name.toLowerCase().includes(keyword);
  const matchesTab = (t: Task) =>
    tab.value === "all" ||
    (tab.value === "active" && activeStatuses.includes(t.status)) ||
    (tab.value === "complete" && t.status === "complete") ||
    (tab.value === "error" && t.status === "error");
  const groups = new Map<string, Task[]>();
  for (const task of state.value.tasks) {
    const batch = task.batch;
    if (!batch) continue;
    const list = groups.get(batch.id);
    if (list) list.push(task);
    else groups.set(batch.id, [task]);
  }
  const result: TaskRow[] = [];
  const emitted = new Set<string>();
  for (const task of state.value.tasks) {
    const batch = task.batch;
    if (!batch) {
      if (matchesTab(task) && matchesName(task))
        result.push({ key: task.id, kind: "task", task, child: false });
      continue;
    }
    if (emitted.has(batch.id)) continue;
    emitted.add(batch.id);
    const children = groups.get(batch.id) || [task];
    // A folder is one unit in the status tabs: it only appears under
    // 已完成 when every file inside is complete, and under 需要处理 when the
    // folder as a whole needs attention.
    const groupStatus = statusForGroup(children);
    const groupInTab =
      tab.value === "all" ||
      (tab.value === "active" && activeStatuses.includes(groupStatus)) ||
      (tab.value === "complete" && groupStatus === "complete") ||
      (tab.value === "error" && groupStatus === "error");
    if (!groupInTab) continue;
    const inTab = children.filter(matchesTab);
    if (!inTab.length) continue;
    const folderMatched =
      !!keyword && batch.name.toLowerCase().includes(keyword);
    const shown = keyword && !folderMatched ? inTab.filter(matchesName) : inTab;
    if (!shown.length) continue;
    result.push({
      key: `group:${batch.id}`,
      kind: "group",
      group: buildGroup(batch.id, batch.name, batch.path || "", children),
    });
    if (expanded.value.includes(batch.id))
      for (const child of shown)
        result.push({ key: child.id, kind: "task", task: child, child: true });
  }
  return result;
});
// Folder rows and standalone downloads are the "items" in the heading; expanded
// child files are part of their folder and are not counted again.
const topLevelCount = computed(
  () => rows.value.filter((row) => row.kind === "group" || !row.child).length,
);
// Nav badges follow the same unit as the list: a folder counts once, and only
// under the status its files produce together.
const unitCounts = computed(() => {
  const children = new Map<string, Task[]>();
  for (const t of state.value.tasks) {
    if (!t.batch) continue;
    const list = children.get(t.batch.id);
    if (list) list.push(t);
    else children.set(t.batch.id, [t]);
  }
  const seen = new Set<string>();
  const counts = { all: 0, active: 0, complete: 0, error: 0 };
  const add = (status: string) => {
    counts.all += 1;
    if (activeStatuses.includes(status)) counts.active += 1;
    else if (status === "complete") counts.complete += 1;
    else if (status === "error") counts.error += 1;
  };
  for (const t of state.value.tasks) {
    if (t.batch) {
      if (seen.has(t.batch.id)) continue;
      seen.add(t.batch.id);
      add(statusForGroup(children.get(t.batch.id) || [t]));
    } else add(t.status);
  }
  return counts;
});
const selectedSet = computed(() => new Set(selected.value));
const allSelected = computed(
  () =>
    !!tasks.value.length &&
    tasks.value.every((t) => selectedSet.value.has(t.id)),
);
const someSelected = computed(
  () =>
    !allSelected.value && tasks.value.some((t) => selectedSet.value.has(t.id)),
);
function toggleTask(id: string) {
  selected.value = selectedSet.value.has(id)
    ? selected.value.filter((item) => item !== id)
    : [...selected.value, id];
}
function toggleSelectAll() {
  selected.value = allSelected.value ? [] : tasks.value.map((t) => t.id);
}
function groupSelected(group: TaskGroup) {
  const ids = group.tasks.map((t) => t.id);
  const hits = ids.filter((id) => selectedSet.value.has(id)).length;
  return hits === 0 ? "none" : hits === ids.length ? "all" : "some";
}
function toggleGroupSelection(group: TaskGroup) {
  const ids = group.tasks.map((t) => t.id);
  const all = ids.every((id) => selectedSet.value.has(id));
  selected.value = all
    ? selected.value.filter((id) => !ids.includes(id))
    : [...new Set([...selected.value, ...ids])];
}
function toggleGroup(id: string) {
  expanded.value = expanded.value.includes(id)
    ? expanded.value.filter((item) => item !== id)
    : [...expanded.value, id];
}
const groupPausable = (group: TaskGroup) =>
  group.tasks
    .filter((t) =>
      ["active", "waiting", "seeding", "resolving"].includes(t.status),
    )
    .map((t) => t.id);
const groupResumable = (group: TaskGroup) =>
  group.tasks
    .filter((t) => ["paused", "error", "cancelled"].includes(t.status))
    .map((t) => t.id);
const bytes = (n: number) => {
  if (!n) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), 4);
  return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${units[i]}`;
};
const progress = (t: Task) =>
  t.size
    ? Math.min(100, (t.completed / t.size) * 100)
    : t.status === "complete"
      ? 100
      : 0;
const statusName = (status: string) =>
  ({
    active: "下载中",
    seeding: "做种中",
    waiting: "等待中",
    resolving: "获取地址",
    paused: "已暂停",
    complete: "已完成",
    error: "下载失败",
    removed: "已移除",
    cancelled: "已取消",
  })[status] || status;
const fileKind = (name: string) => {
  if (/\.(mp4|mkv|avi|mov|webm|flv|ts)$/i.test(name)) return "video";
  if (/\.(zip|rar|7z|tar|gz|iso|exe|msi)$/i.test(name)) return "archive";
  if (/\.(png|jpg|jpeg|gif|webp|svg|heic)$/i.test(name)) return "image";
  if (/\.(mp3|wav|flac|aac|ogg|m4a)$/i.test(name)) return "audio";
  if (/\.(pdf|docx?|xlsx?|pptx?|txt|md|csv)$/i.test(name)) return "document";
  return "file";
};
const fileIcon = (name: string) =>
  ({
    video: FileVideo,
    archive: FileArchive,
    image: FileImage,
    audio: Music2,
    document: FileText,
    file: File,
  })[fileKind(name)];
const remaining = (task: Task) => {
  if (!task.speed || task.size <= task.completed) return "";
  const seconds = Math.ceil((task.size - task.completed) / task.speed);
  return seconds < 60
    ? `剩余 ${seconds} 秒`
    : seconds < 3600
      ? `剩余 ${Math.ceil(seconds / 60)} 分钟`
      : `剩余 ${(seconds / 3600).toFixed(1)} 小时`;
};
function notice(message: string) {
  toast.value = message;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.value = ""), 4500);
}
async function refresh() {
  if (loadingSnapshot) return;
  loadingSnapshot = true;
  try {
    state.value = await invoke<Snapshot>("snapshot");
    if (selected.value.length) {
      const ids = new Set(state.value.tasks.map((t) => t.id));
      selected.value = selected.value.filter((id) => ids.has(id));
    }
    if (expanded.value.length) {
      const groups = new Set(
        state.value.tasks.map((t) => t.batch?.id).filter(Boolean),
      );
      expanded.value = expanded.value.filter((id) => groups.has(id));
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    loadingSnapshot = false;
  }
}
async function login(provider: Provider) {
  try {
    await invoke("open_login", { provider });
    if (!signingIn.value.includes(provider)) signingIn.value.push(provider);
    notice(`请在打开的${providerName(provider)}窗口完成登录`);
  } catch (e) {
    error.value = String(e);
  }
}
async function checkAccount(provider: Provider, silent = false) {
  if (refreshing.value.includes(provider)) return;
  refreshing.value.push(provider);
  try {
    const account = await invoke<Account>("refresh_account", { provider });
    const i = state.value.accounts.findIndex((a) => a.provider === provider);
    if (i >= 0) state.value.accounts[i] = account;
    else state.value.accounts.push(account);
    if (account.loggedIn) {
      const wasSigningIn = signingIn.value.includes(provider);
      if (wasSigningIn || !silent) notice(`${providerName(provider)}已连接`);
      signingIn.value = signingIn.value.filter((p) => p !== provider);
      // A drive the user just signed into becomes the active drive tab.
      if (wasSigningIn) driveProvider.value = provider;
    } else if (!silent) notice(account.message);
  } catch (e) {
    if (!silent) error.value = String(e);
  } finally {
    refreshing.value = refreshing.value.filter((p) => p !== provider);
  }
}
function openDrive(provider: Provider) {
  driveProvider.value = provider;
  tab.value = "drive";
}
function dialogLogin(provider: Provider) {
  addDialog.value = "";
  void login(provider);
}
function dialogSelect(provider: Provider) {
  addDialog.value = "";
  openDrive(provider);
}
function confirmLogout(provider: Provider) {
  logoutError.value = "";
  logoutTarget.value = provider;
}
async function logout() {
  const provider = logoutTarget.value;
  if (!provider || loggingOut.value) return;
  loggingOut.value = true;
  logoutError.value = "";
  try {
    const account = await invoke<Account>("logout_account", { provider });
    const i = state.value.accounts.findIndex((a) => a.provider === provider);
    if (i >= 0) state.value.accounts[i] = account;
    else state.value.accounts.push(account);
    signingIn.value = signingIn.value.filter((p) => p !== provider);
    logoutTarget.value = null;
    notice(`${providerName(provider)}已退出登录`);
  } catch (e) {
    logoutError.value = String(e);
  } finally {
    loggingOut.value = false;
  }
}
// Escape and focus handling for the logout dialog: keydown on the section only
// fires when focus is inside it, so listen on the document while it is open.
const logoutCancel = ref<HTMLButtonElement | null>(null);
let logoutPreviousFocus: HTMLElement | null = null;
function onLogoutKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && !loggingOut.value) logoutTarget.value = null;
}
watch(logoutTarget, (target) => {
  if (target) {
    logoutPreviousFocus = document.activeElement as HTMLElement | null;
    document.addEventListener("keydown", onLogoutKeydown);
    void nextTick(() => logoutCancel.value?.focus());
  } else {
    document.removeEventListener("keydown", onLogoutKeydown);
    logoutPreviousFocus?.focus();
    logoutPreviousFocus = null;
  }
});
// Keep the drive switcher pointing at a connected provider: when the selected
// one is logged out, fall back to the first connected account.
watch(connectedProviders, (connected) => {
  if (connected.length && !accountFor(driveProvider.value).loggedIn)
    driveProvider.value = connected[0].id;
});
// Switching tabs changes which tasks are on screen, so a selection made in one
// tab must not leak into another (it would show "已选中 N 项" with an empty list).
watch(tab, () => {
  selected.value = [];
});
// Closing the login window cancels the pending login: run one final check (the
// user may have signed in right before closing), then stop showing "waiting".
async function cancelLogin(provider: Provider) {
  await checkAccount(provider, true);
  if (!accountFor(provider).loggedIn)
    signingIn.value = signingIn.value.filter((p) => p !== provider);
}
async function chooseDirectory() {
  const path = await open({
    directory: true,
    multiple: false,
    defaultPath: state.value.downloadDir,
    title: "选择下载文件夹",
  });
  if (typeof path === "string") {
    await invoke("set_directory", { path });
    await refresh();
  }
}
async function downloadsCreated(
  tasks: { status: string; error: string; batch?: { id: string } | null }[],
) {
  const created = tasks.filter((t) => t.status !== "error");
  const folders = new Set(
    created.filter((t) => t.batch).map((t) => t.batch!.id),
  );
  const units = folders.size + created.filter((t) => !t.batch).length;
  if (units)
    notice(
      folders.size && !created.some((t) => !t.batch)
        ? `已添加 ${folders.size} 个文件夹下载任务`
        : `已添加 ${units} 个下载任务`,
    );
  await refresh();
}
function settingsSaved(settings: AppSettings) {
  state.value.settings = settings;
  state.value.downloadDir = settings.downloadDir;
}
async function action(task: Task, action: string) {
  pendingAction.value = task.id;
  error.value = "";
  try {
    await invoke("task_action", { id: task.id, action });
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    pendingAction.value = null;
  }
}
const actionLabel = (name: string) =>
  name === "pause" ? "暂停" : name === "resume" ? "继续" : "移除";
async function runBatch(ids: string[], action: string) {
  if (batchBusy.value || !ids.length) return;
  batchBusy.value = true;
  error.value = "";
  try {
    const report = await invoke<{
      done: number;
      failed: { id: string; message: string }[];
    }>("task_batch_action", { ids, action });
    const skipped = ids.length - report.done - report.failed.length;
    if (report.done)
      notice(
        skipped && action !== "remove"
          ? `已${actionLabel(action)} ${report.done} 个任务，${skipped} 个任务当前无需${actionLabel(action)}`
          : `已${actionLabel(action)} ${report.done} 个任务`,
      );
    else if (!report.failed.length)
      notice(`所选任务当前无需${actionLabel(action)}`);
    if (report.failed.length) error.value = report.failed[0].message;
    selected.value = [];
  } catch (e) {
    error.value = String(e);
  } finally {
    batchBusy.value = false;
    await refresh();
  }
}
async function clearCompleted() {
  await runBatch(
    completed.value.map((t) => t.id),
    "remove",
  );
}
function confirmRemove(task: Task) {
  deleteLocalFiles.value = false;
  removeError.value = "";
  removeTarget.value = {
    ids: [task.id],
    name: task.name,
    directory: task.directory,
  };
}
function confirmRemoveGroup(group: TaskGroup) {
  deleteLocalFiles.value = false;
  removeError.value = "";
  removeTarget.value = {
    ids: group.tasks.map((t) => t.id),
    name: `文件夹「${group.name}」的 ${group.total} 个文件`,
    directory: group.tasks[0]?.directory || "",
  };
}
function confirmRemoveSelection() {
  if (!selected.value.length) return;
  deleteLocalFiles.value = false;
  removeError.value = "";
  const first = tasks.value.find((t) => selectedSet.value.has(t.id));
  removeTarget.value = {
    ids: [...selected.value],
    name: `${selected.value.length} 个下载任务`,
    directory: first?.directory || "",
  };
}
async function remove() {
  const target = removeTarget.value;
  if (!target || removing.value) return;
  removing.value = true;
  removeError.value = "";
  try {
    if (target.ids.length === 1) {
      await invoke("task_action", {
        id: target.ids[0],
        action: "remove",
        deleteFiles: deleteLocalFiles.value,
      });
    } else {
      const report = await invoke<{
        done: number;
        failed: { id: string; message: string }[];
      }>("task_batch_action", {
        ids: target.ids,
        action: "remove",
        deleteFiles: deleteLocalFiles.value,
      });
      if (report.failed.length) {
        removeError.value = `${report.failed.length} 个任务移除失败：${report.failed[0].message}`;
        return;
      }
    }
    removeTarget.value = null;
    selected.value = [];
    notice(
      deleteLocalFiles.value
        ? target.ids.length > 1
          ? `${target.ids.length} 个任务及本地文件已删除`
          : "任务及本地文件已删除"
        : target.ids.length > 1
          ? `已移除 ${target.ids.length} 个任务，本地文件已保留`
          : "任务已移除，本地文件已保留",
    );
  } catch (e) {
    removeError.value = String(e);
  } finally {
    removing.value = false;
    await refresh();
  }
}
async function folder(id?: string) {
  try {
    await invoke("open_directory", { id: id || null });
  } catch (e) {
    error.value = String(e);
  }
}
/**
 * 浏览器通过 clouddownloader:// 交过来的链接。后端先入队再发事件，所以这里只
 * 负责取走队列：挂载时和每次事件各取一次，避免冷启动/重载时漏掉或重复处理。
 */
async function receiveHandoff() {
  const links = (await invoke<Handoff[] | null>("take_handoff")) ?? [];
  if (!links.length) return;
  tab.value = "all";
  await nextTick();
  for (const link of links) await downloadBox.value?.receive(link);
}
onMounted(async () => {
  await refresh();
  unlisten = await listen("tasks-changed", refresh);
  unlistenLoginClosed = await listen<string>("login-window-closed", (event) => {
    void cancelLogin(event.payload as Provider);
  });
  unlistenHandoff = await listen("handoff", () => void receiveHandoff());
  await receiveHandoff();
  await Promise.all(providers.map((p) => checkAccount(p.id, true)));
  accountsReady.value = true;
  if (state.value.settings.autoCheckUpdates)
    void checkForUpdates({ silent: true });
  interval = setInterval(() => {
    for (const provider of signingIn.value) void checkAccount(provider, true);
  }, 4000);
});
onUnmounted(() => {
  clearInterval(interval);
  clearTimeout(toastTimer);
  unlisten?.();
  unlistenLoginClosed?.();
  unlistenHandoff?.();
  document.removeEventListener("keydown", onLogoutKeydown);
});
</script>

<template>
  <div class="app-shell">
    <TitleBar @error="(message) => (error = message)" />
    <aside class="sidebar">
      <div class="brand">
        <span class="brand-mark"
          ><ArrowDownToLine :size="24" :stroke-width="2.7"
        /></span>
        <div>CloudDownloader<small>让下载，简单一点</small></div>
      </div>
      <button
        class="new-button"
        @click="
          tab = 'all';
          $nextTick(() => downloadBox?.focus());
        "
      >
        <Plus :size="18" />新建下载
      </button>
      <div class="nav-caption">下载管理</div>
      <nav>
        <button :class="{ selected: tab === 'all' }" @click="tab = 'all'">
          <CloudDownload :size="19" />全部任务<span>{{ unitCounts.all }}</span>
        </button>
        <button :class="{ selected: tab === 'active' }" @click="tab = 'active'">
          <ArrowDown :size="19" />正在下载<span v-if="unitCounts.active">{{
            unitCounts.active
          }}</span>
        </button>
        <button
          :class="{ selected: tab === 'complete' }"
          @click="tab = 'complete'"
        >
          <CircleCheck :size="19" />已完成<span v-if="unitCounts.complete">{{
            unitCounts.complete
          }}</span>
        </button>
        <button :class="{ selected: tab === 'error' }" @click="tab = 'error'">
          <CircleAlert :size="19" />需要处理<span
            v-if="unitCounts.error"
            class="error-count"
            >{{ unitCounts.error }}</span
          >
        </button>
      </nav>
      <div class="nav-caption second-caption">云端空间</div>
      <nav>
        <button :class="{ selected: tab === 'drive' }" @click="tab = 'drive'">
          <Folder :size="19" />我的网盘
        </button>
        <button
          :class="{ selected: tab === 'uploads' }"
          @click="tab = 'uploads'"
        >
          <ArrowUpFromLine :size="19" />上传任务<span
            v-if="state.uploads.length"
            >{{ state.uploads.length }}</span
          >
        </button>
        <button :class="{ selected: tab === 'shares' }" @click="tab = 'shares'">
          <Share2 :size="19" />分享记录
        </button>
        <button
          :class="{ selected: tab === 'account' }"
          @click="tab = 'account'"
        >
          <UserRound :size="19" />账号管理
        </button>
        <button
          :class="{ selected: tab === 'settings' }"
          @click="tab = 'settings'"
        >
          <Settings :size="19" />设置<i
            v-if="updateStatus.phase === 'available'"
            class="nav-dot"
            title="有新版本可用"
          ></i>
        </button>
      </nav>
      <div class="sidebar-bottom">
        <div class="account-mini">
          <div class="avatar"><UserRound :size="20" /></div>
          <div>
            <strong>{{ connectedCount }} 个网盘已连接</strong
            ><small>{{ "管理你的网盘账号" }}</small>
          </div>
          <button title="管理网盘账号" @click="tab = 'account'">
            <ArrowUpRight :size="18" />
          </button>
        </div>
        <div class="engine-state">
          <i :class="{ ready: state.engineRunning }"></i
          >{{ state.engineRunning ? "下载引擎已就绪" : "下载引擎未就绪"
          }}<span>v{{ version }}</span>
        </div>
      </div>
    </aside>
    <main
      class="main-content"
      :class="{
        'drive-workspace': tab === 'drive',
        'task-workspace': ['all', 'active', 'complete', 'error'].includes(tab),
      }"
    >
      <header class="page-header">
        <div class="page-heading-copy">
          <div class="page-title-row">
            <h1>{{ title }}</h1>
            <span
              v-if="['all', 'active', 'complete', 'error'].includes(tab)"
              class="page-count"
              >{{ tasks.length }}</span
            >
          </div>
          <p>
            {{
              tab === "drive"
                ? driveSummary(driveProvider)
                : tab === "uploads"
                  ? "分片并发传输，随时掌握上传进度。"
                  : tab === "shares"
                    ? "管理通过本程序创建的百度分享链接。"
                    : tab === "account"
                      ? "连接你的网盘，文件触手可及。"
                      : tab === "settings"
                        ? "偏好设置与版本更新。"
                        : "所有下载，一个地方。"
            }}
          </p>
        </div>
        <button
          class="connection-badge"
          :class="{ connected: connectedCount > 0 }"
          title="管理网盘账号"
          @click="tab = 'account'"
        >
          <i></i
          >{{ connectedCount ? `${connectedCount} 个网盘已连接` : "尚未登录"
          }}<ChevronRight :size="13" />
        </button>
      </header>
      <div v-if="error || state.engineError" class="error-banner" role="alert">
        <CircleAlert :size="18" /><span>{{ error || state.engineError }}</span
        ><button aria-label="关闭错误提示" @click="error = ''">
          <X :size="16" />
        </button>
      </div>
      <template v-if="tab === 'drive'">
        <div v-if="!accountsReady" class="drive-placeholder card">
          <LoaderCircle :size="22" class="spinning" />
          <span>正在检查各网盘的登录状态…</span>
        </div>
        <div v-else-if="!driveTabs.length" class="drive-placeholder card">
          <span class="empty-illustration"
            ><FolderOpen :size="38" :stroke-width="1.4" /><span
              ><Plus :size="14" /></span
          ></span>
          <h3>还没有可用的网盘</h3>
          <p>连接一个网盘账号后，就能在这里浏览和管理文件。</p>
          <button class="primary" @click="addDialog = 'drive'">
            <Plus :size="16" />添加网盘账号
          </button>
        </div>
        <template v-else>
          <div class="drive-providers" role="group" aria-label="选择个人网盘">
            <button
              v-for="provider in driveTabs"
              :key="provider.id"
              :class="{ selected: driveProvider === provider.id }"
              :aria-pressed="driveProvider === provider.id"
              :disabled="driveBusy"
              @click="driveProvider = provider.id"
            >
              <span
                class="drive-provider-avatar"
                :style="{ background: provider.color }"
                >{{ provider.initial }}</span
              >{{ provider.name }}
            </button>
            <button
              class="drive-provider-add"
              title="添加网盘账号"
              :disabled="driveBusy"
              @click="addDialog = 'drive'"
            >
              <Plus :size="14" />添加网盘
            </button>
          </div>
          <DriveExplorer
            :key="driveProvider"
            :provider="driveProvider"
            :connected="accountFor(driveProvider).loggedIn"
            :download-dir="state.downloadDir"
            @login="login(driveProvider)"
            @notice="notice"
            @error="error = $event"
            @changed="refresh"
            @directory="chooseDirectory"
            @busy="driveBusy = $event"
            @downloaded="
              tab = 'all';
              refresh();
            "
            @uploaded="
              tab = 'uploads';
              refresh();
            "
          />
        </template>
      </template>
      <UploadPanel
        v-else-if="tab === 'uploads'"
        :tasks="state.uploads"
        @changed="refresh"
        @error="error = $event"
        @notice="notice"
        @browse="tab = 'drive'"
      />
      <SharePanel
        v-else-if="tab === 'shares'"
        :shares="state.shares"
        @changed="refresh"
        @error="error = $event"
        @notice="notice"
        @browse="tab = 'drive'"
      />
      <template v-else-if="tab === 'account'">
        <div v-if="!accountsReady" class="account-checking card">
          <LoaderCircle :size="22" class="spinning" />
          <span>正在检查各网盘的登录状态…</span>
        </div>
        <div v-else-if="!connectedProviders.length" class="account-empty card">
          <span class="empty-illustration"
            ><UserRound :size="38" :stroke-width="1.4" /><span
              ><Plus :size="14" /></span
          ></span>
          <h3>还没有连接网盘账号</h3>
          <p>连接后就能浏览网盘文件，把喜欢的内容下载到本地。</p>
          <button class="primary" @click="addDialog = 'account'">
            <Plus :size="16" />添加网盘账号
          </button>
        </div>
        <template v-else>
          <div class="account-list card">
            <div class="account-list-caption">
              <span>已连接的网盘</span>
              <span class="page-count">{{ connectedProviders.length }}</span>
            </div>
            <article
              v-for="provider in connectedProviders"
              :key="provider.id"
              class="account-row"
            >
              <span
                class="provider-avatar"
                :style="{ background: provider.color }"
                >{{ provider.initial }}</span
              >
              <div class="account-row-copy">
                <div class="account-row-title">
                  <strong>{{ provider.name }}</strong>
                  <span
                    class="account-state"
                    :class="{
                      warn: accountFor(provider.id).message !== '账号已连接',
                    }"
                    :title="accountFor(provider.id).message"
                    ><i></i>已连接</span
                  >
                </div>
                <p :title="accountFor(provider.id).name">
                  {{ accountFor(provider.id).name }}
                </p>
              </div>
              <div class="account-row-actions">
                <button class="secondary" @click="openDrive(provider.id)">
                  <FolderOpen :size="15" />进入网盘
                </button>
                <button
                  class="icon-button"
                  title="刷新登录状态"
                  :disabled="refreshing.includes(provider.id)"
                  @click="checkAccount(provider.id)"
                >
                  <RefreshCw
                    :size="16"
                    :class="{ spinning: refreshing.includes(provider.id) }"
                  />
                </button>
                <button
                  class="icon-button logout"
                  title="退出登录"
                  @click="confirmLogout(provider.id)"
                >
                  <LogOut :size="16" />
                </button>
              </div>
            </article>
          </div>
          <button class="account-add-row" @click="addDialog = 'account'">
            <span class="account-add-icon"><Plus :size="17" /></span>
            <span class="account-add-copy">
              <strong>添加网盘账号</strong>
              <small
                >支持夸克、百度、115 等 {{ providers.length }} 个网盘</small
              >
            </span>
            <ChevronRight :size="16" />
          </button>
        </template>
        <div class="privacy-note">
          <ShieldCheck
            :size="17"
          />各网盘的登录状态独立保存在这台电脑上，登录过期时重新连接即可。
        </div>
      </template>
      <SettingsPanel
        v-else-if="tab === 'settings'"
        :settings="state.settings"
        @saved="settingsSaved"
        @directory="chooseDirectory"
        @folder="folder()"
        @error="error = $event"
      />
      <template v-else>
        <DownloadBox
          v-show="tab === 'all'"
          ref="downloadBox"
          :accounts="state.accounts"
          :download-dir="state.downloadDir"
          :default-connections="state.settings.defaultConnections"
          @login="login"
          @created="downloadsCreated"
          @directory="chooseDirectory"
          @folder="folder()"
        />
        <div class="task-heading">
          <div>
            <h2>传输任务</h2>
            <span>{{ topLevelCount }} 项</span>
          </div>
          <div class="speed-label" v-if="speed">
            <ArrowDown :size="14" />{{ bytes(speed) }}/s
          </div>
          <div class="task-search">
            <Search :size="15" /><input
              v-model="filter"
              placeholder="搜索任务"
              aria-label="搜索下载任务"
            />
          </div>
        </div>
        <section class="task-list card">
          <div
            v-if="rows.length"
            class="task-list-head"
            :class="{ 'head-selected': selected.length }"
          >
            <label
              class="task-select-all"
              :title="allSelected ? '取消全选' : '全选'"
            >
              <input
                type="checkbox"
                :checked="allSelected"
                :indeterminate="someSelected"
                :disabled="batchBusy"
                @change="toggleSelectAll"
              />
            </label>
            <template v-if="selected.length">
              <span class="batch-count"
                >已选中 <b>{{ selected.length }}</b> 项</span
              >
              <button
                class="text-button batch-clear"
                :disabled="batchBusy"
                @click="selected = []"
              >
                取消选中
              </button>
              <div class="batch-actions">
                <button
                  class="batch-btn"
                  :disabled="batchBusy"
                  @click="runBatch(selected, 'pause')"
                >
                  <Pause :size="15" />暂停
                </button>
                <button
                  class="batch-btn"
                  :disabled="batchBusy"
                  @click="runBatch(selected, 'resume')"
                >
                  <Play :size="15" />继续
                </button>
                <button
                  class="batch-btn danger"
                  :disabled="batchBusy"
                  @click="confirmRemoveSelection"
                >
                  <Trash2 :size="15" />移除
                </button>
                <LoaderCircle v-if="batchBusy" :size="15" class="spinning" />
              </div>
            </template>
            <span v-else class="head-label">任务</span>
            <span class="batch-spacer" />
            <button
              v-if="completed.length && tab === 'complete'"
              class="text-button task-clear"
              :disabled="batchBusy"
              @click="clearCompleted"
            >
              清除已完成
            </button>
          </div>
          <div class="task-scroll">
            <div v-if="!tasks.length" class="empty-state">
              <span class="empty-illustration"
                ><CloudDownload :size="44" :stroke-width="1.3" /><span
                  ><Plus :size="14" /></span
              ></span>
              <h3>
                {{
                  filter
                    ? "没有找到匹配的任务"
                    : tab === "complete"
                      ? "还没有已完成的下载"
                      : tab === "error"
                        ? "目前没有需要处理的任务"
                        : tab === "active"
                          ? "目前没有正在下载的任务"
                          : "还没有下载任务"
                }}
              </h3>
              <p>
                {{
                  filter
                    ? "试试其他文件名"
                    : "粘贴文件直链、网盘分享或磁力链接，也可导入种子。"
                }}
              </p>
              <div class="empty-steps">
                <span><Link :size="14" />粘贴链接</span
                ><ChevronRight :size="12" /><span
                  ><CheckCheck :size="14" />选择文件</span
                ><ChevronRight :size="12" /><span
                  ><ArrowDownToLine :size="14" />下载到本地</span
                >
              </div>
            </div>
            <template v-for="row in rows" :key="row.key">
              <article
                v-if="row.kind === 'group'"
                class="task-row task-group-row"
                :class="row.group.status"
              >
                <label
                  class="task-select"
                  :title="
                    groupSelected(row.group) === 'all'
                      ? '取消选择文件夹'
                      : '选择文件夹'
                  "
                >
                  <input
                    type="checkbox"
                    :checked="groupSelected(row.group) === 'all'"
                    :indeterminate="groupSelected(row.group) === 'some'"
                    @change="toggleGroupSelection(row.group)"
                  />
                </label>
                <button
                  class="task-expand"
                  :title="
                    expanded.includes(row.group.id)
                      ? '收起文件列表'
                      : '展开文件列表'
                  "
                  :aria-expanded="expanded.includes(row.group.id)"
                  @click="toggleGroup(row.group.id)"
                >
                  <ChevronRight
                    :size="15"
                    :class="{ open: expanded.includes(row.group.id) }"
                  />
                </button>
                <div class="task-file-icon folder">
                  <Folder :size="24" />
                </div>
                <div class="task-details">
                  <div class="task-name-row">
                    <strong :title="row.group.path || row.group.name">{{
                      row.group.name
                    }}</strong
                    ><span class="status-pill" :class="row.group.status"
                      ><i></i>{{ statusName(row.group.status) }}</span
                    >
                  </div>
                  <div
                    class="task-progress"
                    :class="[
                      row.group.status,
                      {
                        indeterminate:
                          !row.group.size && row.group.status === 'active',
                      },
                    ]"
                  >
                    <div :style="{ width: `${row.group.progress}%` }"></div>
                  </div>
                  <div class="task-meta">
                    <span class="provider-tag"
                      >文件夹 · {{ row.group.total }} 个文件</span
                    >
                    <span
                      >{{ bytes(row.group.completed) }} /
                      {{
                        row.group.size ? bytes(row.group.size) : "未知大小"
                      }}</span
                    >
                    <span
                      >{{ row.group.done }} / {{ row.group.total }} 已完成</span
                    >
                  </div>
                </div>
                <div class="task-rate" v-if="row.group.speed">
                  <strong>{{ bytes(row.group.speed) }}<small>/s</small></strong>
                  <span>文件夹总速度</span>
                </div>
                <div class="task-actions">
                  <LoaderCircle
                    v-if="batchBusy"
                    :size="19"
                    class="spinning"
                  /><template v-else
                    ><button
                      v-if="groupPausable(row.group).length"
                      class="icon-button task-main-action"
                      title="暂停文件夹下载"
                      aria-label="暂停文件夹下载"
                      @click="runBatch(groupPausable(row.group), 'pause')"
                    >
                      <Pause :size="17" /></button
                    ><button
                      v-if="groupResumable(row.group).length"
                      class="icon-button task-main-action"
                      title="继续文件夹下载"
                      aria-label="继续文件夹下载"
                      @click="runBatch(groupResumable(row.group), 'resume')"
                    >
                      <Play :size="17" /></button
                    ><button
                      class="icon-button"
                      title="打开下载文件夹"
                      aria-label="打开下载文件夹"
                      @click="folder(row.group.tasks[0]?.id)"
                    >
                      <FolderOpen :size="17" /></button
                    ><button
                      class="icon-button"
                      title="移除文件夹下载任务"
                      aria-label="移除文件夹下载任务"
                      :disabled="row.group.status === 'resolving'"
                      @click="confirmRemoveGroup(row.group)"
                    >
                      <Trash2 :size="17" /></button
                  ></template>
                </div>
              </article>
              <article
                v-else
                class="task-row"
                :class="[row.task.status, { 'task-child-row': row.child }]"
              >
                <label
                  class="task-select"
                  :title="
                    selected.includes(row.task.id) ? '取消选择任务' : '选择任务'
                  "
                >
                  <input
                    type="checkbox"
                    :checked="selected.includes(row.task.id)"
                    @change="toggleTask(row.task.id)"
                  />
                </label>
                <div class="task-file-icon" :class="fileKind(row.task.name)">
                  <component :is="fileIcon(row.task.name)" :size="25" />
                </div>
                <div class="task-details">
                  <div class="task-name-row">
                    <strong :title="row.task.name">{{ row.task.name }}</strong
                    ><span class="status-pill" :class="row.task.status"
                      ><i></i>{{ statusName(row.task.status) }}</span
                    >
                  </div>
                  <div
                    class="task-progress"
                    :class="[
                      row.task.status,
                      {
                        indeterminate:
                          !row.task.size && row.task.status === 'active',
                      },
                    ]"
                  >
                    <div :style="{ width: `${progress(row.task)}%` }"></div>
                  </div>
                  <div class="task-meta">
                    <span class="provider-tag">{{
                      row.task.bt
                        ? "BT 下载"
                        : row.task.direct
                          ? "普通链接"
                          : providerName(row.task.share.provider) +
                            (row.task.own ? " · 我的网盘" : "")
                    }}</span>
                    <span
                      >{{ bytes(row.task.completed) }} /
                      {{
                        row.task.size || row.task.status === "complete"
                          ? bytes(row.task.size)
                          : "未知大小"
                      }}</span
                    ><span v-if="row.task.status === 'complete'"
                      ><Check :size="12" />已保存到本地</span
                    ><span v-else-if="row.task.size"
                      >{{ progress(row.task).toFixed(1) }}%</span
                    >
                  </div>
                  <p v-if="row.task.bt" class="direct-task-detail">
                    已选 {{ row.task.bt.selected.length }} 个文件 ·
                    {{ row.task.connections || 0 }} 个节点 / 最多
                    {{ row.task.bt.maxPeers }} · 上传
                    {{ bytes(row.task.uploadSpeed || 0) }}/s
                  </p>
                  <p v-if="row.task.direct" class="direct-task-detail">
                    {{
                      row.task.direct.rangeSupported
                        ? row.task.status === "active"
                          ? `${row.task.connections || 0} 路连接 · 最多 ${row.task.direct.connections} 路`
                          : `支持分段 · 最多 ${row.task.direct.connections} 路连接`
                        : "单连接 · 服务器不支持分段，恢复时从头下载"
                    }}
                  </p>
                  <p v-if="row.task.error" class="task-error">
                    {{ row.task.error }}
                  </p>
                </div>
                <div
                  class="task-rate"
                  v-if="row.task.speed || row.task.status === 'seeding'"
                >
                  <strong
                    >{{
                      bytes(
                        row.task.status === "seeding"
                          ? row.task.uploadSpeed || 0
                          : row.task.speed,
                      )
                    }}<small>/s</small></strong
                  >
                  <span>{{
                    row.task.status === "seeding"
                      ? "正在做种"
                      : remaining(row.task) || "正在传输"
                  }}</span>
                </div>
                <div class="task-actions">
                  <LoaderCircle
                    v-if="pendingAction === row.task.id"
                    :size="19"
                    class="spinning"
                  /><template v-else
                    ><button
                      v-if="
                        ['active', 'waiting', 'seeding'].includes(
                          row.task.status,
                        )
                      "
                      class="icon-button task-main-action"
                      title="暂停下载"
                      aria-label="暂停下载"
                      @click="action(row.task, 'pause')"
                    >
                      <Pause :size="17" /></button
                    ><button
                      v-if="
                        ['paused', 'error', 'cancelled'].includes(
                          row.task.status,
                        )
                      "
                      class="icon-button task-main-action"
                      :title="
                        row.task.status === 'error'
                          ? row.task.direct
                            ? '重试下载'
                            : '重新解析并重试'
                          : '继续下载'
                      "
                      aria-label="继续下载"
                      @click="action(row.task, 'resume')"
                    >
                      <Play :size="17" /></button
                    ><button
                      class="icon-button"
                      title="打开下载文件夹"
                      aria-label="打开下载文件夹"
                      @click="folder(row.task.id)"
                    >
                      <FolderOpen :size="17" /></button
                    ><button
                      class="icon-button"
                      title="删除下载任务"
                      aria-label="删除下载任务"
                      :disabled="row.task.status === 'resolving'"
                      @click="confirmRemove(row.task)"
                    >
                      <Trash2 :size="17" /></button
                  ></template>
                </div>
              </article>
            </template>
          </div>
        </section>
        <footer class="transfer-footer">
          <span class="transfer-rate"
            ><ArrowDown :size="14" />{{ bytes(speed) }}/s</span
          >
          <span class="transfer-rate"
            ><ArrowUpFromLine :size="14" />{{ bytes(uploadSpeed) }}/s</span
          >
          <span class="tray-hint">关闭窗口后在托盘继续下载</span>
          <span class="footer-engine"
            ><i :class="{ ready: state.engineRunning }" />{{
              state.engineRunning ? "引擎已就绪" : "引擎未就绪"
            }}</span
          >
        </footer>
      </template>
    </main>
    <Transition name="toast"
      ><div v-if="toast" class="toast-message" role="status">
        <CircleCheck :size="18" />{{ toast }}
      </div></Transition
    >
    <div
      v-if="removeTarget"
      class="modal-backdrop"
      @click.self="!removing && (removeTarget = null)"
    >
      <section
        class="confirm-dialog remove-task-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="remove-title"
      >
        <span class="dialog-icon"><Trash2 :size="24" /></span>
        <h2 id="remove-title">
          {{
            removeTarget.ids.length > 1
              ? "移除所选下载任务？"
              : "移除这个下载任务？"
          }}
        </h2>
        <p class="remove-task-name">{{ removeTarget.name }}</p>
        <p>
          {{
            removeTarget.ids.length > 1
              ? `共 ${removeTarget.ids.length} 个任务会停止，并从列表中移除。`
              : "任务会停止，并从列表中移除。"
          }}
        </p>
        <label class="remove-files-option">
          <input
            v-model="deleteLocalFiles"
            type="checkbox"
            :disabled="removing"
          />
          <span>同时删除本地文件</span>
        </label>
        <p class="remove-files-note" :class="{ destructive: deleteLocalFiles }">
          {{
            deleteLocalFiles
              ? "将永久删除已下载或未完成的文件及续传信息，无法恢复。"
              : "未勾选时，已下载的文件和续传信息会保留。"
          }}
        </p>
        <p
          v-if="removeTarget.directory"
          class="remove-file-path"
          :title="removeTarget.directory"
        >
          保存位置：{{ removeTarget.directory }}
        </p>
        <p v-if="removeError" class="remove-task-error" role="alert">
          {{ removeError }}
        </p>
        <div class="button-row">
          <button
            class="secondary"
            :disabled="removing"
            @click="removeTarget = null"
          >
            保留任务</button
          ><button class="danger" :disabled="removing" @click="remove">
            <LoaderCircle v-if="removing" :size="15" class="spinning" />
            {{
              removing
                ? "正在删除…"
                : deleteLocalFiles
                  ? "删除任务及文件"
                  : "仅移除任务"
            }}
          </button>
        </div>
      </section>
    </div>
    <AccountDialog
      v-if="addDialog"
      :accounts="state.accounts"
      :signing-in="signingIn"
      @login="dialogLogin"
      @select="dialogSelect"
      @close="addDialog = ''"
    />
    <div
      v-if="logoutTarget"
      class="modal-backdrop"
      @click.self="!loggingOut && (logoutTarget = null)"
    >
      <section
        class="confirm-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="logout-title"
      >
        <span class="dialog-icon"><LogOut :size="24" /></span>
        <h2 id="logout-title">退出登录{{ providerName(logoutTarget) }}？</h2>
        <p>
          将清除这台电脑上保存的{{
            providerName(logoutTarget)
          }}登录信息，已有下载任务不受影响，重新连接后可继续使用。
        </p>
        <p v-if="logoutError" class="remove-task-error" role="alert">
          {{ logoutError }}
        </p>
        <div class="button-row">
          <button
            ref="logoutCancel"
            class="secondary"
            :disabled="loggingOut"
            @click="logoutTarget = null"
          >
            取消</button
          ><button class="danger" :disabled="loggingOut" @click="logout">
            <LoaderCircle v-if="loggingOut" :size="15" class="spinning" />
            {{ loggingOut ? "正在退出…" : "退出登录" }}
          </button>
        </div>
      </section>
    </div>
  </div>
</template>
