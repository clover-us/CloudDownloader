<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { CircleCheck, LoaderCircle, Plus, Search, ShieldCheck, X } from "@lucide/vue";
import { type Provider, providers } from "./providers";

type Account = {
  provider: Provider;
  loggedIn: boolean;
  name: string;
  message: string;
};

const props = defineProps<{
  accounts: Account[];
  signingIn: Provider[];
}>();
const emit = defineEmits<{
  login: [provider: Provider];
  select: [provider: Provider];
  close: [];
}>();

const keyword = ref("");
const accountFor = (id: Provider) =>
  props.accounts.find((a) => a.provider === id);
const matches = (id: Provider, name: string, host: string) => {
  const k = keyword.value.trim().toLowerCase();
  return !k || name.toLowerCase().includes(k) || host.includes(k) || id.includes(k);
};
const connected = computed(() =>
  providers.filter(
    (p) => accountFor(p.id)?.loggedIn && matches(p.id, p.name, p.host),
  ),
);
const available = computed(() =>
  providers.filter(
    (p) => !accountFor(p.id)?.loggedIn && matches(p.id, p.name, p.host),
  ),
);
const total = computed(() => connected.value.length + available.value.length);

function activate(id: Provider) {
  if (accountFor(id)?.loggedIn) emit("select", id);
  else if (!props.signingIn.includes(id)) emit("login", id);
}

const searchInput = ref<HTMLInputElement | null>(null);
let previousFocus: HTMLElement | null = null;
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}
onMounted(() => {
  previousFocus = document.activeElement as HTMLElement | null;
  document.addEventListener("keydown", onKeydown);
  void nextTick(() => searchInput.value?.focus());
});
onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKeydown);
  previousFocus?.focus();
});
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <section
      class="cloud-dialog account-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="add-account-title"
    >
      <div class="dialog-heading">
        <h2 id="add-account-title">添加网盘账号</h2>
        <button class="icon-button" title="关闭" @click="emit('close')">
          <X :size="17" />
        </button>
      </div>
      <p class="account-dialog-sub">
        选择网盘，在官方窗口完成扫码或验证码登录，状态会保存在这台电脑上。
      </p>
      <label class="account-dialog-search">
        <Search :size="15" />
        <input
          ref="searchInput"
          v-model="keyword"
          type="text"
          placeholder="搜索网盘"
          aria-label="搜索网盘"
        />
      </label>
      <div class="account-dialog-body">
        <p v-if="!total" class="provider-search-empty">没有找到匹配的网盘</p>
        <section v-if="connected.length" class="provider-section">
          <h3 class="provider-section-caption">
            已连接<span class="page-count">{{ connected.length }}</span>
          </h3>
          <div class="provider-grid">
            <button
              v-for="p in connected"
              :key="p.id"
              class="provider-tile connected"
              :title="`进入${p.name}`"
              @click="activate(p.id)"
            >
              <span class="provider-avatar" :style="{ background: p.color }">{{
                p.initial
              }}</span>
              <span class="provider-tile-copy">
                <strong>{{ p.name }}</strong>
                <small>{{ accountFor(p.id)?.name }}</small>
              </span>
              <span class="provider-tile-state">
                <CircleCheck :size="14" />已连接
              </span>
            </button>
          </div>
        </section>
        <section v-if="available.length" class="provider-section">
          <h3 class="provider-section-caption">
            未登录<span class="page-count">{{ available.length }}</span>
          </h3>
          <div class="provider-grid">
            <button
              v-for="p in available"
              :key="p.id"
              class="provider-tile"
              :class="{ waiting: signingIn.includes(p.id) }"
              :title="signingIn.includes(p.id) ? '登录窗口已打开，等待完成登录' : `登录${p.name}`"
              @click="activate(p.id)"
            >
              <span class="provider-avatar" :style="{ background: p.color }">{{
                p.initial
              }}</span>
              <span class="provider-tile-copy">
                <strong>{{ p.name }}</strong>
                <small>{{ p.host }}</small>
              </span>
              <span class="provider-tile-state">
                <template v-if="signingIn.includes(p.id)">
                  <LoaderCircle :size="14" class="spinning" />等待登录
                </template>
                <template v-else><Plus :size="14" />登录</template>
              </span>
            </button>
          </div>
        </section>
      </div>
      <p class="account-dialog-note">
        <ShieldCheck :size="14" />各网盘的登录状态相互独立，互不影响。
      </p>
    </section>
  </div>
</template>
