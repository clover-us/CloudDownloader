<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { ArrowDownToLine, Globe, X } from "@lucide/vue";

const props = defineProps<{
  url: string;
  filename?: string | null;
  site?: string;
  waitSeconds?: number;
}>();
const emit = defineEmits<{ take: []; browse: []; ignore: [] }>();

const left = ref(props.waitSeconds ?? 15);
let timer: number | undefined;

onMounted(() => {
  timer = window.setInterval(() => {
    left.value -= 1;
    if (left.value <= 0) {
      window.clearInterval(timer);
      emit("take");
    }
  }, 1000);
});

onUnmounted(() => window.clearInterval(timer));

function name() {
  const raw = (props.filename || props.url).split("/").pop() ?? props.url;
  const cut = raw.split("?")[0];
  try {
    return decodeURIComponent(cut);
  } catch {
    return cut;
  }
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
        <small :title="url">{{ url }}</small>
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
        <button class="primary" @click="emit('take')">
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
.handoff-file small {
  color: var(--muted);
  font-size: 11.5px;
  word-break: break-all;
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
