<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Link, LoaderCircle, Share2, X } from "@lucide/vue";
import {
  type CreatedShare,
  formatDate,
  formatSharePeriod,
  isShareExpired as expired,
  shareText,
} from "./cloud";
const props = defineProps<{ shares: CreatedShare[] }>();
const emit = defineEmits<{
  changed: [];
  error: [message: string];
  notice: [message: string];
  browse: [];
}>();
const target = ref<CreatedShare | null>(null);
const batchCancel = ref(false);
const batch = ref<string[]>([]);
const busy = ref(false);
const live = (s: CreatedShare) => !s.cancelled && !expired(s);
const selected = computed(() =>
  props.shares.filter((s) => batch.value.includes(s.id)),
);
const allSelected = computed(
  () => props.shares.length > 0 && batch.value.length === props.shares.length,
);
const someSelected = computed(
  () => batch.value.length > 0 && !allSelected.value,
);
const cancellable = computed(() => selected.value.filter(live));
function toggleAll() {
  batch.value = allSelected.value ? [] : props.shares.map((s) => s.id);
}
function toggle(id: string) {
  batch.value = batch.value.includes(id)
    ? batch.value.filter((v) => v !== id)
    : [...batch.value, id];
}
async function copy(s: CreatedShare) {
  try {
    await navigator.clipboard.writeText(shareText(s));
    emit("notice", "分享链接和提取码已复制");
  } catch {
    emit("error", "复制失败，请选中链接手动复制");
  }
}
async function copySelected() {
  if (!selected.value.length) return;
  try {
    await navigator.clipboard.writeText(
      selected.value.map(shareText).join("\n"),
    );
    emit("notice", `已复制 ${selected.value.length} 个分享链接`);
  } catch {
    emit("error", "复制失败，请选中链接手动复制");
  }
}
function askCancelSelected() {
  target.value = null;
  batchCancel.value = true;
}
async function cancelOne(s: CreatedShare) {
  await invoke("drive_cancel_share", { id: s.id });
}
async function cancel() {
  const targets = batchCancel.value
    ? cancellable.value
    : target.value
      ? [target.value]
      : [];
  if (!targets.length) return;
  busy.value = true;
  let done = 0;
  let failure = "";
  for (const s of targets) {
    try {
      await cancelOne(s);
      done += 1;
    } catch (e) {
      failure = String(e);
      break;
    }
  }
  busy.value = false;
  if (!done && !failure) return;
  target.value = null;
  batchCancel.value = false;
  batch.value = [];
  emit("changed");
  if (failure) {
    emit(
      "error",
      done ? `已取消 ${done} 个分享，其余失败：${failure}` : failure,
    );
  } else {
    emit("notice", done > 1 ? `已取消 ${done} 个分享` : "分享已取消");
  }
}
function askCancel(s: CreatedShare) {
  batchCancel.value = false;
  target.value = s;
}
</script>
<template>
  <div class="cloud-page-toolbar">
    <p class="account-intro">显示通过本程序创建的分享。</p>
    <button class="primary" @click="emit('browse')">
      <Share2 :size="16" />选择文件分享
    </button>
  </div>
  <section class="card task-list">
    <div
      v-if="shares.length"
      class="task-list-head"
      :class="{ 'head-selected': batch.length }"
    >
      <label class="task-select-all" :title="allSelected ? '取消全选' : '全选'">
        <input
          type="checkbox"
          :checked="allSelected"
          :indeterminate="someSelected"
          :disabled="busy"
          @change="toggleAll"
        />
      </label>
      <template v-if="batch.length">
        <span class="batch-count"
          >已选中 <b>{{ batch.length }}</b> 项</span
        >
        <button
          class="text-button batch-clear"
          :disabled="busy"
          @click="batch = []"
        >
          取消选中
        </button>
        <div class="batch-actions">
          <button class="batch-btn" :disabled="busy" @click="copySelected">
            <Copy :size="15" />复制分享
          </button>
          <button
            class="batch-btn danger"
            :disabled="busy || !cancellable.length"
            @click="askCancelSelected"
          >
            <X :size="15" />取消分享
          </button>
          <LoaderCircle v-if="busy" :size="15" class="spinning" />
        </div>
      </template>
      <span v-else class="head-label">分享记录</span>
      <span class="batch-spacer" />
    </div>
    <div v-if="!shares.length" class="empty-state">
      <span class="empty-illustration"><Link :size="40" /></span>
      <h3>还没有分享记录</h3>
      <p>在「我的网盘」中选择文件，设置有效期和提取码。</p>
    </div>
    <article
      v-for="s in shares"
      :key="s.id"
      class="share-record"
    >
      <div class="task-name-row">
        <label class="task-select" :title="batch.includes(s.id) ? '取消选择' : '选择'">
          <input
            type="checkbox"
            :checked="batch.includes(s.id)"
            :disabled="busy"
            @change="toggle(s.id)"
          />
        </label>
        <strong :title="s.names.join('、')">{{ s.names.join("、") }}</strong
        ><span
          class="status-pill"
          :class="{ complete: live(s) }"
          >{{ s.cancelled ? "已取消" : expired(s) ? "已过期" : "分享中" }}</span
        >
      </div>
      <div class="share-link-text">
        <Link :size="15" /><span>{{ s.url }}</span
        ><span>提取码：{{ s.password }}</span>
      </div>
      <div class="share-record-bottom">
        <span
          >创建于 {{ formatDate(s.createdAt) }} ·
          {{ formatSharePeriod(s.period) }}</span
        ><button v-if="live(s)" class="text-button" @click="copy(s)">
          <Copy :size="14" />复制分享</button
        ><button
          v-if="live(s)"
          class="text-button cancel-share"
          @click="askCancel(s)"
        >
          <X :size="14" />取消分享
        </button>
      </div>
    </article>
  </section>
  <Teleport to="body"
    ><div v-if="target || batchCancel" class="modal-backdrop">
      <div class="confirm-dialog">
        <h2>
          {{
            batchCancel
              ? `取消所选的 ${cancellable.length} 个分享？`
              : "取消这个分享？"
          }}
        </h2>
        <p v-if="batchCancel">
          取消后这些链接将无法访问，网盘文件会保留。
        </p>
        <p v-else-if="target">
          {{ target.names.join("、")
          }}<br />取消后原链接将无法访问，网盘文件会保留。
        </p>
        <div class="button-row">
          <button
            class="secondary"
            :disabled="busy"
            @click="
              target = null;
              batchCancel = false;
            "
          >
            返回</button
          ><button class="danger" :disabled="busy" @click="cancel">
            {{ busy ? "正在取消…" : "取消分享" }}
          </button>
        </div>
      </div>
    </div></Teleport
  >
</template>