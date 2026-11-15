<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Copy,
  Link,
  LoaderCircle,
  RefreshCw,
  Share2,
  X,
} from "@lucide/vue";
import {
  type CreatedShare,
  formatDate,
  formatSharePeriod,
  isShareExpired as expired,
  shareText,
} from "./cloud";
const props = defineProps<{
  shares: CreatedShare[];
  providers: { id: string; name: string; color: string; initial: string }[];
}>();
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
const syncing = ref(false);
const active = ref(props.providers[0]?.id ?? "");
/**
 * Records written before this version carry no provider, so they stay visible
 * in every tab until the next sync backfills them.
 */
const visible = computed(() =>
  props.shares.filter(
    (s) => !s.provider || !active.value || s.provider === active.value,
  ),
);
const countFor = (provider: string) =>
  props.shares.filter((s) => s.provider === provider).length;
const live = (s: CreatedShare) => !s.cancelled && !expired(s);
const selected = computed(() =>
  visible.value.filter((s) => batch.value.includes(s.id)),
);
const allSelected = computed(
  () => visible.value.length > 0 && batch.value.length === visible.value.length,
);
const someSelected = computed(
  () => batch.value.length > 0 && !allSelected.value,
);
const cancellable = computed(() => selected.value.filter(live));
function toggleAll() {
  batch.value = allSelected.value ? [] : visible.value.map((s) => s.id);
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
function open(s: CreatedShare) {
  void openUrl(s.url).catch(() => emit("error", "无法打开浏览器，请复制链接手动访问"));
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
/**
 * Pull the shares created on the provider so records made elsewhere show up
 * here too. The first pull per tab is silent: a provider whose list endpoint
 * is unavailable should not throw an error banner at page load.
 */
async function sync(quiet: boolean) {
  if (!active.value || syncing.value) return;
  syncing.value = true;
  try {
    await invoke("drive_share_sync", { provider: active.value });
    emit("changed");
  } catch (e) {
    if (!quiet) emit("error", String(e));
  } finally {
    syncing.value = false;
  }
}
watch(active, () => {
  batch.value = [];
  void sync(true);
});
watch(
  () => props.providers,
  (providers) => {
    if (!providers.some((p) => p.id === active.value)) {
      active.value = providers[0]?.id ?? "";
    }
  },
);
onMounted(() => void sync(true));
</script>
<template>
  <div class="cloud-page-toolbar">
    <p class="account-intro">
      按网盘查看分享记录，登录后会同步网盘上已有的分享。
    </p>
    <button class="primary" @click="emit('browse')">
      <Share2 :size="16" />选择文件分享
    </button>
  </div>
  <div
    v-if="providers.length"
    class="drive-providers"
    role="group"
    aria-label="按网盘查看分享记录"
  >
    <button
      v-for="provider in providers"
      :key="provider.id"
      :class="{ selected: active === provider.id }"
      :aria-pressed="active === provider.id"
      :disabled="busy"
      @click="active = provider.id"
    >
      <span
        class="drive-provider-avatar"
        :style="{ background: provider.color }"
        >{{ provider.initial }}</span
      >{{ provider.name
      }}<span v-if="countFor(provider.id)" class="provider-count">{{
        countFor(provider.id)
      }}</span>
    </button>
  </div>
  <section class="card task-list">
    <div
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
      <button
        class="icon-button head-refresh"
        :title="syncing ? '正在同步网盘上的分享记录' : '从网盘刷新'"
        aria-label="从网盘刷新"
        :disabled="syncing || busy"
        @click="sync(false)"
      >
        <LoaderCircle v-if="syncing" :size="16" class="spinning" /><RefreshCw
          v-else
          :size="16"
        />
      </button>
    </div>
    <div v-if="!visible.length" class="empty-state">
      <span class="empty-illustration"><Link :size="40" /></span>
      <h3>还没有分享记录</h3>
      <p>在「我的网盘」中选择文件，设置有效期和提取码。</p>
    </div>
    <article v-for="s in visible" :key="s.id" class="task-row share-record">
      <label
        class="task-select"
        :title="batch.includes(s.id) ? '取消选择' : '选择'"
      >
        <input
          type="checkbox"
          :checked="batch.includes(s.id)"
          :disabled="busy"
          @change="toggle(s.id)"
        />
      </label>
      <div class="task-file-icon"><Share2 :size="22" /></div>
      <div class="task-details">
        <div class="task-name-row">
          <strong :title="s.names.join('、')">{{ s.names.join("、") }}</strong
          ><span class="status-pill" :class="{ complete: live(s) }">{{
            s.cancelled ? "已取消" : expired(s) ? "已过期" : "分享中"
          }}</span>
        </div>
        <div class="share-link-text">
          <Link :size="14" /><a
            class="share-link"
            :href="s.url"
            :title="`在浏览器中打开 ${s.url}`"
            @click.prevent="open(s)"
            >{{ s.url }}</a
          ><span v-if="s.password">提取码：{{ s.password }}</span
          ><span v-else>无需提取码</span>
        </div>
        <div class="share-meta">
          <span>创建于 {{ formatDate(s.createdAt) }}</span>
          <span>· {{ formatSharePeriod(s.period) }}</span>
        </div>
      </div>
      <div v-if="live(s)" class="task-actions">
        <button
          class="icon-button"
          title="复制分享"
          aria-label="复制分享"
          @click="copy(s)"
        >
          <Copy :size="16" /></button
        ><button
          class="icon-button"
          title="取消分享"
          aria-label="取消分享"
          @click="askCancel(s)"
        >
          <X :size="16" />
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