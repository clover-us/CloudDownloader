<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { ArrowDownToLine, Globe, X } from "@lucide/vue";

const props = defineProps<{
  url: string;
  filename?: string | null;
  site?: string;
  waitSeconds?: number;
}>();
const emit = defineEmits<{ take: []; browse: []; ignore: []; open: [] }>();

const left = ref(props.waitSeconds ?? 15);
const primary = ref<HTMLButtonElement | null>(null);
let timer: number | undefined;
let previousFocus: HTMLElement | null = null;

// Esc means "ignore", the same as the close button; focus starts on the primary
// action so the countdown can be acted on with the keyboard alone.
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("ignore");
}

onMounted(() => {
  previousFocus = document.activeElement as HTMLElement | null;
  document.addEventListener("keydown", onKeydown);
  primary.value?.focus();
  timer = window.setInterval(() => {
    left.value -= 1;
    if (left.value <= 0) {
      window.clearInterval(timer);
      emit("take");
    }
  }, 1000);
});

onUnmounted(() => {
  window.clearInterval(timer);
  document.removeEventListener("keydown", onKeydown);
  previousFocus?.focus();
});

function name() {
  const raw = (props.filename || props.url).split("/").pop() ?? props.url;
  const cut = raw.split("?")[0];
  try {
    return decodeURIComponent(cut);
  } catch {
    return cut;
  }
}

// 长链接压成「主机名/…/文件名」，完整地址放在 title 里。
const shortUrl = computed(() => {
  try {
    const parsed = new URL(props.url);
    const segments = parsed.pathname.split("/").filter(Boolean);
    let tail = segments.pop() ?? "";
    try {
      tail = decodeURIComponent(tail);
    } catch {
      /* 保留原样 */
    }
    if (tail.length > 42) tail = `${tail.slice(0, 42)}…`;
    return `${parsed.host}/…/${tail}`;
  } catch {
    return props.url.length > 60 ? `${props.url.slice(0, 60)}…` : props.url;
  }
});

const copied = ref(false);
let clickTimer: number | undefined;

function openLink() {
  window.clearTimeout(clickTimer);
  clickTimer = window.setTimeout(() => emit("open"), 260);
}

async function copyLink() {
  window.clearTimeout(clickTimer);
  try {
    await navigator.clipboard.writeText(props.url);
  } catch {
    const area = document.createElement("textarea");
    area.value = props.url;
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
  copied.value = true;
  window.setTimeout(() => (copied.value = false), 1600);
}
</script>

<template>
  <div class="handoff-backdrop" @click.self="emit('ignore')">
    <section class="handoff-dialog" role="dialog" aria-label="浏览器下载接力">
      <header>
        <span class="handoff-mark"><ArrowDownToLine :size="18" /></span>
        <div>
          <h2>浏览器想把这个下载交给 CloudDownloader</h2>
          <small>
            浏览器已经停下自己的下载；选“用浏览器下载”会重新在浏览器里开始
          </small>
        </div>
        <button class="handoff-close" aria-label="忽略" @click="emit('ignore')">
          <X :size="16" />
        </button>
      </header>

      <div class="handoff-file">
        <strong :title="name()">{{ name() }}</strong>
        <button
          class="handoff-link"
          type="button"
          :title="`${url}\n单击在浏览器打开，双击复制`"
          @click="openLink"
          @dblclick="copyLink"
        >
          {{ copied ? "已复制链接" : shortUrl }}
        </button>
      </div>

      <div class="handoff-meta">
        <span v-if="site">来自 {{ site }}</span>
        <span v-else>来源未知</span>
      </div>

      <footer>
        <button class="ghost" @click="emit('ignore')">忽略</button>
        <button class="ghost" @click="emit('browse')">
          <Globe :size="14" /> 用浏览器下载
        </button>
        <button ref="primary" class="primary" @click="emit('take')">
          <ArrowDownToLine :size="14" /> 交给 CloudDownloader（{{ left }}）
        </button>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.handoff-backdrop {
  position: fixed;
  inset: 0;
  z-index: 65;
  display: grid;
  place-items: center;
  background: #0f1d3066;
  backdrop-filter: blur(2px);
}
.handoff-dialog {
  width: min(520px, calc(100vw - 48px));
  border-radius: 16px;
  border: 1px solid var(--line);
  background: var(--surface);
  box-shadow: 0 24px 60px #0f1d3033;
  padding: 18px 18px 16px;
}
.handoff-dialog header {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}
.handoff-mark {
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: var(--accent-soft);
  color: var(--accent);
}
.handoff-dialog h2 {
  margin: 0;
  font-size: 14.5px;
}
.handoff-dialog header small {
  display: block;
  margin-top: 3px;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.45;
}
.handoff-close {
  margin-left: auto;
  background: transparent;
  color: var(--muted);
  border-radius: 8px;
  padding: 4px;
}
.handoff-file {
  margin: 14px 0 8px;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--surface-soft);
  display: grid;
  gap: 4px;
}
.handoff-file strong {
  font-size: 13px;
  word-break: break-all;
}
.handoff-link {
  justify-self: start;
  max-width: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--accent);
  font-size: 11.5px;
  text-align: left;
  text-decoration: underline;
  text-decoration-color: color-mix(in oklab, var(--accent) 40%, transparent);
  text-underline-offset: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}
.handoff-link:hover {
  text-decoration-color: var(--accent);
}
.handoff-meta {
  color: var(--muted);
  font-size: 12px;
  margin-bottom: 14px;
}
.handoff-dialog footer {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
.handoff-dialog footer button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border-radius: 9px;
  font-size: 12.5px;
}
.handoff-dialog footer .ghost {
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--ink);
}
.handoff-dialog footer .primary {
  background: var(--accent);
  color: #fff;
}
</style>
