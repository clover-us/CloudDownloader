export type Provider =
  | "quark"
  | "baidu"
  | "xunlei"
  | "pan115"
  | "pan123"
  | "c139"
  | "uc"
  | "lanzou"
  | "ilanzou";
export type CloudProvider = Exclude<Provider, "baidu">;

export const providers: {
  id: Provider;
  name: string;
  initial: string;
  color: string;
  host: string;
}[] = [
  { id: "quark", name: "夸克网盘", initial: "夸", color: "#397cf3", host: "pan.quark.cn" },
  { id: "baidu", name: "百度网盘", initial: "百", color: "#388cf8", host: "pan.baidu.com" },
  { id: "xunlei", name: "迅雷网盘", initial: "迅", color: "#2e7bf6", host: "pan.xunlei.com" },
  { id: "pan115", name: "115 网盘", initial: "115", color: "#287bd7", host: "115.com" },
  { id: "pan123", name: "123 云盘", initial: "123", color: "#3671e4", host: "yun.123pan.cn" },
  { id: "c139", name: "139 云盘", initial: "139", color: "#27a380", host: "yun.139.com" },
  { id: "uc", name: "UC 网盘", initial: "UC", color: "#e18b3b", host: "drive.uc.cn" },
  { id: "lanzou", name: "蓝奏云", initial: "蓝", color: "#428ccf", host: "pc.woozooo.com" },
  { id: "ilanzou", name: "蓝奏云优享版", initial: "优", color: "#319ba7", host: "www.ilanzou.com" },
];
export const anonymousShare = (id: Provider) =>
  ["uc", "lanzou", "ilanzou"].includes(id);
export const providerName = (id: Provider) =>
  providers.find((p) => p.id === id)?.name || id;

export function shareProvider(text: string): Provider | undefined {
  for (const match of text.matchAll(/https?:\/\/[^\s<>"，。；）)\]]+/g)) {
    try {
      const url = new URL(match[0]);
      if (url.username || url.password || url.port) continue;
      const host = url.hostname;
      if (host === "pan.quark.cn") return "quark";
      if (host === "pan.baidu.com") return "baidu";
      if (host === "pan.xunlei.com" || host === "www.pan.xunlei.com")
        return "xunlei";
      if (
        [
          "115.com",
          "www.115.com",
          "115cdn.com",
          "www.115cdn.com",
          "115rc.com",
          "www.115rc.com",
          "anxia.com",
        ].includes(host)
      )
        return "pan115";
      if (
        [
          "123pan.com",
          "www.123pan.com",
          "123pan.cn",
          "www.123pan.cn",
          "123865.com",
          "www.123865.com",
          "123912.com",
          "www.123912.com",
        ].includes(host) ||
        host === "share.123pan.cn" ||
        host.endsWith(".share.123pan.cn")
      )
        return "pan123";
      if (host === "yun.139.com") return "c139";
      if (["drive.uc.cn", "fast.uc.cn"].includes(host)) return "uc";
      if (["ilanzou.com", "www.ilanzou.com"].includes(host)) return "ilanzou";
      if (
        /^(?:[a-z0-9-]+\.)*(?:lanzou[a-z]?|lan[zs]o[ux])\.(?:com|net|org|cn)$/.test(
          host,
        )
      )
        return "lanzou";
    } catch {
      /* The backend reports malformed share links. */
    }
  }
}

/** File-manager actions a provider may expose in the drive workspace. */
export type DriveAction =
  | "browse"
  | "download"
  | "upload"
  | "mkdir"
  | "share"
  | "rename"
  | "delete";

/**
 * Per-provider capabilities. This mirrors what the Rust backend exposes today,
 * so unsupported actions can be rendered disabled instead of silently failing.
 * Backend reality: every provider browses + downloads; Baidu can create folders
 * and shares; Baidu and Xunlei can upload; Quark, Baidu and Xunlei can create
 * folders, rename and delete.
 */
export const driveCapabilities: Record<Provider, DriveAction[]> = {
  quark: ["browse", "download", "mkdir", "rename", "delete"],
  baidu: ["browse", "download", "upload", "mkdir", "share", "rename", "delete"],
  xunlei: ["browse", "download", "upload", "mkdir", "rename", "delete"],
  pan115: ["browse", "download"],
  pan123: ["browse", "download"],
  c139: ["browse", "download"],
  uc: ["browse", "download"],
  lanzou: ["browse", "download"],
  ilanzou: ["browse", "download"],
};

export const canDrive = (id: Provider, action: DriveAction) =>
  driveCapabilities[id].includes(action);

/** One-line description of what the drive workspace can do for a provider. */
export function driveSummary(id: Provider): string {
  const extras: string[] = [];
  if (canDrive(id, "upload")) extras.push("上传");
  if (canDrive(id, "mkdir")) extras.push("新建文件夹");
  if (canDrive(id, "share")) extras.push("分享");
  if (canDrive(id, "rename")) extras.push("重命名");
  if (canDrive(id, "delete")) extras.push("删除");
  return extras.length
    ? `浏览${providerName(id)}，支持${extras.join("、")}与下载。`
    : `浏览${providerName(id)}，选择文件或文件夹下载到本地。`;
}
