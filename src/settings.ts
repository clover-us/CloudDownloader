export type CloseAction = "tray" | "exit";

export type Settings = {
  downloadDir: string;
  maxOverallDownloadLimit: number;
  maxConcurrentDownloads: number;
  defaultConnections: number;
  closeAction: CloseAction;
  autoCheckUpdates: boolean;
  uploadChunkMib: number;
};

/** 与 src-tauri/src/settings.rs 中的常量保持一致。 */
export const maxDownloadLimit = 1024 * 1024;
export const maxConcurrentDownloads = 10;
export const connectionChoices = [1, 4, 8, 16];
/** 上传分片大小（MiB）：用于迅雷/夸克 OSS 分片；百度 PCS 固定 4 MiB。与后端常量一致。 */
export const minUploadChunkMib = 1;
export const maxUploadChunkMib = 32;
export const uploadChunkChoices = [4, 8, 16, 32];

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
};
