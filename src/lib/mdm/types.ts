export type DownloadStatus =
  | "Queued"
  | "Downloading"
  | "Stopped"
  | "Complete"
  | "Error";

export interface DownloadInfo {
  id: number;
  file: string;
  url: string;
  path: string;
  status: DownloadStatus;
  size: number | null;
  downloaded: number;
  speed: number;
  added: string | null;
  category: string;
  error: string | null;
}

export interface CategoryInfo {
  key: string;
  name: string;
}

export interface QueueInfo {
  name: string;
  running: boolean;
  ids: number[];
}

export interface MdmList {
  downloads: DownloadInfo[];
  speed: number;
  queues: QueueInfo[];
  categories: CategoryInfo[];
  free: number | null;
}

export type DownloadAction = "pause" | "start" | "remove" | "delete";

export interface ActAnswer {
  done: number[];
  skipped: { id: number; why: string }[];
}

export interface AddInput {
  url: string;
  dir?: string;
  name?: string;
  category?: string;
  later?: boolean;
}

export interface BrowserIntegration {
  seen: boolean;
  browsers: { name: string; version: string }[];
}

export type Filter =
  | { kind: "all" }
  | { kind: "status"; status: DownloadStatus }
  | { kind: "category"; category: string };
