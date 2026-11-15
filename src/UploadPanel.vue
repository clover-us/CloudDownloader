<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowUpFromLine,
  ChevronRight,
  File,
  Folder,
  LoaderCircle,
  Pause,
  Play,
  RefreshCw,
  Trash2,
} from "@lucide/vue";
import { type UploadTask, formatBytes } from "./cloud";
import { type Provider, providerName } from "./providers";
const props = defineProps<{ tasks: UploadTask[] }>();
const emit = defineEmits<{
  changed: [];
  error: [message: string];
  notice: [message: string];
  browse: [];
}>();
type Batch = { id: string; name: string; path?: string };
type Group = {
  id: string;
  name: string;
  path: string;
  tasks: UploadTask[];
  size: number;
  completed: number;
  speed: number;
  status: string;
  done: number;
  total: number;
  progress: number;
};
type Row =
  | { kind: "group"; key: string; group: Group }
  | { kind: "task"; key: string; task: UploadTask; child: boolean };
const busy = ref("");
const filter = ref("all");
const removeTarget = ref<UploadTask | null>(null);
const selected = ref<string[]>([]);
const expanded = ref<string[]>([]);
const batchBusy = ref(false);
const removeSelection = ref(false);
const removeIds = ref<string[]>([]);
// Provider ids are long and opaque ("VOuZtCdxdziwqM1mQ8MUq7G7A1"), so tasks
// queued before the destination existed fall back to a shortened path instead of
// showing that noise to the user.
const opaqueSegment = /^[A-Za-z0-9_-]{20,}$/;
function shortenPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  if (!opaqueSegment.test(parts[0] ?? "")) return path;
  return `…/${parts.pop() ?? ""}`;
}
function destinationLabel(task: UploadTask): string {
  // Cloud providers key folders by opaque ids, so the queued directory is the
  // only human readable hint we get. The remote name may differ from the local
  // one when the provider de-duplicates, hence taking it from the remote path.
  const name = task.remotePath.split("/").filter(Boolean).pop() ?? task.name;
  if (!task.destination) return shortenPath(task.remotePath);
  const directory = task.destination.replace(/\/+$/, "");
  return `${directory || "/"}/${name}`;
}
function groupDestination(group: Group): string {
  const first = group.tasks.find((task) => task.destination) ?? group.tasks[0];
  if (!first) return group.path || "";
  if (first.destination) {
    const directory = first.destination.replace(/\/+$/, "");
    return `${directory || "/"}/${group.name}`;
  }
  return opaqueSegment.test(first.remotePath.split("/").filter(Boolean)[0] ?? "")
    ? `…/${group.name}`
    : group.path || "";
}
const labels: Record<string, string> = {
  waiting: "排队中",
  hashing: "校验本地文件",
  active: "上传中",
  merging: "云端合并中",
  paused: "已暂停",
  complete: "已完成",
  error: "上传失败",
};
const runningStatuses = ["waiting", "hashing", "active", "merging"];
const running = (t: UploadTask) => runningStatuses.includes(t.status);
const matchesTab = (status: string) =>
  filter.value === "all" ||
  (filter.value === "active" && runningStatuses.includes(status)) ||
  status === filter.value;
const groupStatus = (statuses: string[]) => {
  if (!statuses.length) return "waiting";
  if (statuses.every((status) => status === "complete")) return "complete";
  for (const status of [
    "merging",
    "hashing",
    "active",
    "waiting",
    "error",
    "paused",
  ])
    if (statuses.includes(status)) return status;
  return "waiting";
};
const buildGroup = (batch: Batch, tasks: UploadTask[]): Group => {
  const size = tasks.reduce((sum, t) => sum + t.size, 0);
  const completed = tasks.reduce((sum, t) => sum + t.completed, 0);
  const status = groupStatus(tasks.map((t) => t.status));
  return {
    id: batch.id,
    name: batch.name,
    path: batch.path || "",
    tasks,
    size,
    completed,
    speed: tasks.reduce((sum, t) => sum + t.speed, 0),
    status,
    done: tasks.filter((t) => t.status === "complete").length,
    total: tasks.length,
    progress:
      status === "complete"
        ? 100
        : size
          ? Math.min(99.9, (completed / size) * 100)
          : 0,
  };
};
const rows = computed<Row[]>(() => {
  const groups = new Map<string, UploadTask[]>();
  for (const task of props.tasks) {
    const id = task.batch?.id;
    if (!id) continue;
    const bucket = groups.get(id) ?? [];
    bucket.push(task);
    groups.set(id, bucket);
  }
  const out: Row[] = [];
  const emitted = new Set<string>();
  for (const task of props.tasks) {
    const batch = task.batch;
    if (!batch?.id) {
      if (matchesTab(task.status))
        out.push({ kind: "task", key: task.id, task, child: false });
      continue;
    }
    if (emitted.has(batch.id)) continue;
    emitted.add(batch.id);
    const children = groups.get(batch.id) ?? [task];
    const group = buildGroup(batch, children);
    if (!matchesTab(group.status)) continue;
    out.push({ kind: "group", key: batch.id, group });
    if (!expanded.value.includes(batch.id)) continue;
    for (const child of children) {
      if (matchesTab(child.status))
        out.push({
          kind: "task",
          key: `${batch.id}:${child.id}`,
          task: child,
          child: true,
        });
    }
  }
  return out;
});
const topLevelTasks = computed(() =>
  rows.value.flatMap((row) =>
    row.kind === "group" ? row.group.tasks : [row.task],
  ),
);
const completed = computed(() =>
  rows.value.flatMap((row) =>
    row.kind === "group"
      ? row.group.status === "complete"
        ? row.group.tasks
        : []
      : row.task.status === "complete"
        ? [row.task]
        : [],
  ),
);
const speed = computed(() =>
  topLevelTasks.value.reduce((n, t) => n + t.speed, 0),
);
const remaining = (t: UploadTask) => {
  const seconds = Math.max(1, Math.ceil((t.size - t.completed) / t.speed));
  return seconds < 60
    ? `约 ${seconds} 秒`
    : seconds < 3600
      ? `约 ${Math.ceil(seconds / 60)} 分钟`
      : `约 ${(seconds / 3600).toFixed(1)} 小时`;
};
const selectedSet = computed(() => new Set(selected.value));
const allSelected = computed(
  () =>
    topLevelTasks.value.length > 0 &&
    topLevelTasks.value.every((t) => selectedSet.value.has(t.id)),
);
const someSelected = computed(
  () =>
    !allSelected.value &&
    topLevelTasks.value.some((t) => selectedSet.value.has(t.id)),
);
const groupSelected = (group: Group): "none" | "some" | "all" => {
  const count = group.tasks.filter((t) => selectedSet.value.has(t.id)).length;
  if (count === group.tasks.length) return "all";
  return count ? "some" : "none";
};
// A selection belongs to the list the user is looking at.
watch(filter, () => (selected.value = []));
watch(
  () => props.tasks,
  (tasks) => {
    const ids = new Set(tasks.map((t) => t.id));
    const batches = new Set(
      tasks.map((t) => t.batch?.id).filter((id): id is string => !!id),
    );
    selected.value = selected.value.filter((id) => ids.has(id));
    expanded.value = expanded.value.filter((id) => batches.has(id));
  },
);
function toggleTask(id: string) {
  selected.value = selected.value.includes(id)
    ? selected.value.filter((current) => current !== id)
    : [...selected.value, id];
}
function toggleGroupSelection(group: Group) {
  const ids = group.tasks.map((t) => t.id);
  const all = groupSelected(group) === "all";
  selected.value = all
    ? selected.value.filter((id) => !ids.includes(id))
    : [...new Set([...selected.value, ...ids])];
}
function toggleGroup(id: string) {
  expanded.value = expanded.value.includes(id)
    ? expanded.value.filter((current) => current !== id)
    : [...expanded.value, id];
}
function toggleSelectAll() {
  const ids = topLevelTasks.value.map((t) => t.id);
  selected.value = allSelected.value
    ? selected.value.filter((id) => !ids.includes(id))
    : [...new Set([...selected.value, ...ids])];
}
async function action(t: UploadTask, action: string) {
  busy.value = t.id;
  try {
    await invoke("upload_action", { id: t.id, action });
    emit("changed");
  } catch (e) {
    emit("error", String(e));
  } finally {
    busy.value = "";
  }
}
const actionLabels: Record<string, string> = {
  pause: "暂停",
  resume: "继续",
  remove: "移除",
};
async function runBatch(ids: string[], action: string) {
  if (!ids.length || batchBusy.value) return;
  batchBusy.value = true;
  try {
    const report = await invoke<{
      done: number;
      failed: { id: string; message: string }[];
    }>("upload_batch_action", { ids, action });
    const skipped = ids.length - report.done - report.failed.length;
    const verb = actionLabels[action] || action;
    if (report.done) {
      emit(
        "notice",
        `已${verb} ${report.done} 个上传任务${skipped ? `，${skipped} 个任务当前无需${verb}` : ""}`,
      );
    } else if (skipped || report.failed.length) {
      emit("notice", `所选任务当前无需${verb}`);
    }
    if (report.failed.length) emit("error", report.failed[0].message);
    selected.value = [];
    emit("changed");
  } catch (e) {
    emit("error", String(e));
  } finally {
    batchBusy.value = false;
  }
}
function clearCompleted() {
  void runBatch(
    completed.value.map((t) => t.id),
    "remove",
  );
}
function askRemove(ids: string[]) {
  if (!ids.length) return;
  removeIds.value = ids;
  removeSelection.value = true;
}
const pauseIds = (group: Group) =>
  group.tasks
    .filter((t) => ["waiting", "hashing", "active"].includes(t.status))
    .map((t) => t.id);
const resumeIds = (group: Group) =>
  group.tasks
    .filter((t) => ["paused", "error"].includes(t.status))
    .map((t) => t.id);
async function remove() {
  const task = removeTarget.value;
  removeTarget.value = null;
  if (task) await action(task, "remove");
}
async function removeSelected() {
  const ids = [...removeIds.value];
  removeSelection.value = false;
  removeIds.value = [];
  await runBatch(ids, "remove");
}
</script>
<template>
  <div class="cloud-page-toolbar">
    <div class="segmented">
      <button
        v-for="[key, label] in [
          ['all', '全部'],
          ['active', '下载中'],
          ['complete', '已完成'],
          ['paused', '已暂停'],
          ['error', '失败'],
        ]"
        :key="key"
        :class="{ selected: filter === key }"
        @click="filter = key!"
      >
        {{ label }}
      </button>
    </div>
    <span v-if="speed" class="speed-label"
      ><ArrowUpFromLine :size="15" />{{ formatBytes(speed) }}/s</span
    ><button class="primary" @click="emit('browse')">
      <Folder :size="16" />选择上传位置
    </button>
  </div>
  <section class="task-list card">
    <div
      v-if="rows.length"
      class="task-list-head upload-head"
      :class="{ 'head-selected': selected.length }"
    >
      <label class="task-select-all" :title="allSelected ? '取消全选' : '全选'">
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
            @click="askRemove(selected)"
          >
            <Trash2 :size="15" />移除
          </button>
          <LoaderCircle v-if="batchBusy" :size="15" class="spinning" />
        </div>
      </template>
      <span v-else class="head-label">上传任务</span>
      <span class="batch-spacer" />
      <button
        v-if="completed.length && filter === 'complete'"
        class="text-button task-clear"
        :disabled="batchBusy"
        @click="clearCompleted"
      >
        清除已完成
      </button>
    </div>
    <div v-if="!rows.length" class="empty-state">
      <span class="empty-illustration"><ArrowUpFromLine :size="40" /></span>
      <h3>{{ tasks.length ? "这里没有匹配的任务" : "把本地文件带到云端" }}</h3>
      <p>在「我的网盘」中打开目标文件夹，然后选择上传文件。</p>
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
            groupSelected(row.group) === 'all' ? '取消选择文件夹' : '选择文件夹'
          "
        >
          <input
            type="checkbox"
            :checked="groupSelected(row.group) === 'all'"
            :indeterminate="groupSelected(row.group) === 'some'"
            :disabled="batchBusy"
            @change="toggleGroupSelection(row.group)"
          />
        </label>
        <button
          class="task-expand"
          :title="
            expanded.includes(row.group.id) ? '收起文件列表' : '展开文件列表'
          "
          :aria-expanded="expanded.includes(row.group.id)"
          @click="toggleGroup(row.group.id)"
        >
          <ChevronRight
            :size="15"
            :class="{ open: expanded.includes(row.group.id) }"
          />
        </button>
        <div class="task-file-icon folder"><Folder :size="24" /></div>
        <div class="task-details">
          <div class="task-name-row">
            <strong :title="row.group.path || row.group.name">{{
              row.group.name
            }}</strong
            ><span class="status-pill" :class="row.group.status"
              ><i></i>{{ labels[row.group.status] || row.group.status }}</span
            >
          </div>
          <div class="task-progress" :class="row.group.status">
            <div :style="{ width: `${row.group.progress}%` }"></div>
          </div>
          <div class="task-meta">
            <span class="provider-tag"
              >文件夹 · {{ row.group.total }} 个文件</span
            >
            <span
              >{{ formatBytes(row.group.completed) }} /
              {{
                row.group.size ? formatBytes(row.group.size) : "未知大小"
              }}</span
            >
            <span>{{ row.group.done }} / {{ row.group.total }} 已完成</span>
          </div>
          <p v-if="groupDestination(row.group)" class="upload-destination">
            上传到 {{ groupDestination(row.group) }}
          </p>
        </div>
        <div v-if="row.group.speed" class="task-rate">
          <strong>{{ formatBytes(row.group.speed) }}<small>/s</small></strong>
          <span>文件夹总速度</span>
        </div>
        <div class="task-actions">
          <LoaderCircle v-if="batchBusy" :size="19" class="spinning" />
          <template v-else>
            <button
              v-if="pauseIds(row.group).length"
              class="icon-button task-main-action"
              title="暂停文件夹上传"
              aria-label="暂停文件夹上传"
              @click="runBatch(pauseIds(row.group), 'pause')"
            >
              <Pause :size="17" /></button
            ><button
              v-if="resumeIds(row.group).length"
              class="icon-button task-main-action"
              title="继续文件夹上传"
              aria-label="继续文件夹上传"
              @click="runBatch(resumeIds(row.group), 'resume')"
            >
              <Play :size="17" /></button
            ><button
              class="icon-button"
              title="移除文件夹上传任务"
              aria-label="移除文件夹上传任务"
              @click="askRemove(row.group.tasks.map((t) => t.id))"
            >
              <Trash2 :size="17" />
            </button>
          </template>
        </div>
      </article>
      <article v-else class="task-row" :class="{ 'task-child-row': row.child }">
        <label
          class="task-select"
          :title="selected.includes(row.task.id) ? '取消选择' : '选择任务'"
        >
          <input
            type="checkbox"
            :checked="selected.includes(row.task.id)"
            @change="toggleTask(row.task.id)"
          />
        </label>
        <div class="task-file-icon"><File :size="25" /></div>
        <div class="task-details">
          <div class="task-name-row">
            <strong :title="row.task.localPath">{{ row.task.name }}</strong
            ><span class="status-pill" :class="row.task.status"
              ><i />{{ labels[row.task.status] || row.task.status }}</span
            >
          </div>
          <div class="task-progress" :class="row.task.status">
            <div
              :style="{
                width: `${row.task.status === 'complete' ? 100 : row.task.size ? Math.min(99.9, (row.task.completed / row.task.size) * 100) : 0}%`,
              }"
            />
          </div>
          <div class="task-meta upload-meta">
            <span class="provider-tag">{{
              providerName(row.task.provider as Provider)
            }}</span
            ><span
              >{{ formatBytes(row.task.completed) }} /
              {{ formatBytes(row.task.size) }}</span
            ><span>{{ row.task.concurrency }} 路并发</span
            ><span v-if="row.task.totalParts"
              >分片 {{ row.task.completedParts }} /
              {{ row.task.totalParts }}</span
            ><span v-if="row.task.speed"
              >{{ formatBytes(row.task.speed) }}/s ·
              {{ remaining(row.task) }}</span
            >
          </div>
          <p class="upload-destination" :title="destinationLabel(row.task)">
            上传到 {{ destinationLabel(row.task) }}
          </p>
          <p v-if="row.task.error" class="task-error">{{ row.task.error }}</p>
        </div>
        <div class="task-actions">
          <button
            v-if="running(row.task)"
            class="icon-button"
            :disabled="busy === row.task.id || row.task.status === 'merging'"
            :title="
              row.task.status === 'merging'
                ? '正在提交云端文件，请稍候'
                : '暂停上传'
            "
            @click="action(row.task, 'pause')"
          >
            <Pause :size="18" /></button
          ><button
            v-if="['paused', 'error'].includes(row.task.status)"
            class="icon-button"
            :disabled="busy === row.task.id"
            :title="row.task.status === 'error' ? '重试上传' : '继续上传'"
            @click="action(row.task, 'resume')"
          >
            <RefreshCw v-if="row.task.status === 'error'" :size="18" /><Play
              v-else
              :size="18"
            /></button
          ><button
            v-if="!running(row.task)"
            class="icon-button"
            title="移除上传记录"
            :disabled="busy === row.task.id"
            @click="removeTarget = row.task"
          >
            <Trash2 :size="18" />
          </button>
        </div>
      </article>
    </template>
  </section>
  <p class="bottom-note">
    进度按已确认的分片计算。重新启动后，未完成的上传保持暂停。
  </p>
  <Teleport to="body"
    ><div v-if="removeTarget" class="modal-backdrop">
      <div class="confirm-dialog">
        <h2>移除上传记录？</h2>
        <p>{{ removeTarget.name }}<br />已经上传到网盘的文件会保留。</p>
        <div class="button-row">
          <button class="secondary" @click="removeTarget = null">取消</button
          ><button class="danger" @click="remove">移除记录</button>
        </div>
      </div>
    </div>
    <div v-else-if="removeSelection" class="modal-backdrop">
      <div class="confirm-dialog">
        <h2>移除所选上传任务？</h2>
        <p>
          共
          {{
            removeIds.length
          }}
          个任务会从列表中移除。<br />已经上传到网盘的文件会保留。
        </p>
        <div class="button-row">
          <button class="secondary" @click="removeSelection = false">
            取消</button
          ><button class="danger" @click="removeSelected">移除记录</button>
        </div>
      </div>
    </div></Teleport
  >
</template>
