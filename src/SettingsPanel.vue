<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { version } from "../package.json";
import {
  CircleAlert,
  CloudDownload,
  Download,
  ExternalLink,
  FolderOpen,
  LoaderCircle,
  RefreshCw,
  RotateCw,
  Sparkles,
  Check,
  ChevronDown,
  ChevronUp,
  Sun,
  Moon,
} from "@lucide/vue";
import {
  type Settings,
  connectionChoices,
  limitPresets,
  maxConcurrentDownloads,
  maxDownloadLimit,
  maxSplitMib,
  maxUploadChunkMib,
  maxUploadConcurrency,
  maxUploadPartConcurrency,
  minSplitMib,
  minUploadChunkMib,
  minUploadConcurrency,
  minUploadPartConcurrency,
  uploadChunkChoices,
} from "./settings";
import { theme, setTheme } from "./theme";
import {
  checkForUpdates,
  installUpdate,
  restartForUpdate,
  updateStatus,
} from "./updater";

const props = defineProps<{ settings: Settings }>();
const emit = defineEmits<{
  saved: [settings: Settings];
  directory: [];
  folder: [];
  notice: [message: string];
  error: [message: string];
}>();

// 后端以 KiB/s 存储限速；界面以 MiB/s 展示与输入（与预设标签一致）。
const toMib = (kib: number) => Math.round((kib / 1024) * 100) / 100;
const maxLimitMib = Math.round((maxDownloadLimit / 1024) * 100) / 100;

const draft = ref<Settings>({ ...props.settings });
const limitInput = ref(toMib(props.settings.maxOverallDownloadLimit));
const concurrentInput = ref(props.settings.maxConcurrentDownloads);
const uploadChunkInput = ref(props.settings.uploadChunkMib);
const uploadConcurrentInput = ref(props.settings.uploadConcurrency);
const uploadPartConcurrentInput = ref(props.settings.uploadPartConcurrency);
const minSplitInput = ref(props.settings.minSplitMib);
const saving = ref(false);

function syncInputs(value: Settings) {
  draft.value = { ...value };
  limitInput.value = toMib(value.maxOverallDownloadLimit);
  concurrentInput.value = value.maxConcurrentDownloads;
  uploadChunkInput.value = value.uploadChunkMib;
  uploadConcurrentInput.value = value.uploadConcurrency;
  uploadPartConcurrentInput.value = value.uploadPartConcurrency;
  minSplitInput.value = value.minSplitMib;
}

function sameSettings(a: Settings, b: Settings) {
  return (
    a.downloadDir === b.downloadDir &&
    a.maxOverallDownloadLimit === b.maxOverallDownloadLimit &&
    a.maxConcurrentDownloads === b.maxConcurrentDownloads &&
    a.defaultConnections === b.defaultConnections &&
    a.closeAction === b.closeAction &&
    a.autoCheckUpdates === b.autoCheckUpdates &&
    a.uploadChunkMib === b.uploadChunkMib &&
    a.uploadConcurrency === b.uploadConcurrency &&
    a.uploadPartConcurrency === b.uploadPartConcurrency &&
    a.minSplitMib === b.minSplitMib
  );
}

// 后台快照（tasks-changed 等）会反复替换 props.settings。只有值真的变了才回填
// 输入框，这样下载/上传进行时的刷新不会把正在编辑的数值重置；而切换下载目录等
// 真实变更仍能同步过来。
watch(
  () => props.settings,
  (value) => {
    if (saving.value) return;
    if (!sameSettings(value, draft.value)) syncInputs(value);
  },
);

async function apply(patch: Partial<Settings>) {
  const next = { ...draft.value, ...patch };
  draft.value = next;
  saving.value = true;
  try {
    const saved = await invoke<Settings>("settings_set", { settings: next });
    syncInputs(saved);
    emit("saved", saved);
  } catch (error) {
    syncInputs(props.settings);
    emit("error", String(error));
  } finally {
    saving.value = false;
  }
}

function applyLimit() {
  const parsedMib = Number(limitInput.value) || 0;
  const clampedMib = Math.min(Math.max(parsedMib, 0), maxLimitMib);
  limitInput.value = clampedMib;
  const kib = Math.round(clampedMib * 1024);
  if (kib !== draft.value.maxOverallDownloadLimit) {
    void apply({ maxOverallDownloadLimit: kib });
  }
}

function setLimit(kib: number) {
  limitInput.value = toMib(kib);
  if (kib !== draft.value.maxOverallDownloadLimit) {
    void apply({ maxOverallDownloadLimit: kib });
  }
}

function applyConcurrent() {
  const parsed = Math.round(Number(concurrentInput.value) || 0);
  const clamped = Math.min(Math.max(parsed, 1), maxConcurrentDownloads);
  concurrentInput.value = clamped;
  if (clamped !== draft.value.maxConcurrentDownloads) {
    void apply({ maxConcurrentDownloads: clamped });
  }
}

function applyUploadChunk() {
  const parsed = Math.round(Number(uploadChunkInput.value) || 0);
  const clamped = Math.min(Math.max(parsed, minUploadChunkMib), maxUploadChunkMib);
  uploadChunkInput.value = clamped;
  if (clamped !== draft.value.uploadChunkMib) {
    void apply({ uploadChunkMib: clamped });
  }
}

function applyUploadConcurrent() {
  const parsed = Math.round(Number(uploadConcurrentInput.value) || 0);
  const clamped = Math.min(
    Math.max(parsed, minUploadConcurrency),
    maxUploadConcurrency,
  );
  uploadConcurrentInput.value = clamped;
  if (clamped !== draft.value.uploadConcurrency) {
    void apply({ uploadConcurrency: clamped });
  }
}

function applyUploadPartConcurrent() {
  const parsed = Math.round(Number(uploadPartConcurrentInput.value) || 0);
  const clamped = Math.min(
    Math.max(parsed, minUploadPartConcurrency),
    maxUploadPartConcurrency,
  );
  uploadPartConcurrentInput.value = clamped;
  if (clamped !== draft.value.uploadPartConcurrency) {
    void apply({ uploadPartConcurrency: clamped });
  }
}

function applyMinSplit() {
  const parsed = Math.round(Number(minSplitInput.value) || 0);
  const clamped = Math.min(Math.max(parsed, minSplitMib), maxSplitMib);
  minSplitInput.value = clamped;
  if (clamped !== draft.value.minSplitMib) {
    void apply({ minSplitMib: clamped });
  }
}

type StepperField =
  | "limit"
  | "concurrent"
  | "minSplit"
  | "uploadChunk"
  | "uploadConcurrent"
  | "uploadPartConcurrent";

type StepConfig = {
  value: () => number;
  store: (next: number) => void;
  min: number;
  max: number;
  step: number;
  apply: () => void;
};

function stepConfig(field: StepperField): StepConfig {
  switch (field) {
    case "limit":
      return {
        value: () => limitInput.value,
        store: (next) => (limitInput.value = next),
        min: 0,
        max: maxLimitMib,
        step: 0.5,
        apply: applyLimit,
      };
    case "concurrent":
      return {
        value: () => concurrentInput.value,
        store: (next) => (concurrentInput.value = next),
        min: 1,
        max: maxConcurrentDownloads,
        step: 1,
        apply: applyConcurrent,
      };
    case "minSplit":
      return {
        value: () => minSplitInput.value,
        store: (next) => (minSplitInput.value = next),
        min: minSplitMib,
        max: maxSplitMib,
        step: 1,
        apply: applyMinSplit,
      };
    case "uploadChunk":
      return {
        value: () => uploadChunkInput.value,
        store: (next) => (uploadChunkInput.value = next),
        min: minUploadChunkMib,
        max: maxUploadChunkMib,
        step: 1,
        apply: applyUploadChunk,
      };
    case "uploadConcurrent":
      return {
        value: () => uploadConcurrentInput.value,
        store: (next) => (uploadConcurrentInput.value = next),
        min: minUploadConcurrency,
        max: maxUploadConcurrency,
        step: 1,
        apply: applyUploadConcurrent,
      };
    case "uploadPartConcurrent":
      return {
        value: () => uploadPartConcurrentInput.value,
        store: (next) => (uploadPartConcurrentInput.value = next),
        min: minUploadPartConcurrency,
        max: maxUploadPartConcurrency,
        step: 1,
        apply: applyUploadPartConcurrent,
      };
  }
}

// 输入框右侧的悬浮箭头：按各字段自己的步进值加减，再走一遍原有的钳制与保存逻辑。
// 返回是否真的移动了（到边界时停下，按住不放不会持续写配置）。
function bump(field: StepperField, direction: 1 | -1, steps = 1): boolean {
  const config = stepConfig(field);
  const clamp = (value: number, min: number, max: number) =>
    Math.min(Math.max(value, min), max);
  const current = clamp(Number(config.value()) || config.min, config.min, config.max);
  const next = clamp(current + direction * config.step * steps, config.min, config.max);
  const changed = next !== config.value();
  config.store(next);
  config.apply();
  return changed;
}

let holdDelay: number | undefined;
let holdTimer: number | undefined;
let releaseHold: (() => void) | undefined;

function stopHold() {
  if (holdDelay !== undefined) {
    window.clearTimeout(holdDelay);
    holdDelay = undefined;
  }
  if (holdTimer !== undefined) {
    window.clearInterval(holdTimer);
    holdTimer = undefined;
  }
  releaseHold?.();
  releaseHold = undefined;
}

// 按住箭头连续加减：按下立即走一步，停留 420ms 后每 110ms 走一步，到边界自动停。
function beginHold(field: StepperField, direction: 1 | -1) {
  stopHold();
  holdDelay = window.setTimeout(() => {
    holdDelay = undefined;
    holdTimer = window.setInterval(() => {
      if (!bump(field, direction)) stopHold();
    }, 110);
  }, 420);
  const release = () => stopHold();
  window.addEventListener("pointerup", release, { once: true });
  window.addEventListener("pointercancel", release, { once: true });
  releaseHold = () => {
    window.removeEventListener("pointerup", release);
    window.removeEventListener("pointercancel", release);
  };
}

// 鼠标/触屏按下时先走一步；随后那次 click 的 detail 为 1，忽略它，避免一次按下走两步。
function pressStep(field: StepperField, direction: 1 | -1) {
  bump(field, direction);
  beginHold(field, direction);
}

// 键盘（Enter/空格、按住连发）触发的 click 的 detail 为 0，在这里走一步。
function clickStep(event: MouseEvent, field: StepperField, direction: 1 | -1) {
  if (event.detail > 0) return;
  bump(field, direction);
}

onBeforeUnmount(stopHold);

const bytes = (n: number) => {
  if (!n) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB"];
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), 3);
  return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${units[i]}`;
};

const checkedAt = computed(() =>
  updateStatus.value.checkedAt
    ? new Date(updateStatus.value.checkedAt).toLocaleTimeString("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
      })
    : "",
);

const title = computed(() => {
  const status = updateStatus.value;
  if (status.phase === "checking") return "正在检查更新…";
  if (status.phase === "current") return "已是最新版本";
  if (status.phase === "available") return `发现新版本 v${status.version}`;
  if (status.phase === "downloading") return `正在下载 v${status.version}`;
  if (status.phase === "ready") return `v${status.version} 已就绪`;
  if (status.phase === "error") return status.error;
  return "尚未检查更新";
});

const hint = computed(() => {
  const status = updateStatus.value;
  if (status.phase === "current") return `上次检查 ${checkedAt.value}`;
  if (status.phase === "available")
    return status.publishedAt
      ? `发布于 ${status.publishedAt.slice(0, 10)}`
      : "建议更新以获得最新修复";
  if (status.phase === "downloading") {
    return status.total
      ? `${bytes(status.downloaded)} / ${bytes(status.total)}`
      : bytes(status.downloaded);
  }
  if (status.phase === "ready") return "重启后自动完成安装";
  if (status.phase === "error") return "可稍后重试，或到发布页手动下载";
  return "连网后检查 GitHub Releases 上是否有新版本";
});

const percent = computed(() => {
  const status = updateStatus.value;
  if (!status.total) return 0;
  return Math.min(100, Math.round((status.downloaded / status.total) * 100));
});

const busy = computed(
  () => saving.value || updateStatus.value.phase === "checking",
);

function install() {
  if (updateStatus.value.phase === "ready") return;
  void installUpdate();
}

async function restart() {
  try {
    await restartForUpdate();
  } catch (error) {
    emit("error", String(error));
  }
}

async function openDataDirectory() {
  try {
    await invoke("open_data_directory");
  } catch (error) {
    emit("error", String(error));
  }
}

function openRepository() {
  void openUrl("https://github.com/sclliang/CloudDownloader");
}
</script>

<template>
  <div class="settings">
    <section class="settings-group card">
      <div class="group-head">
        <h2>通用</h2>
        <p>下载位置与窗口行为</p>
      </div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>下载目录</strong>
          <small>新任务默认保存到这个文件夹</small>
          <span class="setting-path" :title="draft.downloadDir">{{
            draft.downloadDir || "尚未设置"
          }}</span>
        </div>
        <div class="row-actions">
          <button class="secondary" @click="emit('directory')">
            <FolderOpen :size="15" />更改
          </button>
          <button class="text-button" @click="emit('folder')">打开</button>
        </div>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>关闭窗口时</strong>
          <small>{{
            draft.closeAction === "tray"
              ? "窗口收进托盘，下载继续进行"
              : "退出程序会停止下载引擎，任务将在下次启动时暂停"
          }}</small>
        </div>
        <div class="segmented" role="group" aria-label="关闭窗口时的行为">
          <button
            :class="{ selected: draft.closeAction === 'tray' }"
            :aria-pressed="draft.closeAction === 'tray'"
            :disabled="saving"
            @click="apply({ closeAction: 'tray' })"
          >
            最小化到托盘
          </button>
          <button
            :class="{ selected: draft.closeAction === 'exit' }"
            :aria-pressed="draft.closeAction === 'exit'"
            :disabled="saving"
            @click="apply({ closeAction: 'exit' })"
          >
            退出程序
          </button>
        </div>
      </div>
    </section>

    <section class="settings-group card">
      <div class="group-head">
        <h2>传输</h2>
        <p>立即生效，进行中的任务不受影响</p>
      </div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>全局限速</strong>
          <small>0 表示不限制带宽</small>
        </div>
        <div class="row-actions">
          <span class="number-field">
            <input
              v-model.number="limitInput"
              class="number-input"
              type="number"
              min="0"
              :max="maxLimitMib"
              step="0.5"
              aria-label="全局下载限速（MiB/s）"
              @change="applyLimit"
              @keydown.enter="applyLimit"
              @keydown.page-up.prevent="bump('limit', 1, 5)"
              @keydown.page-down.prevent="bump('limit', -1, 5)"
            />
            <span class="number-stepper">
              <button
                type="button"
                :disabled="saving"
                aria-label="增加全局限速"
                @pointerdown="pressStep('limit', 1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'limit', 1)"
              >
                <ChevronUp :size="10" :stroke-width="2.4" />
              </button>
              <button
                type="button"
                :disabled="saving"
                aria-label="减少全局限速"
                @pointerdown="pressStep('limit', -1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'limit', -1)"
              >
                <ChevronDown :size="10" :stroke-width="2.4" />
              </button>
            </span>
          </span>
          <span class="unit">MiB/s</span>
        </div>
      </div>
      <div class="presets">
        <button
          v-for="preset in limitPresets"
          :key="preset.value"
          class="chip"
          :class="{ active: draft.maxOverallDownloadLimit === preset.value }"
          :disabled="saving"
          @click="setLimit(preset.value)"
        >
          {{ preset.label }}
        </button>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>同时下载数</strong>
          <small>同时进行的任务数量（1–{{ maxConcurrentDownloads }}）</small>
        </div>
        <div class="row-actions">
          <span class="number-field">
            <input
              v-model.number="concurrentInput"
              class="number-input"
              type="number"
              min="1"
              :max="maxConcurrentDownloads"
              step="1"
              aria-label="同时下载的任务数量"
              @change="applyConcurrent"
              @keydown.enter="applyConcurrent"
              @keydown.page-up.prevent="bump('concurrent', 1, 5)"
              @keydown.page-down.prevent="bump('concurrent', -1, 5)"
            />
            <span class="number-stepper">
              <button
                type="button"
                :disabled="saving"
                aria-label="增加同时下载数"
                @pointerdown="pressStep('concurrent', 1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'concurrent', 1)"
              >
                <ChevronUp :size="10" :stroke-width="2.4" />
              </button>
              <button
                type="button"
                :disabled="saving"
                aria-label="减少同时下载数"
                @pointerdown="pressStep('concurrent', -1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'concurrent', -1)"
              >
                <ChevronDown :size="10" :stroke-width="2.4" />
              </button>
            </span>
          </span>
          <span class="unit">个</span>
        </div>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>默认连接数</strong>
          <small>新任务的分段连接数；服务器不支持分段时自动降为 1</small>
        </div>
        <div class="segmented" role="group" aria-label="默认连接数">
          <button
            v-for="choice in connectionChoices"
            :key="choice"
            :class="{ selected: draft.defaultConnections === choice }"
            :aria-pressed="draft.defaultConnections === choice"
            :disabled="saving"
            @click="apply({ defaultConnections: choice })"
          >
            {{ choice }}
          </button>
        </div>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>最小分段大小</strong>
          <small
            >aria2 下载的最小分段（{{ minSplitMib }}–{{ maxSplitMib }} MiB）；越小分段越细，越大分段越少</small
          >
        </div>
        <div class="row-actions">
          <span class="number-field">
            <input
              v-model.number="minSplitInput"
              class="number-input"
              type="number"
              :min="minSplitMib"
              :max="maxSplitMib"
              step="1"
              aria-label="最小分段大小（MiB）"
              @change="applyMinSplit"
              @keydown.enter="applyMinSplit"
              @keydown.page-up.prevent="bump('minSplit', 1, 5)"
              @keydown.page-down.prevent="bump('minSplit', -1, 5)"
            />
            <span class="number-stepper">
              <button
                type="button"
                :disabled="saving"
                aria-label="增加最小分段大小"
                @pointerdown="pressStep('minSplit', 1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'minSplit', 1)"
              >
                <ChevronUp :size="10" :stroke-width="2.4" />
              </button>
              <button
                type="button"
                :disabled="saving"
                aria-label="减少最小分段大小"
                @pointerdown="pressStep('minSplit', -1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'minSplit', -1)"
              >
                <ChevronDown :size="10" :stroke-width="2.4" />
              </button>
            </span>
          </span>
          <span class="unit">MiB</span>
        </div>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>上传分片大小</strong>
          <small>
            迅雷 / 夸克上传的分片粒度（{{ minUploadChunkMib }}–{{
              maxUploadChunkMib
            }}
            MiB）；越大多分片开销越少，越小断点续传越细。百度上传按协议固定 4 MiB。
          </small>
        </div>
        <div class="row-actions">
          <span class="number-field">
            <input
              v-model.number="uploadChunkInput"
              class="number-input"
              type="number"
              :min="minUploadChunkMib"
              :max="maxUploadChunkMib"
              step="1"
              aria-label="上传分片大小（MiB）"
              @change="applyUploadChunk"
              @keydown.enter="applyUploadChunk"
              @keydown.page-up.prevent="bump('uploadChunk', 1, 5)"
              @keydown.page-down.prevent="bump('uploadChunk', -1, 5)"
            />
            <span class="number-stepper">
              <button
                type="button"
                :disabled="saving"
                aria-label="增加上传分片大小"
                @pointerdown="pressStep('uploadChunk', 1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'uploadChunk', 1)"
              >
                <ChevronUp :size="10" :stroke-width="2.4" />
              </button>
              <button
                type="button"
                :disabled="saving"
                aria-label="减少上传分片大小"
                @pointerdown="pressStep('uploadChunk', -1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'uploadChunk', -1)"
              >
                <ChevronDown :size="10" :stroke-width="2.4" />
              </button>
            </span>
          </span>
          <span class="unit">MiB</span>
        </div>
      </div>
      <div class="presets">
        <button
          v-for="choice in uploadChunkChoices"
          :key="choice"
          class="chip"
          :class="{ active: draft.uploadChunkMib === choice }"
          :disabled="saving"
          @click="apply({ uploadChunkMib: choice })"
        >
          {{ choice }} MiB
        </button>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>同时上传数</strong>
          <small
            >同时进行的上传任务数量（{{ minUploadConcurrency }}–{{
              maxUploadConcurrency
            }}）</small
          >
        </div>
        <div class="row-actions">
          <span class="number-field">
            <input
              v-model.number="uploadConcurrentInput"
              class="number-input"
              type="number"
              :min="minUploadConcurrency"
              :max="maxUploadConcurrency"
              step="1"
              aria-label="同时上传的任务数量"
              @change="applyUploadConcurrent"
              @keydown.enter="applyUploadConcurrent"
              @keydown.page-up.prevent="bump('uploadConcurrent', 1, 5)"
              @keydown.page-down.prevent="bump('uploadConcurrent', -1, 5)"
            />
            <span class="number-stepper">
              <button
                type="button"
                :disabled="saving"
                aria-label="增加同时上传数"
                @pointerdown="pressStep('uploadConcurrent', 1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'uploadConcurrent', 1)"
              >
                <ChevronUp :size="10" :stroke-width="2.4" />
              </button>
              <button
                type="button"
                :disabled="saving"
                aria-label="减少同时上传数"
                @pointerdown="pressStep('uploadConcurrent', -1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'uploadConcurrent', -1)"
              >
                <ChevronDown :size="10" :stroke-width="2.4" />
              </button>
            </span>
          </span>
          <span class="unit">个</span>
        </div>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>单文件分片并发</strong>
          <small
            >每个文件同时上传的分片数上限（{{ minUploadPartConcurrency }}–{{
              maxUploadPartConcurrency
            }}）</small
          >
        </div>
        <div class="row-actions">
          <span class="number-field">
            <input
              v-model.number="uploadPartConcurrentInput"
              class="number-input"
              type="number"
              :min="minUploadPartConcurrency"
              :max="maxUploadPartConcurrency"
              step="1"
              aria-label="单文件分片并发"
              @change="applyUploadPartConcurrent"
              @keydown.enter="applyUploadPartConcurrent"
              @keydown.page-up.prevent="bump('uploadPartConcurrent', 1, 5)"
              @keydown.page-down.prevent="bump('uploadPartConcurrent', -1, 5)"
            />
            <span class="number-stepper">
              <button
                type="button"
                :disabled="saving"
                aria-label="增加单文件分片并发"
                @pointerdown="pressStep('uploadPartConcurrent', 1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'uploadPartConcurrent', 1)"
              >
                <ChevronUp :size="10" :stroke-width="2.4" />
              </button>
              <button
                type="button"
                :disabled="saving"
                aria-label="减少单文件分片并发"
                @pointerdown="pressStep('uploadPartConcurrent', -1)"
                @pointerleave="stopHold"
                @click="clickStep($event, 'uploadPartConcurrent', -1)"
              >
                <ChevronDown :size="10" :stroke-width="2.4" />
              </button>
            </span>
          </span>
          <span class="unit">个</span>
        </div>
      </div>
    </section>

    <section class="settings-group card">
      <div class="group-head">
        <h2>外观</h2>
        <p>主题会随窗口一起切换</p>
      </div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>界面主题</strong>
          <small>日间清爽，Night 适合弱光环境</small>
        </div>
        <div class="segmented" role="group" aria-label="界面主题">
          <button
            :class="{ selected: theme === 'light' }"
            :aria-pressed="theme === 'light'"
            @click="setTheme('light')"
          >
            <Sun :size="15" />日间
          </button>
          <button
            :class="{ selected: theme === 'night' }"
            :aria-pressed="theme === 'night'"
            @click="setTheme('night')"
          >
            <Moon :size="15" />Night
          </button>
        </div>
      </div>
    </section>

    <section class="settings-group card">
      <div class="group-head">
        <h2>关于与更新</h2>
        <p>CloudDownloader v{{ version }} · Windows 10/11 x64</p>
      </div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>自动检查更新</strong>
          <small>启动时静默检查，发现新版本会在「设置」上标记</small>
        </div>
        <button
          class="switch"
          role="switch"
          :aria-checked="draft.autoCheckUpdates"
          aria-label="自动检查更新"
          :disabled="saving"
          @click="apply({ autoCheckUpdates: !draft.autoCheckUpdates })"
        >
          <span></span>
        </button>
      </div>
      <div class="update-box" :class="updateStatus.phase">
        <div class="update-main">
          <LoaderCircle
            v-if="
              updateStatus.phase === 'checking' ||
              updateStatus.phase === 'downloading'
            "
            :size="18"
            class="spinning"
          />
          <CircleAlert v-else-if="updateStatus.phase === 'error'" :size="18" />
          <Check v-else-if="updateStatus.phase === 'current'" :size="18" />
          <Sparkles v-else-if="updateStatus.phase === 'available'" :size="18" />
          <CloudDownload v-else-if="updateStatus.phase === 'ready'" :size="18" />
          <RefreshCw v-else :size="18" />
          <div>
            <strong>{{ title }}</strong>
            <small>{{ hint }}</small>
          </div>
        </div>
        <div class="update-actions">
          <button
            v-if="updateStatus.phase !== 'available' && updateStatus.phase !== 'ready'"
            class="secondary"
            :disabled="busy || updateStatus.phase === 'downloading'"
            @click="checkForUpdates()"
          >
            <RefreshCw
              :size="15"
              :class="{ spinning: updateStatus.phase === 'checking' }"
            />{{ updateStatus.phase === "current" ? "重新检查" : "检查更新" }}
          </button>
          <button
            v-else-if="updateStatus.phase === 'available'"
            class="primary"
            :disabled="updateStatus.phase !== 'available'"
            @click="install"
          >
            <Download :size="15" />下载并安装
          </button>
          <button v-else class="primary" @click="restart">
            <RotateCw :size="15" />立即重启
          </button>
          <button
            v-if="updateStatus.phase === 'available'"
            class="text-button"
            @click="checkForUpdates()"
          >
            稍后
          </button>
        </div>
        <div
          v-if="updateStatus.phase === 'downloading'"
          class="update-progress"
        >
          <span :style="{ width: `${percent}%` }"></span>
        </div>
        <p
          v-if="updateStatus.notes && updateStatus.phase === 'available'"
          class="release-notes"
        >
          {{ updateStatus.notes }}
        </p>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>本地数据</strong>
          <small>账号配置、任务数据库与种子缓存</small>
        </div>
        <button class="secondary" @click="openDataDirectory">
          <FolderOpen :size="15" />打开目录
        </button>
      </div>
      <div class="divider"></div>
      <div class="setting-row">
        <div class="setting-copy">
          <strong>开源仓库</strong>
          <small>问题反馈与版本说明</small>
        </div>
        <button class="secondary" @click="openRepository">
          <ExternalLink :size="15" />sclliang/CloudDownloader
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.settings {
  columns: 2;
  column-gap: 16px;
  padding-bottom: 12px;
}
.settings-group {
  break-inside: avoid;
  margin-bottom: 16px;
  padding: 18px 20px 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
@media (max-width: 980px) {
  .settings {
    columns: 1;
  }
}
.group-head h2 {
  margin: 0 0 3px;
  font-size: 15px;
}
.group-head p {
  margin: 0;
  color: var(--muted);
  font-size: 12px;
}
.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
}
.setting-copy {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}
.setting-copy strong {
  font-size: 13px;
}
.setting-copy small {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.5;
}
.setting-path {
  color: var(--muted);
  font-size: 12px;
  font-family: Consolas, "Cascadia Mono", monospace;
  line-height: 1.5;
  overflow-wrap: anywhere;
}
.row-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
.divider {
  height: 1px;
  background: var(--line);
}
.segmented {
  display: inline-flex;
  gap: 2px;
  padding: 3px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--surface-soft);
  flex-shrink: 0;
}
.segmented button {
  padding: 6px 11px;
  border-radius: 8px;
  background: transparent;
  color: var(--muted);
  font-size: 12px;
}
.segmented button.selected {
  background: var(--surface);
  color: var(--ink);
  font-weight: 600;
  box-shadow: 0 1px 4px #182c4314;
}
.number-input {
  width: 100px;
  padding: 7px 10px;
  border: 1px solid var(--line);
  border-radius: 9px;
  background: var(--surface);
  text-align: center;
  appearance: textfield;
  -moz-appearance: textfield;
}
.number-input::-webkit-outer-spin-button,
.number-input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}
.number-field {
  position: relative;
  display: inline-flex;
}
.number-stepper {
  position: absolute;
  top: 4px;
  right: 5px;
  bottom: 4px;
  display: flex;
  flex-direction: column;
  width: 16px;
}
.number-stepper button {
  flex: 1;
  display: grid;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--muted);
  opacity: 0.5;
  cursor: pointer;
}
.number-stepper button:hover {
  background: var(--surface-hover);
  color: var(--ink);
  opacity: 1;
}
.number-stepper button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 0;
  opacity: 1;
}
.number-stepper button:disabled {
  opacity: 0.25;
  cursor: default;
  background: transparent;
}
.unit {
  color: var(--muted);
  font-size: 12px;
}
.presets {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: -6px;
}
.chip {
  padding: 4px 10px;
  border-radius: 999px;
  background: var(--surface-soft);
  color: var(--muted);
  font-size: 12px;
}
.chip.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}
.switch {
  width: 44px;
  height: 24px;
  padding: 2px;
  border-radius: 999px;
  background: var(--line);
  justify-content: flex-start;
  flex-shrink: 0;
}
.switch span {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px #182c4326;
  transition: transform 0.16s;
}
.switch[aria-checked="true"] {
  background: var(--accent);
}
.switch[aria-checked="true"] span {
  transform: translateX(20px);
}
.update-box {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface-soft);
}
.update-box.error {
  border-color: #f0c9c9;
  background: var(--danger-soft);
}
.update-box.available,
.update-box.ready {
  border-color: #cfe3fb;
  background: var(--accent-soft);
}
.update-main {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  color: var(--ink);
}
.update-main strong {
  display: block;
  font-size: 13px;
  line-height: 1.4;
}
.update-main small {
  color: var(--muted);
  font-size: 12px;
}
.update-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.update-progress {
  height: 4px;
  border-radius: 2px;
  background: var(--line);
  overflow: hidden;
}
.update-progress span {
  display: block;
  height: 100%;
  background: var(--accent);
  transition: width 0.2s;
}
.release-notes {
  margin: 0;
  max-height: 96px;
  overflow: auto;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.55;
  white-space: pre-wrap;
}
.spinning {
  animation: settings-spin 1s linear infinite;
}
@keyframes settings-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
