const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return "未知";
  if (bytes < 1) return "0 B";
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    UNITS.length - 1,
  );
  const value = bytes / 1024 ** exponent;
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${UNITS[exponent]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  if (!bytesPerSecond) return "—";
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function percentOf(downloaded: number, size: number | null): number {
  if (!size) return 0;
  return Math.min(100, Math.round((downloaded / size) * 100));
}

export function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url.startsWith("magnet:") ? "magnet" : url;
  }
}

export function fileNameOf(url: string): string {
  if (url.startsWith("magnet:")) {
    const match = url.match(/dn=([^&]+)/);
    return match ? decodeURIComponent(match[1]) : "磁力链接";
  }
  try {
    const path = new URL(url).pathname;
    const name = path.split("/").filter(Boolean).pop();
    return name ? decodeURIComponent(name) : url;
  } catch {
    return url;
  }
}

export function categoryLabel(key: string, fallback?: string): string {
  const names: Record<string, string> = {
    General: "常规",
    Video: "视频",
    Music: "音乐",
    Documents: "文档",
    Programs: "程序",
    Compressed: "压缩包",
    Torrents: "种子",
  };
  return names[key] ?? fallback ?? key;
}

export function extensionOf(file: string): string {
  const dot = file.lastIndexOf(".");
  return dot > 0 ? file.slice(dot + 1).toLowerCase() : "";
}
