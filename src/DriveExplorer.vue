<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowDown,
  ArrowDownToLine,
  ArrowDownNarrowWide,
  ArrowUp,
  Check,
  ChevronRight,
  ChevronsUpDown,
  CloudUpload,
  Copy,
  File,
  Filter,
  Folder,
  FolderPlus,
  FolderUp,
  Home,
  LayoutGrid,
  List,
  LoaderCircle,
  MoreHorizontal,
  Pencil,
  RefreshCw,
  Search,
  Share2,
  Trash2,
  UserRound,
  X,
} from "@lucide/vue";
import {
  type CreatedShare,
  type DriveFile,
  formatBytes,
  formatDate,
  formatSharePeriod,
  shareText,
} from "./cloud";
import { type Provider, canDrive, providerName } from "./providers";

const props = defineProps<{
  provider: Provider;
  downloadDir: string;
  connected: boolean;
}>();
const emit = defineEmits<{
  notice: [message: string];
  error: [message: string];
  login: [];
  downloaded: [];
  uploaded: [];
  changed: [];
  directory: [];
  busy: [value: boolean];
}>();

type Trail = { name: string; path: string };
type Listing = {
  accountId: string;
  path: string;
  files: DriveFile[];
  hasMore: boolean;
  nextCursor: string | null;
};

const listing = ref<Listing | null>(null);
const trail = ref<Trail[]>([{ name: "全部文件", path: "/" }]);
type CrumbItem =
  | { kind: "crumb"; index: number; name: string; path: string }
  | { kind: "gap"; names: string[] };
const displayTrail = computed<CrumbItem[]>(() => {
  const items = trail.value;
  if (items.length <= 4)
    return items.map((crumb, index) => ({
      kind: "crumb" as const,
      index,
      name: crumb.name,
      path: crumb.path,
    }));
  const hidden = items.slice(1, items.length - 2).map((crumb) => crumb.name);
  return [
    {
      kind: "crumb" as const,
      index: 0,
      name: items[0]!.name,
      path: items[0]!.path,
    },
    { kind: "gap" as const, names: hidden },
    ...items.slice(items.length - 2).map((crumb, offset) => ({
      kind: "crumb" as const,
      index: items.length - 2 + offset,
      name: crumb.name,
      path: crumb.path,
    })),
  ];
});
const path = ref("/");

// 按网盘记住上次浏览的目录：切到其它标签页（如“上传任务”）或切换网盘再回来时恢复，
// 而不是每次都退回根目录。用挂在 window 上的 Map 做模块级单例，组件销毁后仍保留。
type ExplorerMemo = { path: string; trail: Trail[] };
const explorerMemo: Map<string, ExplorerMemo> = ((
  window as unknown as { __cdExplorerMemo?: Map<string, ExplorerMemo> }
).__cdExplorerMemo ??= new Map<string, ExplorerMemo>());
const savedMemo = explorerMemo.get(props.provider);
if (savedMemo) {
  path.value = savedMemo.path;
  trail.value = savedMemo.trail.map((crumb) => ({ ...crumb }));
}
const keyword = ref("");
const loadedKeyword = ref("");
const sortKey = ref<"name" | "size" | "modified" | "created">("name");
const sortAsc = ref(true);
const viewMode = ref<"list" | "grid">("list");
const filterOpen = ref(false);
const uploadMenuOpen = ref(false);
const sortOpen = ref(false);
const filterTypes = ref<string[]>([]);
const draftTypes = ref<string[]>([]);
const globalMode = ref(false);
const fullFiles = ref<DriveFile[] | null>(null);
const fullLoading = ref(false);
const fullTruncated = ref(false);
let fullKey = "";
// 非百度网盘没有服务端搜索接口：按下“搜索”时递归遍历当前目录及子目录，
// 结果单独保存，避免和“全库”加载状态混在一起。
const searching = ref(false);
const searchResults = ref<DriveFile[] | null>(null);
const searchTruncated = ref(false);
type FolderStep = {
  label: string;
  remote: string;
  total: number;
  done: number;
  percent: number;
  status: "waiting" | "creating" | "uploading" | "done" | "error";
};
const folderSteps = ref<FolderStep[] | null>(null);
const folderBusy = ref(false);
let folderTimer: number | undefined;
const selected = ref<string[]>([]);
const loading = ref(false);
const adding = ref(false);
const uploading = ref(false);
const choosingUpload = ref(false);
const error = ref("");
const page = ref(1);
const mode = ref<"" | "upload" | "share" | "mkdir" | "rename" | "delete">("");
const uploadPaths = ref<string[]>([]);
const uploadDirectory = ref("/");
const uploadPlan = ref<{ path: string; relative: string }[] | null>(null);
// 文件夹上传时所有文件共用一个归组 id，上传列表按文件夹合并显示。
const uploadBatch = ref<{ id: string; name: string } | null>(null);
const concurrency = ref(4);
const folderName = ref("");
const period = ref(7);
const password = ref("");
const created = ref<CreatedShare | null>(null);
const shareTargets = ref<DriveFile[]>([]);
const renameTarget = ref<DriveFile | null>(null);
const renameName = ref("");
const deleteTargets = ref<DriveFile[]>([]);
const modalError = ref("");
const modalBusy = ref(false);
const panel = ref<HTMLElement | null>(null);
const dropDirectory = ref<string | null>(null);
let unlistenDrop: UnlistenFn | undefined;
let disposed = false;
let revision = 0;

const isBaidu = computed(() => props.provider === "baidu");
const canUpload = computed(() => canDrive(props.provider, "upload"));
const canMkdir = computed(() => canDrive(props.provider, "mkdir"));
const canShare = computed(() => canDrive(props.provider, "share"));
const canRename = computed(() => canDrive(props.provider, "rename"));
const canDelete = computed(() => canDrive(props.provider, "delete"));
const busy = computed(
  () => loading.value || adding.value || uploading.value || folderBusy.value,
);
const canDrop = computed(
  () =>
    canUpload.value &&
    !!listing.value &&
    !busy.value &&
    !mode.value &&
    !choosingUpload.value,
);
const TYPE_EXT: Record<string, string[]> = {
  video: [
    "mp4",
    "mkv",
    "avi",
    "mov",
    "wmv",
    "flv",
    "webm",
    "ts",
    "m4v",
    "rmvb",
    "mpeg",
    "mpg",
    "3gp",
  ],
  audio: ["mp3", "flac", "wav", "aac", "ogg", "m4a", "wma", "ape", "opus"],
  image: [
    "jpg",
    "jpeg",
    "png",
    "gif",
    "bmp",
    "webp",
    "svg",
    "ico",
    "tif",
    "tiff",
    "heic",
    "raw",
  ],
  text: [
    "txt",
    "md",
    "log",
    "doc",
    "docx",
    "pdf",
    "xls",
    "xlsx",
    "ppt",
    "pptx",
    "csv",
    "rtf",
    "epub",
  ],
  font: ["ttf", "otf", "woff", "woff2"],
  subtitle: ["srt", "ass", "ssa", "vtt", "sub"],
  package: ["exe", "msi", "apk", "dmg", "pkg", "deb", "appx", "ipa"],
  archive: ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "cab"],
  torrent: ["torrent"],
};
const FILE_TYPES: { id: string; label: string }[] = [
  { id: "video", label: "视频" },
  { id: "audio", label: "音频" },
  { id: "image", label: "图片" },
  { id: "text", label: "文本" },
  { id: "font", label: "字体" },
  { id: "subtitle", label: "字幕" },
  { id: "package", label: "安装包" },
  { id: "archive", label: "压缩包" },
  { id: "torrent", label: "BT种子" },
  { id: "other", label: "其他" },
];
function fileType(file: DriveFile): string {
  if (file.isDir) return "folder";
  const dot = file.name.lastIndexOf(".");
  const ext = dot >= 0 ? file.name.slice(dot + 1).toLowerCase() : "";
  for (const [type, list] of Object.entries(TYPE_EXT)) {
    if (list.includes(ext)) return type;
  }
  return "other";
}
const visibleFiles = computed<DriveFile[]>(() => {
  const useFull =
    globalMode.value &&
    !!fullFiles.value &&
    !(isBaidu.value && keyword.value.trim());
  const files =
    searchResults.value ??
    (useFull ? fullFiles.value! : (listing.value?.files ?? []));
  const key = keyword.value.trim().toLowerCase();
  let result = isBaidu.value
    ? files.slice()
    : key
      ? files.filter((file) => file.name.toLowerCase().includes(key))
      : files.slice();
  if (filterTypes.value.length) {
    result = result.filter(
      (file) => !file.isDir && filterTypes.value.includes(fileType(file)),
    );
  }
  const dir = sortAsc.value ? 1 : -1;
  const k = sortKey.value;
  result.sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    if (k === "size") return (a.size - b.size) * dir;
    if (k === "modified") return (a.modified - b.modified) * dir;
    if (k === "created") return (a.created - b.created) * dir;
    return a.name.localeCompare(b.name, "zh-Hans-CN") * dir;
  });
  return result;
});
const sortLabels = {
  name: "名称",
  size: "大小",
  modified: "修改时间",
  created: "创建时间",
} as const;
const sortLabel = computed(
  () => `${sortLabels[sortKey.value]}·${sortAsc.value ? "升序" : "降序"}`,
);
function applySort(
  key: "name" | "size" | "modified" | "created",
  asc: boolean,
) {
  sortKey.value = key;
  sortAsc.value = asc;
  sortOpen.value = false;
  globalMode.value = true;
  void ensureFull();
}
function isSort(key: "name" | "size" | "modified" | "created", asc: boolean) {
  return sortKey.value === key && sortAsc.value === asc;
}
function sortHeader(key: "name" | "size" | "modified" | "created") {
  if (sortKey.value === key) applySort(key, !sortAsc.value);
  else applySort(key, key === "name");
}
const filterActive = computed(() => filterTypes.value.length > 0);
function openFilter() {
  draftTypes.value = [...filterTypes.value];
  filterOpen.value = true;
}
function toggleAllTypes() {
  draftTypes.value = [];
}
function toggleDraftType(id: string) {
  const index = draftTypes.value.indexOf(id);
  if (index >= 0) draftTypes.value.splice(index, 1);
  else draftTypes.value.push(id);
}
function applyFilter() {
  filterTypes.value = [...draftTypes.value];
  filterOpen.value = false;
  globalMode.value = true;
  void ensureFull();
}
function resetFilter() {
  draftTypes.value = [];
}
const pathNames = computed(() => {
  const map = new Map<string, string>();
  for (const crumb of trail.value) map.set(crumb.path, crumb.name);
  for (const file of fullFiles.value ?? listing.value?.files ?? [])
    map.set(file.path, file.name);
  return map;
});
// Render a folder path with human-readable names instead of the internal ids
// used by cloud providers (Baidu already stores readable paths).
function pathLabel(target: string): string {
  if (!target || target === "/") return "/";
  if (isBaidu.value) return target;
  const map = pathNames.value;
  const parts = target.split("/").filter(Boolean);
  const resolved = parts.map((part, index) => {
    const sub = "/" + parts.slice(0, index + 1).join("/");
    return map.get(sub) ?? map.get("/" + part) ?? part;
  });
  return "/" + resolved.join("/");
}
const scopeLabel = computed(() => {
  if (searching.value) return "搜索中…";
  if (searchResults.value) {
    const count = searchResults.value.length;
    return `搜索到 ${count} 项${searchTruncated.value ? "（已截断）" : ""}`;
  }
  if (!globalMode.value) return "";
  if (fullLoading.value) return "全库加载中…";
  const count = fullFiles.value?.length ?? 0;
  return `全库 ${count} 项${fullTruncated.value ? "（已截断）" : ""}`;
});
async function ensureFull() {
  if (!listing.value || !globalMode.value) return;
  const types = [...filterTypes.value].sort();
  const key = `${path.value}|${sortKey.value}|${sortAsc.value}|${types.join(",")}`;
  if (fullFiles.value && fullKey === key) return;
  const current = revision;
  fullLoading.value = true;
  try {
    const result = await invoke<{
      files: DriveFile[];
      total: number;
      truncated: boolean;
    }>("dir_list_all", {
      provider: props.provider,
      path: path.value,
      sortKey: sortKey.value,
      sortDesc: !sortAsc.value,
      fileTypes: filterTypes.value,
    });
    if (current !== revision) return;
    fullFiles.value = result.files;
    fullTruncated.value = result.truncated;
    fullKey = key;
  } catch (e) {
    if (current === revision) {
      emit("error", String(e));
      fullFiles.value = null;
    }
  } finally {
    if (current === revision) fullLoading.value = false;
  }
}
const selectedFiles = computed(() => {
  // 搜索结果可能不在当前分页里，因此从所有已加载的集合里取。
  const pool = new Map<string, DriveFile>();
  for (const file of listing.value?.files ?? []) pool.set(file.id, file);
  for (const file of fullFiles.value ?? []) pool.set(file.id, file);
  for (const file of searchResults.value ?? []) pool.set(file.id, file);
  return selected.value
    .map((id) => pool.get(id))
    .filter((file): file is DriveFile => !!file);
});
const shareFiles = computed(() =>
  shareTargets.value.length ? shareTargets.value : selectedFiles.value,
);
const allSelected = computed(
  () =>
    visibleFiles.value.length > 0 &&
    visibleFiles.value
      .slice(0, 100)
      .every((file) => selected.value.includes(file.id)),
);

watch(busy, (value) => emit("busy", value), { flush: "sync" });
watch(
  () => props.connected,
  (connected, previous) => {
    if (connected && !previous && !disposed) void load();
  },
);
watch(mode, (value) => {
  if (!value) {
    shareTargets.value = [];
    renameTarget.value = null;
    deleteTargets.value = [];
  }
});
function onDocumentClick(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  if (target && target.closest(".menu-anchor")) return;
  uploadMenuOpen.value = false;
  sortOpen.value = false;
  filterOpen.value = false;
}
onUnmounted(() => {
  explorerMemo.set(props.provider, {
    path: path.value,
    trail: trail.value.map((crumb) => ({ ...crumb })),
  });
  disposed = true;
  unlistenDrop?.();
  stopFolderPolling();
  document.removeEventListener("click", onDocumentClick);
  emit("busy", false);
});

function buildTrail(target: string): Trail[] {
  const segments = target.split("/").filter(Boolean);
  return [
    { name: "全部文件", path: "/" },
    ...segments.map((name, index) => ({
      name,
      path: "/" + segments.slice(0, index + 1).join("/"),
    })),
  ];
}

async function load(target?: string | Trail[], more = false) {
  const current = ++revision;
  loading.value = true;
  error.value = "";
  if (!more) {
    searchResults.value = null;
    searchTruncated.value = false;
    // A search that this load superseded will never reset its own spinner.
    searching.value = false;
  }
  try {
    if (isBaidu.value) {
      const dir = typeof target === "string" ? target : path.value;
      const nextPage = more ? page.value + 1 : 1;
      const result = await invoke<{
        accountId: string;
        path: string;
        files: DriveFile[];
        hasMore: boolean;
      }>("drive_list", {
        path: dir,
        page: nextPage,
        keyword: more ? loadedKeyword.value : keyword.value,
      });
      if (current !== revision) return;
      const previous = more ? (listing.value?.files ?? []) : [];
      if (more && listing.value && listing.value.accountId !== result.accountId)
        throw new Error("网盘账号已切换，请刷新文件列表");
      const known = new Set(previous.map((file) => file.id));
      listing.value = {
        accountId: result.accountId,
        path: result.path,
        files: [
          ...previous,
          ...result.files.filter((file) => !known.has(file.id)),
        ],
        hasMore: result.hasMore,
        nextCursor: null,
      };
      page.value = nextPage;
      path.value = result.path;
      trail.value = buildTrail(result.path);
      if (!more) {
        selected.value = [];
        loadedKeyword.value = keyword.value;
        fullFiles.value = null;
        fullKey = "";
        fullTruncated.value = false;
        if (globalMode.value) void ensureFull();
      }
    } else {
      const crumbs = Array.isArray(target) ? target : trail.value;
      const nextPage = more ? page.value + 1 : 1;
      const result = await invoke<{
        accountId: string;
        path: string;
        files: DriveFile[];
        nextCursor: string | null;
      }>("cloud_list", {
        provider: props.provider,
        path: crumbs[crumbs.length - 1]!.path,
        page: nextPage,
        cursor: more ? (listing.value?.nextCursor ?? null) : null,
      });
      if (current !== revision) return;
      const previous = more ? (listing.value?.files ?? []) : [];
      if (more && listing.value && listing.value.accountId !== result.accountId)
        throw new Error("网盘账号已切换，请刷新文件列表");
      const known = new Set(previous.map((file) => file.id));
      listing.value = {
        accountId: result.accountId,
        path: result.path,
        files: [
          ...previous,
          ...result.files.filter((file) => !known.has(file.id)),
        ],
        hasMore: result.nextCursor != null,
        nextCursor: result.nextCursor,
      };
      page.value = nextPage;
      trail.value = crumbs;
      path.value = result.path;
      if (!more) {
        selected.value = [];
        keyword.value = "";
        fullFiles.value = null;
        fullKey = "";
        fullTruncated.value = false;
        if (globalMode.value) void ensureFull();
      }
    }
  } catch (e) {
    if (current === revision) {
      error.value = String(e);
      if (!more) {
        listing.value = null;
        selected.value = [];
      }
    }
  } finally {
    if (current === revision) loading.value = false;
  }
}

function enter(file: DriveFile) {
  if (!file.isDir || busy.value) return;
  keyword.value = "";
  if (isBaidu.value) {
    void load(file.path);
    return;
  }
  const fromSearch = !!searchResults.value;
  searchResults.value = null;
  searchTruncated.value = false;
  if (fromSearch) {
    // 搜索结果可能来自任意子目录，用完整路径直接进入；上级只保留根。
    void load([
      { name: "全部文件", path: "/" },
      { name: file.name, path: file.path },
    ]);
    return;
  }
  void load([...trail.value, { name: file.name, path: file.path }]);
}
function goCrumb(index: number) {
  if (busy.value) return;
  keyword.value = "";
  if (isBaidu.value) void load(trail.value[index]!.path);
  else void load(trail.value.slice(0, index + 1));
}
async function search() {
  if (isBaidu.value) {
    void load();
    return;
  }
  const key = keyword.value.trim();
  if (!key) {
    searchResults.value = null;
    searchTruncated.value = false;
    return;
  }
  // Bump the revision like load() does: a stale search result must not
  // overwrite a newer listing, and the lock must be released when a load takes over.
  const current = ++revision;
  searching.value = true;
  error.value = "";
  try {
    const result = await invoke<{
      accountId: string;
      keyword: string;
      files: DriveFile[];
      total: number;
      truncated: boolean;
    }>("cloud_search", {
      provider: props.provider,
      path: path.value,
      keyword: key,
    });
    if (current !== revision) return;
    searchResults.value = result.files;
    searchTruncated.value = result.truncated;
    selected.value = [];
  } catch (e) {
    if (current === revision) {
      error.value = String(e);
      searchResults.value = null;
      searchTruncated.value = false;
    }
  } finally {
    if (current === revision) searching.value = false;
  }
}
function selectAll() {
  selected.value = allSelected.value
    ? []
    : visibleFiles.value.slice(0, 100).map((file) => file.id);
}

async function enqueue(files: DriveFile[]) {
  if (!listing.value || !files.length) return;
  adding.value = true;
  error.value = "";
  try {
    const tasks = isBaidu.value
      ? await invoke<{ status?: string; error?: string }[]>("drive_enqueue", {
          accountId: listing.value.accountId,
          files,
        })
      : await invoke<{ status?: string; error?: string }[]>("cloud_enqueue", {
          provider: props.provider,
          accountId: listing.value.accountId,
          files,
        });
    const failure = (tasks || []).find(
      (task) => task && task.status === "error",
    );
    if (failure) emit("error", failure.error ?? "部分文件添加失败");
    else
      emit(
        "notice",
        files.some((file) => file.isDir)
          ? `文件夹已加入下载列表（共 ${(tasks || []).length} 个文件），正在后台解析下载地址`
          : `已添加 ${(tasks || []).length} 个文件下载任务，正在后台解析下载地址`,
      );
    selected.value = [];
    emit("changed");
    emit("downloaded");
  } catch (e) {
    emit("error", String(e));
  } finally {
    adding.value = false;
  }
}
function download() {
  if (!busy.value && selected.value.length) void enqueue(selectedFiles.value);
}
function downloadOne(file: DriveFile) {
  if (!busy.value) void enqueue([file]);
}

async function chooseUpload() {
  uploadMenuOpen.value = false;
  if (!canDrop.value) return;
  const directory = trail.value[trail.value.length - 1]!.path;
  choosingUpload.value = true;
  try {
    const picked = await open({
      multiple: true,
      directory: false,
      title: `选择要上传到${providerName(props.provider)}的文件`,
    });
    const paths = (
      Array.isArray(picked) ? picked : picked ? [picked] : []
    ).filter((value): value is string => typeof value === "string");
    if (!paths.length) return;
    uploadPaths.value = paths;
    uploadDirectory.value = directory;
    uploadPlan.value = null;
    uploadBatch.value = null;
    modalError.value = "";
    created.value = null;
    mode.value = "upload";
  } catch (e) {
    emit("error", String(e));
  } finally {
    choosingUpload.value = false;
  }
}
async function chooseUploadFolder() {
  uploadMenuOpen.value = false;
  if (!canDrop.value) return;
  const directory = trail.value[trail.value.length - 1]!.path;
  choosingUpload.value = true;
  try {
    const picked = await open({
      multiple: false,
      directory: true,
      title: `选择要上传到${providerName(props.provider)}的文件夹`,
    });
    const folder = Array.isArray(picked) ? picked[0] : picked;
    if (typeof folder !== "string") return;
    const entries = await invoke<{ path: string; relative: string }[]>(
      "collect_local_files",
      { root: folder },
    );
    if (!entries.length) {
      emit("notice", "所选文件夹为空");
      return;
    }
    uploadPlan.value = entries;
    uploadPaths.value = entries.map((entry) => entry.path);
    uploadDirectory.value = directory;
    const rootName =
      folder.split(/[\\/]/).filter(Boolean).pop()?.trim() || "上传文件夹";
    uploadBatch.value = {
      id: `upload:${props.provider}:${listing.value?.accountId ?? ""}:${directory}:${rootName}`,
      name: rootName,
    };
    modalError.value = "";
    created.value = null;
    mode.value = "upload";
  } catch (e) {
    emit("error", String(e));
  } finally {
    choosingUpload.value = false;
  }
}
async function enqueueUploads(paths: string[], directory: string) {
  if (!listing.value) return;
  const files = [...new Set(paths)];
  if (!files.length || files.length > 100)
    throw new Error("一次请选择 1 至 100 个本地文件上传");
  await invoke("upload_enqueue", {
    provider: props.provider,
    accountId: listing.value.accountId,
    directory,
    paths: files,
    concurrency: concurrency.value,
    batch: null,
  });
  mode.value = "";
  uploadPlan.value = null;
  emit("notice", `已添加 ${files.length} 个上传任务，目标目录：${directory}`);
  emit("changed");
  emit("uploaded");
}
async function findRemoteDir(
  parent: string,
  name: string,
): Promise<DriveFile | null> {
  if (!listing.value) return null;
  if (props.provider === "baidu") {
    const result = await invoke<{ files: DriveFile[] }>("drive_list", {
      path: parent,
      page: 1,
      keyword: "",
    });
    return (
      result.files.find((file) => file.isDir && file.name === name) ?? null
    );
  }
  let page = 1;
  let cursor: string | undefined;
  for (let i = 0; i < 50; i += 1) {
    const result = await invoke<{
      files: DriveFile[];
      nextCursor?: string | null;
    }>("cloud_list", {
      provider: props.provider,
      path: parent,
      page,
      cursor,
    });
    const hit = result.files.find((file) => file.isDir && file.name === name);
    if (hit) return hit;
    if (!result.nextCursor) break;
    cursor = result.nextCursor;
    page += 1;
  }
  return null;
}
async function ensureRemoteDir(
  base: string,
  relative: string,
): Promise<string> {
  if (!listing.value) return base;
  let current = base;
  for (const segment of relative.split("/").filter(Boolean)) {
    const existing = await findRemoteDir(current, segment);
    if (existing) {
      current = existing.path;
      continue;
    }
    try {
      current = (
        await invoke<DriveFile>("drive_mkdir_at", {
          provider: props.provider,
          accountId: listing.value.accountId,
          parent: current,
          name: segment,
        })
      ).path;
    } catch (error) {
      const again = await findRemoteDir(current, segment);
      if (!again) throw error;
      current = again.path;
    }
  }
  return current;
}
const folderOverall = computed(() => {
  const steps = folderSteps.value;
  if (!steps || !steps.length) return 0;
  const total = steps.reduce((sum, step) => sum + step.total, 0);
  if (!total) return 0;
  const done = steps.reduce(
    (sum, step) => sum + Math.min(step.done, step.total),
    0,
  );
  return Math.round((done / total) * 100);
});
function remoteParent(remotePath: string): string {
  const index = remotePath.lastIndexOf("/");
  return index <= 0 ? "/" : remotePath.slice(0, index);
}
function stopFolderPolling() {
  if (folderTimer) {
    window.clearTimeout(folderTimer);
    folderTimer = undefined;
  }
}
async function pollFolder() {
  stopFolderPolling();
  const steps = folderSteps.value;
  if (!steps || disposed) return;
  try {
    const snapshot = await invoke<{
      uploads: { remotePath: string; status: string }[];
    }>("snapshot");
    // The panel can be unmounted while this request is in flight; rescheduling
    // afterwards kept a detached component polling every second forever.
    if (disposed) return;
    const uploads = snapshot.uploads ?? [];
    for (const step of steps) {
      if (
        step.status === "creating" ||
        step.status === "waiting" ||
        step.status === "error"
      )
        continue;
      const tasks = uploads.filter(
        (task) => remoteParent(task.remotePath) === step.remote,
      );
      if (!tasks.length) continue;
      const finished = tasks.filter(
        (task) => task.status === "complete",
      ).length;
      const errored = tasks.filter((task) => task.status === "error").length;
      step.done = finished;
      step.percent = step.total ? Math.round((finished / step.total) * 100) : 0;
      step.status =
        finished + errored >= step.total && step.total > 0
          ? errored
            ? "error"
            : "done"
          : "uploading";
    }
    if (
      steps.some(
        (step) =>
          step.status === "uploading" ||
          step.status === "creating" ||
          step.status === "waiting",
      )
    ) {
      folderTimer = window.setTimeout(() => void pollFolder(), 900);
    }
  } catch {
    if (disposed) return;
    folderTimer = window.setTimeout(() => void pollFolder(), 1500);
  }
}
function dismissFolder() {
  stopFolderPolling();
  folderSteps.value = null;
}
async function enqueueFolder(
  plan: { path: string; relative: string }[],
  directory: string,
) {
  if (!listing.value) return;
  const counts = new Map<string, number>();
  for (const entry of plan) {
    const relative = entry.relative || "";
    counts.set(relative, (counts.get(relative) ?? 0) + 1);
  }
  const relatives = [...counts.keys()]
    .filter(Boolean)
    .sort((a, b) => a.split("/").length - b.split("/").length);
  const steps: FolderStep[] = [];
  const rootCount = counts.get("") ?? 0;
  if (rootCount > 0) {
    steps.push({
      label: "（当前目录）",
      remote: directory,
      total: rootCount,
      done: 0,
      percent: 0,
      status: "waiting",
    });
  }
  for (const relative of relatives) {
    steps.push({
      label: relative,
      remote: "",
      total: counts.get(relative) ?? 0,
      done: 0,
      percent: 0,
      status: "waiting",
    });
  }
  folderSteps.value = steps;
  folderBusy.value = true;
  uploadPlan.value = null;
  const directoryMap = new Map<string, string>();
  try {
    for (const step of folderSteps.value!) {
      const relative = step.label === "（当前目录）" ? "" : step.label;
      step.status = "creating";
      const target = await ensureRemoteDir(directory, relative);
      step.remote = target;
      step.status = "uploading";
      directoryMap.set(relative, target);
    }
    const groups = new Map<string, string[]>();
    for (const entry of plan) {
      const target = directoryMap.get(entry.relative || "") ?? directory;
      const bucket = groups.get(target) ?? [];
      bucket.push(entry.path);
      groups.set(target, bucket);
    }
    let total = 0;
    for (const [target, paths] of groups) {
      for (let index = 0; index < paths.length; index += 100) {
        const chunk = paths.slice(index, index + 100);
        await invoke("upload_enqueue", {
          provider: props.provider,
          accountId: listing.value.accountId,
          directory: target,
          paths: chunk,
          concurrency: concurrency.value,
          batch: uploadBatch.value,
        });
        total += chunk.length;
      }
    }
    emit("notice", `已添加 ${total} 个上传任务（保留目录结构）`);
    emit("changed");
    void pollFolder();
  } catch (error) {
    emit("error", String(error));
    if (folderSteps.value) {
      folderSteps.value = folderSteps.value.map((step) => ({
        ...step,
        status: step.status === "done" ? "done" : "error",
      }));
    }
  } finally {
    folderBusy.value = false;
  }
}
function handleDragDrop(event: DragDropEvent) {
  dropDirectory.value = null;
  if (disposed || event.type === "leave" || !canDrop.value) return;
  // Native drag positions are physical pixels; DOM hit testing uses CSS pixels.
  const { x, y } = event.position.toLogical(window.devicePixelRatio);
  const element = document.elementFromPoint(x, y);
  if (!element || !panel.value?.contains(element)) return;
  const folder = element.closest<HTMLElement>("[data-upload-directory]");
  const directory = folder?.dataset.uploadDirectory ?? path.value;
  if (event.type !== "drop") {
    dropDirectory.value = directory;
    return;
  }
  uploading.value = true;
  void enqueueUploads(event.paths, directory)
    .catch((e) => emit("error", String(e)))
    .finally(() => {
      uploading.value = false;
    });
}
function openMkdir() {
  if (!canMkdir.value) return;
  modalError.value = "";
  folderName.value = "";
  mode.value = "mkdir";
}
function prepareShare(files?: DriveFile[]) {
  if (!canShare.value) return;
  shareTargets.value = files ?? [];
  const random = new Uint8Array(4);
  crypto.getRandomValues(random);
  const alphabet = "abcdefghjkmnpqrstuvwxyz23456789";
  password.value = Array.from(
    random,
    (byte) => alphabet[byte % alphabet.length],
  ).join("");
  created.value = null;
  modalError.value = "";
  mode.value = "share";
}
function openRename(file: DriveFile) {
  if (!canRename.value || busy.value) return;
  renameTarget.value = file;
  renameName.value = file.name;
  modalError.value = "";
  mode.value = "rename";
}
function openDelete(files?: DriveFile[]) {
  if (!canDelete.value || busy.value) return;
  const list = files && files.length ? files : selectedFiles.value;
  if (!list.length) {
    emit("notice", "请选择要删除的文件");
    return;
  }
  deleteTargets.value = list;
  modalError.value = "";
  mode.value = "delete";
}
async function submit() {
  if (!listing.value) return;
  modalBusy.value = true;
  modalError.value = "";
  try {
    if (mode.value === "upload") {
      if (uploadPlan.value && uploadPlan.value.length) {
        const plan = uploadPlan.value;
        mode.value = "";
        await enqueueFolder(plan, uploadDirectory.value);
      } else {
        await enqueueUploads(uploadPaths.value, uploadDirectory.value);
      }
    } else if (mode.value === "share") {
      created.value = await invoke<CreatedShare>("drive_share", {
        provider: props.provider,
        accountId: listing.value.accountId,
        files: shareFiles.value,
        period: period.value,
        password: password.value,
      });
      emit("changed");
    } else if (mode.value === "mkdir") {
      await invoke("drive_mkdir_at", {
        provider: props.provider,
        accountId: listing.value.accountId,
        parent: path.value,
        name: folderName.value,
      });
      mode.value = "";
      folderName.value = "";
      emit("notice", "文件夹已创建");
      await load();
    } else if (mode.value === "rename") {
      if (!renameTarget.value) return;
      const name = renameName.value.trim();
      if (!name) {
        modalError.value = "请输入新的名称";
        return;
      }
      if (name === renameTarget.value.name) {
        mode.value = "";
        return;
      }
      await invoke<DriveFile>("drive_rename_at", {
        provider: props.provider,
        accountId: listing.value.accountId,
        file: renameTarget.value,
        newName: name,
      });
      mode.value = "";
      renameTarget.value = null;
      emit("notice", "已重命名");
      await load();
      emit("changed");
    } else if (mode.value === "delete") {
      if (!deleteTargets.value.length) return;
      await invoke("drive_delete_at", {
        provider: props.provider,
        accountId: listing.value.accountId,
        files: deleteTargets.value,
      });
      const count = deleteTargets.value.length;
      mode.value = "";
      deleteTargets.value = [];
      selected.value = [];
      emit("notice", `已删除 ${count} 项`);
      await load();
      emit("changed");
    }
  } catch (e) {
    modalError.value = String(e);
  } finally {
    modalBusy.value = false;
  }
}
async function copyShare() {
  if (!created.value) return;
  try {
    await navigator.clipboard.writeText(shareText(created.value));
    emit("notice", "分享链接和提取码已复制");
  } catch {
    modalError.value = "复制失败，请选中下方文字手动复制";
  }
}

onMounted(async () => {
  void load();
  document.addEventListener("click", onDocumentClick);
  if (!canUpload.value) return;
  try {
    const unlisten = await getCurrentWebview().onDragDropEvent(({ payload }) =>
      handleDragDrop(payload),
    );
    if (disposed) unlisten();
    else unlistenDrop = unlisten;
  } catch (e) {
    if (!disposed) emit("error", `无法启用拖拽上传，请使用「上传文件」：${e}`);
  }
});
</script>

<template>
  <section
    ref="panel"
    class="drive-panel card"
    :class="{ 'drop-active': canDrop && dropDirectory !== null }"
  >
    <div
      v-if="canDrop && dropDirectory !== null"
      class="drive-drop-notice"
      role="status"
    >
      <CloudUpload :size="22" />
      <span
        >松开即可上传到 <strong>{{ pathLabel(dropDirectory) }}</strong></span
      >
    </div>

    <div class="drive-toolbar">
      <span class="provider-tag">{{ providerName(provider) }}</span>
      <form class="drive-search" @submit.prevent="search">
        <Search :size="16" /><input
          v-model="keyword"
          placeholder="搜索当前目录及子目录"
          aria-label="搜索网盘文件"
          maxlength="128"
          :disabled="busy"
        /><button class="text-button" :disabled="busy || searching">
          {{ searching ? "搜索中…" : "搜索" }}
        </button>
      </form>
      <span class="drive-toolbar-spacer" />
      <button v-if="!connected" class="primary" @click="emit('login')">
        <UserRound :size="16" />登录{{ providerName(provider) }}
      </button>
      <div
        v-if="canUpload"
        class="menu-anchor"
        @mouseenter="uploadMenuOpen = true"
        @mouseleave="uploadMenuOpen = false"
      >
        <button class="secondary" :disabled="!canDrop" @click="chooseUpload">
          <CloudUpload :size="16" />上传文件
        </button>
        <div v-if="uploadMenuOpen" class="drive-menu menu-upload" @click.stop>
          <button class="drive-menu-item" @click="chooseUpload">
            <CloudUpload :size="16" />上传文件
          </button>
          <button class="drive-menu-item" @click="chooseUploadFolder">
            <FolderUp :size="16" />上传文件夹
          </button>
        </div>
      </div>
      <button
        v-if="canMkdir"
        class="secondary"
        :disabled="!listing || busy"
        @click="openMkdir"
      >
        <FolderPlus :size="16" />新建文件夹
      </button>
      <div class="drive-toolbar-group">
        <div class="menu-anchor">
          <button
            class="icon-button"
            :class="{ active: sortKey !== 'name' || !sortAsc }"
            :title="`排序：${sortLabel}`"
            @click="sortOpen = !sortOpen"
          >
            <ArrowDownNarrowWide :size="18" />
          </button>
          <div v-if="sortOpen" class="drive-menu menu-sort" @click.stop>
            <div class="sort-group">
              <h4>按修改时间</h4>
              <div class="sort-options">
                <button
                  class="sort-option"
                  :class="{ active: isSort('modified', false) }"
                  @click="applySort('modified', false)"
                >
                  从新到旧
                </button>
                <button
                  class="sort-option"
                  :class="{ active: isSort('modified', true) }"
                  @click="applySort('modified', true)"
                >
                  从旧到新
                </button>
              </div>
            </div>
            <div class="sort-group">
              <h4>按创建时间</h4>
              <div class="sort-options">
                <button
                  class="sort-option"
                  :class="{ active: isSort('created', false) }"
                  @click="applySort('created', false)"
                >
                  从新到旧
                </button>
                <button
                  class="sort-option"
                  :class="{ active: isSort('created', true) }"
                  @click="applySort('created', true)"
                >
                  从旧到新
                </button>
              </div>
            </div>
            <div class="sort-group">
              <h4>按文件名称</h4>
              <div class="sort-options">
                <button
                  class="sort-option"
                  :class="{ active: isSort('name', true) }"
                  @click="applySort('name', true)"
                >
                  从 A 到 Z
                </button>
                <button
                  class="sort-option"
                  :class="{ active: isSort('name', false) }"
                  @click="applySort('name', false)"
                >
                  从 Z 到 A
                </button>
              </div>
            </div>
            <div class="sort-group">
              <h4>按文件大小</h4>
              <div class="sort-options">
                <button
                  class="sort-option"
                  :class="{ active: isSort('size', true) }"
                  @click="applySort('size', true)"
                >
                  从小到大
                </button>
                <button
                  class="sort-option"
                  :class="{ active: isSort('size', false) }"
                  @click="applySort('size', false)"
                >
                  从大到小
                </button>
              </div>
            </div>
          </div>
        </div>
        <div class="view-toggle" role="group" aria-label="视图切换">
          <button
            class="view-btn"
            :class="{ active: viewMode === 'list' }"
            title="列表视图"
            @click="viewMode = 'list'"
          >
            <List :size="17" />
          </button>
          <button
            class="view-btn"
            :class="{ active: viewMode === 'grid' }"
            title="网格视图"
            @click="viewMode = 'grid'"
          >
            <LayoutGrid :size="17" />
          </button>
        </div>
        <div class="menu-anchor">
          <button
            class="icon-button"
            :class="{ active: filterActive }"
            title="筛选"
            @click="openFilter"
          >
            <Filter :size="17" />
          </button>
          <div v-if="filterOpen" class="drive-menu menu-filter" @click.stop>
            <h4 class="filter-title">添加筛选条件</h4>
            <p class="filter-sub">文件类型</p>
            <label class="filter-all">
              <input
                type="checkbox"
                :checked="draftTypes.length === 0"
                @change="toggleAllTypes"
              />
              全部类型
            </label>
            <div class="filter-grid">
              <label
                v-for="type in FILE_TYPES"
                :key="type.id"
                class="filter-cell"
              >
                <input
                  type="checkbox"
                  :checked="draftTypes.includes(type.id)"
                  @change="toggleDraftType(type.id)"
                />
                {{ type.label }}
              </label>
            </div>
            <div class="filter-foot">
              <button class="secondary reset" @click="resetFilter">重置</button>
              <button class="secondary" @click="filterOpen = false">
                取消
              </button>
              <button class="primary" @click="applyFilter">确认</button>
            </div>
          </div>
        </div>
        <button
          class="icon-button"
          title="刷新网盘文件"
          :disabled="busy"
          @click="load()"
        >
          <RefreshCw :size="17" :class="{ spinning: loading }" />
        </button>
      </div>
    </div>

    <div v-if="viewMode === 'grid' && selected.length" class="drive-batchbar">
      <span class="batch-count"
        >已选中 <b>{{ selected.length }}</b> 项</span
      >
      <button class="text-button batch-clear" @click="selected = []">
        取消选中
      </button>
      <span class="batch-spacer" />
      <div class="batch-actions">
        <button class="primary" :disabled="busy" @click="download">
          <LoaderCircle
            v-if="adding"
            class="spinning"
            :size="16"
          /><ArrowDownToLine v-else :size="16" />下载
        </button>
        <button
          class="secondary"
          :disabled="!canShare || busy"
          :title="
            canShare
              ? '创建分享链接'
              : `${providerName(provider)}暂不支持创建分享`
          "
          @click="prepareShare()"
        >
          <Share2 :size="16" />分享
        </button>
        <button
          class="secondary"
          :disabled="!canDelete || busy"
          :title="
            canDelete ? '删除所选' : `${providerName(provider)}暂不支持删除`
          "
          @click="openDelete()"
        >
          <Trash2 :size="16" />删除
        </button>
        <button class="icon-button" disabled title="更多操作，敬请期待">
          <MoreHorizontal :size="17" />
        </button>
      </div>
    </div>

    <div class="drive-navrow">
      <div class="drive-breadcrumbs" aria-label="网盘文件路径">
        <Home :size="16" />
        <template
          v-for="(item, index) in displayTrail"
          :key="item.kind === 'crumb' ? item.path : `gap-${index}`"
        >
          <ChevronRight v-if="index" :size="14" /><span
            v-if="item.kind === 'gap'"
            class="crumb-gap"
            :title="item.names.join(' / ')"
            >…</span
          ><button
            v-else
            :data-upload-directory="item.path"
            :class="{ 'drop-folder': canDrop && dropDirectory === item.path }"
            :disabled="busy"
            @click="goCrumb(item.index)"
          >
            {{ item.name }}
          </button>
        </template>
      </div>
      <span v-if="scopeLabel" class="drive-scope">{{ scopeLabel }}</span>
    </div>

    <p v-if="canUpload" class="drive-drop-hint">
      拖入本地文件即可上传到当前目录，也可直接拖到下方文件夹。
    </p>

    <div v-if="error" class="drive-error" role="alert">
      <p>{{ error }}</p>
      <button class="secondary" :disabled="busy" @click="emit('login')">
        登录{{ providerName(provider) }}</button
      ><button class="text-button" :disabled="busy" @click="load()">
        重新加载
      </button>
    </div>
    <div
      v-else
      class="drive-files"
      :class="{ 'view-grid': viewMode === 'grid' }"
      :aria-busy="loading"
    >
      <div
        class="drive-file-head"
        :class="{ 'head-selected': selected.length }"
      >
        <input
          type="checkbox"
          :checked="allSelected"
          :disabled="busy || !visibleFiles.length"
          aria-label="全选网盘文件（最多100项）"
          @change="selectAll"
        />
        <div class="head-name">
          <template v-if="!selected.length">
            <button class="col-sort" @click="sortHeader('name')">
              文件名称
              <ArrowUp
                v-if="sortKey === 'name' && sortAsc"
                :size="12"
                class="sort-ind"
              /><ArrowDown
                v-else-if="sortKey === 'name'"
                :size="12"
                class="sort-ind"
              /><ChevronsUpDown v-else :size="12" class="sort-ind muted" />
            </button>
          </template>
          <template v-else>
            <span class="batch-count"
              >已选中 <b>{{ selected.length }}</b> 项</span
            >
            <button class="text-button batch-clear" @click="selected = []">
              取消选中
            </button>
            <div class="batch-actions">
              <button class="batch-btn" :disabled="busy" @click="download">
                <LoaderCircle
                  v-if="adding"
                  class="spinning"
                  :size="15"
                /><ArrowDownToLine v-else :size="15" />下载
              </button>
              <button
                class="batch-btn"
                :disabled="!canShare || busy"
                :title="
                  canShare
                    ? '创建分享链接'
                    : `${providerName(provider)}暂不支持创建分享`
                "
                @click="prepareShare()"
              >
                <Share2 :size="15" />分享
              </button>
              <button
                class="batch-btn"
                :disabled="!canDelete || busy"
                :title="
                  canDelete
                    ? '删除所选'
                    : `${providerName(provider)}暂不支持删除`
                "
                @click="openDelete()"
              >
                <Trash2 :size="15" />删除
              </button>
              <button
                class="batch-btn icon"
                disabled
                title="更多操作，敬请期待"
              >
                <MoreHorizontal :size="16" />
              </button>
            </div>
          </template>
        </div>
        <button class="col-sort" @click="sortHeader('size')">
          大小
          <ArrowUp
            v-if="sortKey === 'size' && sortAsc"
            :size="12"
            class="sort-ind"
          /><ArrowDown
            v-else-if="sortKey === 'size'"
            :size="12"
            class="sort-ind"
          /><ChevronsUpDown v-else :size="12" class="sort-ind muted" />
        </button>
        <button class="col-sort" @click="sortHeader('modified')">
          修改时间
          <ArrowUp
            v-if="sortKey === 'modified' && sortAsc"
            :size="12"
            class="sort-ind"
          /><ArrowDown
            v-else-if="sortKey === 'modified'"
            :size="12"
            class="sort-ind"
          /><ChevronsUpDown v-else :size="12" class="sort-ind muted" />
        </button>
      </div>
      <div
        v-for="file in visibleFiles"
        :key="file.id"
        class="drive-file-row"
        :data-upload-directory="file.isDir ? file.path : undefined"
        :class="{
          'drop-folder': canDrop && file.isDir && dropDirectory === file.path,
        }"
      >
        <input
          v-model="selected"
          type="checkbox"
          :value="file.id"
          :disabled="
            busy || (selected.length >= 100 && !selected.includes(file.id))
          "
          :aria-label="`选择 ${file.name}`"
        />
        <div class="drive-file-name">
          <Folder v-if="file.isDir" class="folder-glyph" :size="22" /><File
            v-else
            :size="21"
          />
          <div>
            <button
              v-if="file.isDir"
              :title="file.path"
              :disabled="busy"
              @click="enter(file)"
            >
              {{ file.name }}</button
            ><span v-else :title="file.path">{{ file.name }}</span
            ><small v-if="isBaidu && keyword">{{ file.path }}</small>
          </div>
          <div class="drive-row-actions">
            <button
              class="row-action"
              :title="`下载 ${file.name}`"
              :disabled="busy"
              @click.stop="downloadOne(file)"
            >
              <ArrowDownToLine :size="16" />
            </button>
            <button
              class="row-action"
              :title="canShare ? `分享 ${file.name}` : '分享（该网盘暂不支持）'"
              :disabled="!canShare || busy"
              @click.stop="prepareShare([file])"
            >
              <Share2 :size="16" />
            </button>
            <button
              class="row-action"
              :title="canRename ? '重命名' : '重命名（该网盘暂不支持）'"
              :disabled="!canRename || busy"
              @click.stop="openRename(file)"
            >
              <Pencil :size="15" />
            </button>
            <button
              class="row-action danger"
              :title="canDelete ? '删除' : '删除（该网盘暂不支持）'"
              :disabled="!canDelete || busy"
              @click.stop="openDelete([file])"
            >
              <Trash2 :size="15" />
            </button>
          </div>
        </div>
        <span class="file-size">{{
          file.isDir ? "文件夹" : formatBytes(file.size)
        }}</span
        ><span class="file-modified">{{ formatDate(file.modified) }}</span>
      </div>
      <div v-if="loading && !listing" class="drive-empty">
        <LoaderCircle class="spinning" :size="30" />
        <p>正在读取{{ providerName(provider) }}文件…</p>
      </div>
      <div v-else-if="listing && !visibleFiles.length" class="drive-empty">
        <Folder :size="40" />
        <p>
          {{
            keyword
              ? isBaidu
                ? "没有找到匹配的文件"
                : "没有匹配的文件"
              : "这个文件夹还是空的"
          }}
        </p>
        <span v-if="canUpload && !keyword">拖入本地文件或点击「上传文件」</span>
      </div>
      <button
        v-if="listing?.hasMore && !globalMode"
        class="load-more"
        :disabled="busy"
        @click="load(undefined, true)"
      >
        {{ loading ? "正在加载…" : "加载更多文件" }}
      </button>
    </div>
  </section>

  <div class="directory-row">
    <Folder :size="16" /><span>下载保存到</span
    ><span class="directory" :title="downloadDir">{{ downloadDir }}</span
    ><button class="text-button" :disabled="busy" @click="emit('directory')">
      更改
    </button>
  </div>

  <Teleport to="body"
    ><div
      v-if="mode"
      class="modal-backdrop"
      @click.self="!modalBusy && (mode = '')"
    >
      <form class="cloud-dialog" @submit.prevent="submit">
        <div class="dialog-heading">
          <h2>
            {{
              mode === "upload"
                ? `上传到${providerName(provider)}`
                : mode === "mkdir"
                  ? "新建文件夹"
                  : mode === "rename"
                    ? "重命名"
                    : mode === "delete"
                      ? `删除 ${deleteTargets.length} 项`
                      : created
                        ? "分享已创建"
                        : "创建文件分享"
            }}
          </h2>
          <button
            type="button"
            class="icon-button"
            aria-label="关闭对话框"
            :disabled="modalBusy"
            @click="mode = ''"
          >
            <X :size="18" />
          </button>
        </div>
        <div v-if="modalError" class="error-banner" role="alert">
          {{ modalError }}
        </div>
        <template v-if="mode === 'upload'"
          ><label
            >目标目录<input :value="pathLabel(uploadDirectory)" readonly
          /></label>
          <ul class="review-files">
            <li v-for="file in uploadPaths" :key="file" :title="file">
              {{ file.split(/[\\/]/).pop() }}
            </li>
          </ul>
          <label
            >每个文件的分片并发<select v-model.number="concurrency">
              <option v-for="n in [1, 2, 4, 8]" :key="n" :value="n">
                {{ n }} 路并发
              </option>
            </select></label
          >
          <p class="dialog-note">
            最多同时上传 2 个文件，总分片并发不超过
            8。遇到同名文件会提示处理，已有文件不会被覆盖。
          </p></template
        >
        <template v-else-if="mode === 'mkdir'"
          ><label
            >名称<input
              v-model="folderName"
              placeholder="文件夹名称"
              maxlength="120"
              required
              autofocus
          /></label>
          <p class="dialog-note">创建位置：{{ path }}</p></template
        >
        <template v-else-if="mode === 'rename'"
          ><label
            >新名称<input
              v-model="renameName"
              placeholder="文件或文件夹名称"
              maxlength="120"
              required
              autofocus
          /></label>
          <p class="dialog-note">
            当前名称：{{ renameTarget ? renameTarget.name : "" }}
          </p></template
        >
        <template v-else-if="mode === 'delete'"
          ><ul class="review-files">
            <li v-for="file in deleteTargets" :key="file.id" :title="file.path">
              {{ file.name }}
            </li>
          </ul>
          <p class="dialog-note">
            删除后无法恢复，请确认要删除以上 {{ deleteTargets.length }} 项。
          </p></template
        >
        <template v-else-if="mode === 'share' && !created"
          ><ul class="review-files">
            <li v-for="file in shareFiles" :key="file.id">
              {{ file.name }}
            </li>
          </ul>
          <label
            >有效期<select v-model.number="period">
              <option :value="1">1 天</option>
              <option :value="7">7 天</option>
              <option :value="30">30 天</option>
              <option :value="0">永久有效</option>
            </select></label
          ><label
            >提取码<input
              v-model="password"
              maxlength="4"
              minlength="4"
              pattern="[a-zA-Z0-9]{4}"
              required
          /></label>
          <p class="dialog-note">
            持有链接和提取码的人可以访问所选内容。创建后可在「分享记录」中取消分享。
          </p></template
        >
        <template v-else-if="mode === 'share' && created"
          ><div class="share-success">
            <Check :size="22" />链接已生成 ·
            {{ formatSharePeriod(created.period) }}
          </div>
          <textarea
            :value="shareText(created)"
            readonly
            aria-label="生成的分享链接"
            rows="3"
          /><button type="button" class="primary wide" @click="copyShare">
            <Copy :size="16" />复制链接和提取码
          </button></template
        >
        <div v-if="!created || mode !== 'share'" class="dialog-actions">
          <button
            type="button"
            class="secondary"
            :disabled="modalBusy"
            @click="mode = ''"
          >
            取消</button
          ><button class="primary" :disabled="modalBusy">
            <LoaderCircle v-if="modalBusy" class="spinning" :size="16" />{{
              modalBusy
                ? "正在处理…"
                : mode === "upload"
                  ? "开始上传"
                  : mode === "rename"
                    ? "保存"
                    : mode === "delete"
                      ? "删除"
                      : "创建"
            }}
          </button>
        </div>
      </form>
    </div></Teleport
  >
  <Teleport to="body">
    <div v-if="folderSteps" class="folder-progress">
      <header>
        <strong>上传文件夹</strong>
        <span class="fp-overall-text">{{ folderOverall }}%</span>
        <button
          class="fp-close"
          aria-label="关闭上传进度"
          @click="dismissFolder"
        >
          ×
        </button>
      </header>
      <div class="fp-bar"><i :style="{ width: folderOverall + '%' }" /></div>
      <ul>
        <li v-for="(step, index) in folderSteps" :key="index">
          <div class="fp-row">
            <span class="fp-label" :title="step.label">{{ step.label }}</span>
            <span class="fp-status">{{
              step.status === "creating"
                ? "创建目录…"
                : step.status === "waiting"
                  ? "等待中"
                  : `${step.done}/${step.total}`
            }}</span>
          </div>
          <div class="fp-bar small" :class="step.status">
            <i
              :style="{
                width:
                  (step.status === 'creating' || step.status === 'waiting'
                    ? 0
                    : step.percent) + '%',
              }"
            />
          </div>
        </li>
      </ul>
      <footer>
        <button class="secondary" @click="emit('uploaded')">
          查看上传任务
        </button>
      </footer>
    </div>
  </Teleport>
</template>
