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

/** The local axum server; in dev Vite proxies relative `/api` to the same port. */
const API_PORT = 8484;
const API_ORIGIN = isTauri ? `http://127.0.0.1:${API_PORT}` : "";

const params =
  typeof window !== "undefined"
    ? new URLSearchParams(window.location.search)
    : new URLSearchParams();

export const isMock = !isTauri && params.get("mock") === "1";
const offlinePreview = !isTauri && params.get("offline") === "1";

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${API_ORIGIN}${path}`, init);
  } catch {
    throw new Error(
      `无法连接浮槎本地 API（127.0.0.1:${API_PORT}）。开发时请先运行 npm run api。`,
    );
  }

  const text = await response.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      if (response.ok) throw new Error(`API 返回了无法解析的内容：${text}`);
    }
  }

  if (!response.ok) {
    const message =
      body && typeof body === "object" && "error" in body
        ? String((body as { error: unknown }).error)
        : `请求失败（HTTP ${response.status}）`;
    throw new Error(message);
  }
  return body as T;
}

function post<T>(path: string, body: unknown): Promise<T> {
  return call<T>(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function request<T>(payload: Record<string, unknown>): Promise<T> {
  if (offlinePreview && payload.query === "list") {
    throw new Error("无法连接 MDM（预览模式）");
  }
  if (isMock) return mockCall(payload) as Promise<T>;

  const answer = await post<Record<string, unknown>>("/api/mdm", payload);
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
  if (isMock) return "%APPDATA%\\misterdownloadmanager\\ipc.json";
  return call<string>("/api/ipc-path");
}

export async function revealInFolder(path: string): Promise<void> {
  if (isMock) throw new Error("模拟模式无法打开文件夹");
  await post("/api/reveal", { path });
}

export async function openExternal(url: string): Promise<void> {
  if (!isTauri) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await post("/api/open", { url });
}

export async function readMdmDownloadDir(): Promise<string | null> {
  if (isMock) return null;
  return call<string | null>("/api/download-dir");
}

export async function systemDownloadDir(): Promise<string | null> {
  if (isMock) return null;
  return call<string | null>("/api/system-download-dir");
}

export async function pickDirectory(
  defaultPath?: string,
): Promise<string | null> {
  if (isMock) return null;
  const answer = await post<{ path: string | null }>("/api/pick-directory", {
    defaultPath: defaultPath?.trim() || undefined,
  });
  return answer.path;
}

export async function applyMdmSettings(input: {
  mode: MdmMode;
  downloadDir: string;
}): Promise<unknown> {
  if (isMock) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    return { ok: true, ...input };
  }
  return post("/api/settings", input);
}
