import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";
import {
  CheckIcon,
  CircleAlertIcon,
  Clock3Icon,
  PauseIcon,
} from "lucide-react";
import type { DownloadStatus } from "@/lib/mdm/types";

const LABELS: Record<DownloadStatus, string> = {
  Downloading: "下载中",
  Queued: "排队中",
  Stopped: "已停止",
  Complete: "已完成",
  Error: "出错",
};

export const STATUS_LABELS = LABELS;

export function StatusBadge({ status }: { status: DownloadStatus }) {
  if (status === "Downloading") {
    return (
      <Badge>
        <Spinner data-icon="inline-start" />
        {LABELS[status]}
      </Badge>
    );
  }
  if (status === "Error") {
    return (
      <Badge variant="destructive">
        <CircleAlertIcon />
        {LABELS[status]}
      </Badge>
    );
  }
  if (status === "Complete") {
    return (
      <Badge variant="secondary">
        <CheckIcon />
        {LABELS[status]}
      </Badge>
    );
  }
  if (status === "Queued") {
    return (
      <Badge variant="outline">
        <Clock3Icon />
        {LABELS[status]}
      </Badge>
    );
  }
  return (
    <Badge variant="outline">
      <PauseIcon />
      {LABELS[status]}
    </Badge>
  );
}
