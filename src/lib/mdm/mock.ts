import type {
  ActAnswer,
  CategoryInfo,
  DownloadInfo,
  MdmList,
  QueueInfo,
} from "./types";
import { fileNameOf } from "./format";

const KB = 1024;
const MB = 1024 * KB;
const GB = 1024 * MB;

const CATEGORIES: CategoryInfo[] = [
  { key: "Video", name: "视频" },
  { key: "Music", name: "音乐" },
  { key: "Documents", name: "文档" },
  { key: "Programs", name: "程序" },
  { key: "Compressed", name: "压缩包" },
  { key: "Torrents", name: "种子" },
];

function make(
  id: number,
  file: string,
  status: DownloadInfo["status"],
  size: number,
  extra: Partial<DownloadInfo> = {},
): DownloadInfo {
  return {
    id,
    file,
    url: `https://example.com/files/${file}`,
    path: `D:\\Downloads\\${file}`,
    status,
    size,
    downloaded: 0,
    speed: 0,
    added: "今天 14:32",
    category: "Programs",
    error: null,
    ...extra,
  };
}

let downloads: DownloadInfo[] = [
  make(1, "ubuntu-24.04.2-desktop-amd64.iso", "Downloading", 6.1 * GB, {
    downloaded: 2.5 * GB,
    speed: 8.6 * MB,
    added: "今天 14:32",
  }),
  make(2, "open-data-2026.csv", "Downloading", 12.4 * MB, {
    downloaded: 8.4 * MB,
    speed: 1.2 * MB,
    category: "Documents",
    added: "今天 14:44",
  }),
  make(3, "rust-1.99.1-x86_64-pc-windows-msvc.msi", "Downloading", 192 * MB, {
    downloaded: 44 * MB,
    speed: 3.4 * MB,
    added: "今天 14:51",
  }),
  make(4, "big-buck-bunny-1080p.mp4", "Complete", 1.4 * GB, {
    downloaded: 1.4 * GB,
    category: "Video",
    added: "昨天 21:08",
  }),
  make(5, "podcast-episode-42.mp3", "Complete", 38 * MB, {
    downloaded: 38 * MB,
    category: "Music",
    added: "昨天 20:12",
  }),
  make(6, "project-report-2026.pdf", "Complete", 4.2 * MB, {
    downloaded: 4.2 * MB,
    category: "Documents",
    added: "昨天 18:03",
  }),
  make(7, "holiday-photos.zip", "Error", 240 * MB, {
    downloaded: 90 * MB,
    category: "Compressed",
    error: "服务器返回 403 Forbidden",
    added: "昨天 16:47",
  }),
  make(8, "release-notes-2026.pdf", "Stopped", 2.1 * MB, {
    downloaded: 0.9 * MB,
    category: "Documents",
    added: "昨天 11:26",
  }),
  make(9, "archlinux-2026.11.01-x86_64.iso", "Queued", 1.2 * GB, {
    downloaded: 0,
    added: "今天 15:02",
  }),
];

let queues: QueueInfo[] = [
  { name: "Main download queue", running: true, ids: [1, 2, 3, 9] },
  { name: "今晚下载", running: false, ids: [] },
];

let nextId = 100;
let lastTick = Date.now();

function tick() {
  const now = Date.now();
  const seconds = Math.min((now - lastTick) / 1000, 2);
  lastTick = now;
  for (const item of downloads) {
    if (item.status !== "Downloading" || item.size === null) continue;
    item.downloaded = Math.min(item.size, item.downloaded + item.speed * seconds);
    if (item.downloaded >= item.size) {
      item.status = "Complete";
      item.speed = 0;
    }
  }
}

function categoryFor(url: string): string {
  if (url.startsWith("magnet:")) return "Torrents";
  const extension = fileNameOf(url).split(".").pop()?.toLowerCase() ?? "";
  if (["mp4", "mkv", "webm", "avi", "mov"].includes(extension)) return "Video";
  if (["mp3", "flac", "wav", "m4a", "opus"].includes(extension)) return "Music";
  if (["pdf", "doc", "docx", "xls", "xlsx", "csv", "txt"].includes(extension))
    return "Documents";
  if (["zip", "rar", "7z", "tar", "gz", "xz"].includes(extension))
    return "Compressed";
  return "Programs";
}

function act(action: string, ids: number[]): ActAnswer {
  const answer: ActAnswer = { done: [], skipped: [] };
  for (const id of ids) {
    const item = downloads.find((candidate) => candidate.id === id);
    if (!item) {
      answer.skipped.push({ id, why: "not in list" });
      continue;
    }
    if ((action === "remove" || action === "delete") && item.status === "Downloading") {
      answer.skipped.push({ id, why: "not stopped" });
      continue;
    }
    if (action === "pause") {
      if (item.status !== "Downloading" && item.status !== "Queued") {
        answer.skipped.push({ id, why: "not running" });
        continue;
      }
      item.status = "Stopped";
      item.speed = 0;
    } else if (action === "start") {
      if (item.status === "Complete") {
        answer.skipped.push({ id, why: "already complete" });
        continue;
      }
      item.status = "Downloading";
      item.speed = item.speed || 2.4 * MB;
    } else if (action === "remove" || action === "delete") {
      downloads = downloads.filter((candidate) => candidate.id !== id);
    }
    answer.done.push(id);
  }
  return answer;
}

export async function mockCall(payload: Record<string, unknown>): Promise<unknown> {
  await new Promise((resolve) => setTimeout(resolve, 80));
  tick();

  if (payload.query === "list") {
    const list: MdmList = {
      downloads: structuredClone(downloads),
      speed: downloads
        .filter((item) => item.status === "Downloading")
        .reduce((total, item) => total + item.speed, 0),
      queues: structuredClone(queues),
      categories: structuredClone(CATEGORIES),
      free: 412 * GB,
    };
    return list;
  }

  if (typeof payload.action === "string" && Array.isArray(payload.ids)) {
    return act(payload.action, payload.ids as number[]);
  }

  if (payload.action === "start_queue" || payload.action === "stop_queue") {
    const queue = queues.find((candidate) => candidate.name === payload.queue);
    if (!queue) return { error: "unknown queue", code: "unknown-queue" };
    queue.running = payload.action === "start_queue";
    return { done: [], skipped: [] };
  }

  if (typeof payload.url === "string") {
    const url = payload.url;
    const file = (payload.name as string) || fileNameOf(url);
    const category =
      (payload.category as string) || categoryFor(url);
    const later = payload.later === true;
    const item: DownloadInfo = {
      id: nextId++,
      file,
      url,
      path: `${(payload.dir as string) || "D:\\Downloads"}\\${file}`,
      status: later ? "Queued" : "Downloading",
      size: Math.round((80 + Math.random() * 620) * MB),
      downloaded: 0,
      speed: later ? 0 : 2.6 * MB,
      added: new Date().toLocaleString("zh-CN", {
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
      }),
      category,
      error: null,
    };
    downloads = [item, ...downloads];
    return { id: item.id, pending: false };
  }

  return { ok: true };
}
