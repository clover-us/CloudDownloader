export type DriveFile = {
  id: string;
  path: string;
  name: string;
  size: number;
  isDir: boolean;
  modified: number;
  created: number;
  md5: string;
};
export type DriveListing = {
  accountId: string;
  path: string;
  page: number;
  hasMore: boolean;
  files: DriveFile[];
};
export type UploadTask = {
  id: string;
  name: string;
  localPath: string;
  remotePath: string;
  /** Human readable upload directory; empty for tasks queued before it existed. */
  destination?: string;
  size: number;
  concurrency: number;
  status: string;
  error: string;
  completed: number;
  speed: number;
  createdAt: number;
  completedParts: number;
  totalParts: number;
  provider: string;
  batch?: { id: string; name: string; path?: string } | null;
};
export type CreatedShare = {
  id: string;
  url: string;
  password: string;
  period: number;
  createdAt: number;
  accountId: string;
  names: string[];
  cancelled: boolean;
  /** Provider key; empty for records saved before the field existed. */
  provider?: string;
  /** Files behind the link, kept so a stopped share can be created again. */
  items?: { id: string; path: string; name: string; isDir?: boolean }[];
  /** `custom`, `auto` or `none`; empty for records saved before it existed. */
  codeMode?: string;
};
export const formatBytes = (n: number) => {
  if (!n) return "0 B";
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), 4);
  return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${["B", "KiB", "MiB", "GiB", "TiB"][i]}`;
};
export const formatDate = (n: number) =>
  n ? new Date(n * 1000).toLocaleString("zh-CN", { hour12: false }) : "—";
export const shareText = (s: CreatedShare) =>
  s.password ? `链接：${s.url}\n提取码：${s.password}` : `链接：${s.url}`;
export const formatSharePeriod = (period: number) =>
  period === 0 ? "永久有效" : `${period} 天有效`;
export const isShareExpired = (s: CreatedShare, now = Date.now()) =>
  s.period > 0 && now >= (s.createdAt + s.period * 86400) * 1000;
