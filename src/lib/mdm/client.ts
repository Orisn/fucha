import { invoke } from "@tauri-apps/api/core";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { mockCall } from "./mock";
import type {
  ActAnswer,
  AddInput,
  DownloadAction,
  MdmList,
} from "./types";
import type { MdmMode } from "@/lib/studio-settings";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const offlinePreview =
  !isTauri &&
  new URLSearchParams(window.location.search).get("offline") === "1";

async function request<T>(payload: Record<string, unknown>): Promise<T> {
  if (!isTauri) {
    if (offlinePreview && payload.query === "list") {
      throw new Error("无法连接 MDM（预览模式）");
    }
    return mockCall(payload) as Promise<T>;
  }
  const answer = await invoke<Record<string, unknown>>("mdm_request", {
    payload,
  });
  if (answer && typeof answer === "object" && "error" in answer) {
    throw new Error(String((answer as { error: unknown }).error));
  }
  return answer as T;
}

export async function fetchList(): Promise<MdmList> {
  const answer = await request<MdmList>({ query: "list" });
  return {
    downloads: answer.downloads ?? [],
    speed: answer.speed ?? 0,
    queues: answer.queues ?? [],
    categories: answer.categories ?? [],
    free: answer.free ?? null,
  };
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
  if (!isTauri) return "%APPDATA%\\misterdownloadmanager\\ipc.json";
  return invoke<string>("mdm_ipc_path");
}

export async function revealInFolder(path: string): Promise<void> {
  if (!isTauri) throw new Error("浏览器预览模式无法打开文件夹");
  await revealItemInDir(path);
}

export async function openExternal(url: string): Promise<void> {
  if (!isTauri) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await openUrl(url);
}

export async function readMdmDownloadDir(): Promise<string | null> {
  if (!isTauri) return null;
  return invoke<string | null>("mdm_download_dir");
}

export async function applyMdmSettings(input: {
  mode: MdmMode;
  downloadDir: string;
}): Promise<unknown> {
  if (!isTauri) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    return { ok: true, ...input };
  }
  return invoke("mdm_apply_settings", input);
}
