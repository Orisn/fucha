import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { CheckCircle2Icon, GaugeIcon, HardDriveIcon } from "lucide-react";
import { formatBytes, formatSpeed } from "@/lib/mdm/format";
import type { MdmList } from "@/lib/mdm/types";

export function StatsCards({ data }: { data: MdmList | null }) {
  const downloads = data?.downloads ?? [];
  const active = downloads.filter(
    (item) => item.status === "Downloading",
  ).length;
  const complete = downloads.filter((item) => item.status === "Complete");
  const completedBytes = complete.reduce(
    (total, item) => total + (item.size ?? 0),
    0,
  );

  return (
    <div className="grid gap-3 sm:grid-cols-3">
      <Card className="py-4">
        <CardHeader>
          <CardDescription>总速度</CardDescription>
          <CardTitle className="text-2xl tabular-nums">
            {formatSpeed(data?.speed ?? 0)}
          </CardTitle>
          <CardAction>
            <GaugeIcon className="size-4 text-muted-foreground" />
          </CardAction>
        </CardHeader>
        <CardContent className="text-xs text-muted-foreground">
          {active} 个任务正在下载
        </CardContent>
      </Card>

      <Card className="py-4">
        <CardHeader>
          <CardDescription>剩余空间</CardDescription>
          <CardTitle className="text-2xl tabular-nums">
            {formatBytes(data?.free ?? null)}
          </CardTitle>
          <CardAction>
            <HardDriveIcon className="size-4 text-muted-foreground" />
          </CardAction>
        </CardHeader>
        <CardContent className="text-xs text-muted-foreground">
          下载目录所在磁盘
        </CardContent>
      </Card>

      <Card className="py-4">
        <CardHeader>
          <CardDescription>已完成</CardDescription>
          <CardTitle className="text-2xl tabular-nums">
            {complete.length} 项
          </CardTitle>
          <CardAction>
            <CheckCircle2Icon className="size-4 text-muted-foreground" />
          </CardAction>
        </CardHeader>
        <CardContent className="text-xs text-muted-foreground">
          共 {formatBytes(completedBytes)}
        </CardContent>
      </Card>
    </div>
  );
}
