import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mockCall } from "./mock";
import type {
  ActAnswer,
  AddInput,
  BrowserIntegration,
  DownloadAction,
  MdmList,
  MdmNotice,
} from "./types";
import type { MdmMode } from "@/lib/studio-settings";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const params =
  typeof window !== "undefined"
    ? new URLSearchParams(window.location.search)
    : new URLSearchParams();

/** `?mock=1` in a browser: the in-memory mock, no backend. */
export const isMock = !isTauri && params.get("mock") === "1";
/** `?offline=1` in a browser: show the connection screen, as when the core is down. */
const offlinePreview = !isTauri && params.get("offline") === "1";

function messageOf(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function requireTauri(): void {
  if (!isTauri) {
    throw new Error("本地后端只在浮槎桌面应用中可用；浏览器预览请加上 ?mock=1。");
  }
}

/** One command to the in-process core (`mdm_call`), the answer as JSON. */
async function invokeCall<T>(payload: Record<string, unknown>): Promise<T> {
  requireTauri();
  try {
    const answer = await invoke<Record<string, unknown>>("mdm_call", {
      payload,
    });
    if (answer && typeof answer === "object" && "error" in answer) {
      throw new Error(String((answer as { error: unknown }).error));
    }
    return answer as T;
  } catch (cause) {
    throw cause instanceof Error ? cause : new Error(messageOf(cause));
  }
}

async function request<T>(payload: Record<string, unknown>): Promise<T> {
  if (offlinePreview && payload.query === "list") {
    throw new Error("无法连接下载核心（预览模式）");
  }
  if (isMock) return mockCall(payload) as Promise<T>;
  return invokeCall<T>(payload);
}

function normalizeList(payload: unknown): MdmList {
  let raw = payload;
  if (typeof raw === "string") {
    try {
      raw = JSON.parse(raw);
    } catch {
      raw = {};
    }
  }
  const answer = (raw ?? {}) as Partial<MdmList>;
  return {
    downloads: answer.downloads ?? [],
    speed: answer.speed ?? 0,
    queues: answer.queues ?? [],
    categories: answer.categories ?? [],
    free: answer.free ?? null,
  };
}

export async function fetchList(): Promise<MdmList> {
  const answer = await request<MdmList>({ query: "list" });
  return normalizeList(answer);
}

/**
 * The core's list snapshot, pushed at most four times a second. Resolves
 * with an unsubscribe function. Mock and browser previews get a no-op.
 */
export function onMdmList(
  handler: (list: MdmList) => void,
): Promise<() => void> {
  if (!isTauri || isMock) return Promise.resolve(() => {});
  return listen<string | MdmList>("mdm://list", (event) => {
    handler(normalizeList(event.payload));
  });
}

/** Notices that are not list state: limits, finished/failed, save errors. */
export function onMdmNotice(
  handler: (notice: MdmNotice) => void,
): Promise<() => void> {
  if (!isTauri || isMock) return Promise.resolve(() => {});
  return listen<string | MdmNotice>("mdm://notice", (event) => {
    const payload =
      typeof event.payload === "string"
        ? (JSON.parse(event.payload) as MdmNotice)
        : event.payload;
    handler(payload);
  });
}

export function act(action: DownloadAction, ids: number[]): Promise<ActAnswer> {
  return request<ActAnswer>({ action, ids });
}

export function addDownload(input: AddInput): Promise<unknown> {
  const payload: Record<string, unknown> = { url: input.url.trim(), silent: true };
  if (input.dir?.trim()) payload.dir = input.dir.trim();
  if (input.name?.trim()) payload.name = input.name.trim();
  if (input.category) payload.category = input.category;
  if (input.later) payload.later = true;
  return request(payload);
}

export function setQueue(
  action: "start_queue" | "stop_queue",
  queue: string,
): Promise<ActAnswer> {
  return request<ActAnswer>({ action, queue });
}

export async function ipcPath(): Promise<string> {
  if (!isTauri || isMock) return "%APPDATA%\\misterdownloadmanager\\ipc.json";
  return invoke<string>("ipc_path");
}

export async function revealInFolder(path: string): Promise<void> {
  if (isMock) throw new Error("模拟模式无法打开文件夹");
  requireTauri();
  await invoke("reveal", { path });
}

export async function openExternal(url: string): Promise<void> {
  if (!isTauri) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await invoke("open_external", { url });
}

export async function readMdmDownloadDir(): Promise<string | null> {
  if (isMock) return null;
  requireTauri();
  return invoke<string | null>("download_dir");
}

export async function systemDownloadDir(): Promise<string | null> {
  if (isMock) return null;
  requireTauri();
  return invoke<string | null>("system_download_dir");
}

export async function browserIntegration(): Promise<BrowserIntegration> {
  if (isMock) return { seen: false, browsers: [] };
  requireTauri();
  return invoke<BrowserIntegration>("browser_integration");
}

export async function pickDirectory(
  defaultPath?: string,
): Promise<string | null> {
  if (isMock) return null;
  requireTauri();
  return invoke<string | null>("pick_directory", {
    defaultPath: defaultPath?.trim() || null,
  });
}

export async function applyMdmSettings(input: {
  mode: MdmMode;
  downloadDir: string;
}): Promise<unknown> {
  if (isMock) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    return { ok: true, ...input };
  }
  requireTauri();
  return invoke("settings_write", { input });
}
