<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  CircleAlert,
  CircleCheck,
  FolderOpen,
  Puzzle,
  RefreshCw,
  X,
} from "@lucide/vue";

/** Mirrors `bridge::BridgeStatus`. */
type BridgeStatus = {
  listening: boolean;
  port: number;
  enabled: boolean;
  last: { site: string; ok: boolean; message: string; at: number } | null;
  seenAt: number | null;
  version: string;
};

const emit = defineEmits<{ close: []; notice: [message: string] }>();

const status = ref<BridgeStatus | null>(null);
const folder = ref("");
const checking = ref(false);

async function refresh() {
  checking.value = true;
  try {
    status.value = await invoke<BridgeStatus>("bridge_status");
    folder.value = await invoke<string>("extension_folder");
  } catch (error) {
    emit("notice", String(error));
  } finally {
    checking.value = false;
  }
}

function ago(seconds: number) {
  if (seconds < 60) return `${seconds} 秒`;
  if (seconds < 3600) return `${Math.round(seconds / 60)} 分钟`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)} 小时`;
  return `${Math.round(seconds / 86400)} 天`;
}

const age = computed(() => {
  const seen = status.value?.seenAt;
  return seen ? Math.max(0, Math.round(Date.now() / 1000 - seen)) : null;
});

const state = computed(() => {
  const s = status.value;
  if (!s)
    return {
      tone: "checking",
      title: "正在检测…",
      detail: "正在询问本机端口和扩展心跳",
    };
  if (!s.enabled)
    return {
      tone: "bad",
      title: "浏览器接力是关闭的",
      detail: "在「设置 → 浏览器接力」里打开后，网页下载才会交给这里",
    };
  if (!s.listening)
    return {
      tone: "bad",
      title: "本机端口没有打开",
      detail: `127.0.0.1:${s.port} 被别的程序占用，或桥启动失败`,
    };
  const seconds = age.value;
  if (seconds === null)
    return {
      tone: "wait",
      title: "还没检测到扩展",
      detail: "扩展装好并启用后，会每分钟向这里报一次到",
    };
  if (seconds <= 180)
    return {
      tone: "ok",
      title: `扩展正常工作${s.version ? `（v${s.version}）` : ""}`,
      detail: `${ago(seconds)}前收到扩展心跳`,
    };
  return {
    tone: "wait",
    title: "扩展已经装好，但没有再报到了",
    detail: `${ago(seconds)}前的最后一次心跳；浏览器在休眠、扩展被停用，或者端口被改过`,
  };
});

async function openFolder() {
  try {
    await invoke("open_extension_folder");
  } catch (error) {
    emit("notice", `打不开扩展目录：${String(error)}`);
  }
}

function copySteps() {
  const steps = [
    "1. 点击「打开扩展文件夹」，记下路径",
    "2. 打开 edge://extensions（Chrome 为 chrome://extensions），开启开发者模式",
    "3. 点「加载已解压的扩展程序」，选择刚才的文件夹",
    "4. 回到这里点「重新检测」，状态应变成扩展正常工作",
  ].join("\n");
  void navigator.clipboard.writeText(steps).then(
    () => emit("notice", "安装步骤已复制到剪贴板"),
    () => emit("notice", "复制失败，请手动按提示操作"),
  );
}

// Esc closes the panel and focus returns to the button that opened it, matching
// the account dialog; without it the only way out was the mouse.
let previousFocus: HTMLElement | null = null;
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}
onMounted(() => {
  previousFocus = document.activeElement as HTMLElement | null;
  document.addEventListener("keydown", onKeydown);
  void refresh();
});
onUnmounted(() => {
  document.removeEventListener("keydown", onKeydown);
  previousFocus?.focus();
});
</script>

<template>
  <div class="plugin-backdrop" @click.self="emit('close')">
    <section class="plugin-dialog" role="dialog" aria-label="浏览器扩展">
      <header>
        <span class="plugin-mark" :class="state.tone">
          <CircleCheck v-if="state.tone === 'ok'" :size="18" />
          <CircleAlert v-else-if="state.tone === 'bad'" :size="18" />
          <Puzzle v-else :size="18" />
        </span>
        <div>
          <h2>浏览器扩展</h2>
          <small>网页里的下载交给 CloudDownloader</small>
        </div>
        <button class="plugin-close" aria-label="关闭" @click="emit('close')">
          <X :size="16" />
        </button>
      </header>

      <div class="plugin-state" :class="state.tone">
        <strong>{{ state.title }}</strong>
        <small>{{ state.detail }}</small>
      </div>

      <div class="plugin-rows">
        <div class="plugin-row">
          <span>本机端口</span>
          <b>{{ status?.listening ? `127.0.0.1:${status.port}` : "未监听" }}</b>
        </div>
        <div class="plugin-row">
          <span>扩展版本</span>
          <b>{{ status?.version || "未知" }}</b>
        </div>
        <div class="plugin-row">
          <span>最近一次接力</span>
          <b>{{ status?.last ? status.last.site || "未知站点" : "还没有" }}</b>
        </div>
        <div class="plugin-row">
          <span>扩展目录</span>
          <b class="plugin-path" :title="folder">{{ folder || "未知" }}</b>
        </div>
      </div>

      <p class="plugin-hint">
        浏览器不允许外部程序自动装扩展，所以这里做的是「检测 +
        一键打开安装位置」：
        点下面的按钮打开扩展文件夹，再在浏览器的扩展页里选择它即可。
      </p>

      <footer>
        <button class="ghost" :disabled="checking" @click="refresh">
          <RefreshCw :size="14" /> 重新检测
        </button>
        <button class="ghost" @click="copySteps">复制安装步骤</button>
        <button class="primary" @click="openFolder">
          <FolderOpen :size="14" /> 打开扩展文件夹
        </button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.plugin-backdrop {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: grid;
  place-items: center;
  background: #0f1d3066;
  backdrop-filter: blur(2px);
}
.plugin-dialog {
  width: min(460px, calc(100vw - 48px));
  border-radius: 16px;
  border: 1px solid var(--line);
  background: var(--surface);
  box-shadow: 0 24px 60px #0f1d3033;
  padding: 18px 18px 16px;
}
.plugin-dialog header {
  display: flex;
  align-items: center;
  gap: 10px;
}
.plugin-mark {
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: var(--surface-soft);
  color: var(--accent);
}
.plugin-mark.ok {
  color: #1a9d63;
  background: #1a9d6314;
}
.plugin-mark.bad {
  color: #d14343;
  background: #d1434314;
}
.plugin-dialog h2 {
  margin: 0;
  font-size: 15px;
}
.plugin-dialog header small {
  color: var(--muted);
  font-size: 12px;
}
.plugin-close {
  margin-left: auto;
  background: transparent;
  color: var(--muted);
  border-radius: 8px;
  padding: 4px;
}
.plugin-state {
  margin: 14px 0 12px;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--surface-soft);
  display: grid;
  gap: 3px;
}
.plugin-state strong {
  font-size: 13px;
}
.plugin-state small {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.45;
}
.plugin-state.ok strong {
  color: #1a9d63;
}
.plugin-state.bad strong {
  color: #d14343;
}
.plugin-rows {
  display: grid;
  gap: 8px;
  padding: 12px 0;
  border-top: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
}
.plugin-row {
  display: flex;
  justify-content: space-between;
  font-size: 12.5px;
}
.plugin-row span {
  color: var(--muted);
}
.plugin-path {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}
.plugin-row b {
  font-weight: 600;
}
.plugin-hint {
  margin: 12px 0 14px;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
}
.plugin-dialog footer {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
.plugin-dialog footer button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border-radius: 9px;
  font-size: 12.5px;
}
.plugin-dialog footer .ghost {
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--ink);
}
.plugin-dialog footer .primary {
  background: var(--accent);
  color: #fff;
}
.plugin-dialog footer button:disabled {
  opacity: 0.6;
}
</style>
