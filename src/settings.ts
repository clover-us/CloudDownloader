export type CloseAction = "tray" | "exit";

export type Settings = {
  downloadDir: string;
  maxOverallDownloadLimit: number;
  maxConcurrentDownloads: number;
  defaultConnections: number;
  closeAction: CloseAction;
  autoCheckUpdates: boolean;
  uploadChunkMib: number;
  uploadConcurrency: number;
  uploadPartConcurrency: number;
  minSplitMib: number;
};

/** 与 src-tauri/src/settings.rs 中的常量保持一致。 */
export const maxDownloadLimit = 1024 * 1024;
export const maxConcurrentDownloads = 10;
export const connectionChoices = [1, 4, 8, 16];
/** 上传分片大小（MiB）：用于迅雷/夸克 OSS 分片；百度 PCS 固定 4 MiB。与后端常量一致。 */
export const minUploadChunkMib = 1;
export const maxUploadChunkMib = 32;
export const uploadChunkChoices = [4, 8, 16, 32];

/** 同时上传的任务数（个）。与后端常量一致。 */
export const minUploadConcurrency = 1;
export const maxUploadConcurrency = 8;
/** 单个文件内部的分片并发上限。与后端常量一致。 */
export const minUploadPartConcurrency = 1;
export const maxUploadPartConcurrency = 16;
/** aria2 最小分段大小（MiB）。与后端常量一致。 */
export const minSplitMib = 1;
export const maxSplitMib = 64;

export const limitPresets = [
  { label: "不限速", value: 0 },
  { label: "1 MiB/s", value: 1024 },
  { label: "5 MiB/s", value: 5120 },
  { label: "10 MiB/s", value: 10240 },
];

export const defaultSettings: Settings = {
  downloadDir: "",
  maxOverallDownloadLimit: 0,
  maxConcurrentDownloads: 3,
  defaultConnections: 16,
  closeAction: "tray",
  autoCheckUpdates: true,
  uploadChunkMib: 8,
  uploadConcurrency: 2,
  uploadPartConcurrency: 4,
  minSplitMib: 1,
};
