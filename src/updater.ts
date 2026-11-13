import { ref } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdatePhase =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "downloading"
  | "ready"
  | "error";

export type UpdateStatus = {
  phase: UpdatePhase;
  version: string;
  notes: string;
  publishedAt: string;
  downloaded: number;
  total: number;
  error: string;
  checkedAt: number;
};

function idle(): UpdateStatus {
  return {
    phase: "idle",
    version: "",
    notes: "",
    publishedAt: "",
    downloaded: 0,
    total: 0,
    error: "",
    checkedAt: 0,
  };
}

export const updateStatus = ref<UpdateStatus>(idle());

/** 检查更新必须跨两次点击保持同一个 Update 句柄，否则无法下载。 */
let pending: Update | null = null;

function friendlyError(error: unknown): string {
  const message = String(error instanceof Error ? error.message : error);
  if (/signature|verify/i.test(message)) {
    return "更新包签名校验失败，已取消本次更新。";
  }
  if (/404|not found|release json|empty/i.test(message)) {
    return "尚未发布可用的更新清单，请稍后再试。";
  }
  if (/network|fetch|connect|timeout|dns|offline|request|http/i.test(message)) {
    return "无法连接更新服务器，请检查网络后重试。";
  }
  if (/not supported|unsupported|dev/i.test(message)) {
    return "开发模式下无法安装更新，请在安装版中验证。";
  }
  return message;
}

export async function checkForUpdates(options: { silent?: boolean } = {}) {
  if (!isTauri()) {
    updateStatus.value = {
      ...idle(),
      phase: "error",
      error: "当前环境不支持自动更新。",
      checkedAt: Date.now(),
    };
    return updateStatus.value;
  }
  updateStatus.value = { ...idle(), phase: "checking" };
  try {
    const update = await check();
    if (!update) {
      pending = null;
      updateStatus.value = { ...idle(), phase: "current", checkedAt: Date.now() };
    } else {
      pending = update;
      updateStatus.value = {
        ...idle(),
        phase: "available",
        version: update.version,
        notes: update.body ?? "",
        publishedAt: update.date ?? "",
        checkedAt: Date.now(),
      };
    }
  } catch (error) {
    pending = null;
    updateStatus.value = options.silent
      ? { ...idle(), checkedAt: Date.now() }
      : {
          ...idle(),
          phase: "error",
          error: friendlyError(error),
          checkedAt: Date.now(),
        };
  }
  return updateStatus.value;
}

export async function installUpdate() {
  const update = pending;
  if (!update) {
    updateStatus.value = {
      ...updateStatus.value,
      phase: "error",
      error: "请先检查更新。",
    };
    return;
  }
  updateStatus.value = {
    ...updateStatus.value,
    phase: "downloading",
    downloaded: 0,
    total: 0,
  };
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") {
        updateStatus.value = {
          ...updateStatus.value,
          total: event.data.contentLength ?? 0,
        };
      } else if (event.event === "Progress") {
        updateStatus.value = {
          ...updateStatus.value,
          downloaded: updateStatus.value.downloaded + event.data.chunkLength,
        };
      }
    });
    updateStatus.value = { ...updateStatus.value, phase: "ready" };
  } catch (error) {
    updateStatus.value = {
      ...updateStatus.value,
      phase: "error",
      error: friendlyError(error),
    };
  }
}

export async function restartForUpdate() {
  await relaunch();
}
