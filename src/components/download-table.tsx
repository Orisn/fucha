import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { FileIcon } from "@/components/file-icon";
import { StatusBadge } from "@/components/status-badge";
import {
  FolderOpenIcon,
  MoreHorizontalIcon,
  PauseIcon,
  PlayIcon,
  Trash2Icon,
} from "lucide-react";
import { categoryLabel, formatBytes, formatSpeed, hostOf, percentOf } from "@/lib/mdm/format";
import type { DownloadAction, DownloadInfo } from "@/lib/mdm/types";

interface DownloadTableProps {
  downloads: DownloadInfo[];
  onAction: (action: DownloadAction, ids: number[]) => void;
  onReveal: (path: string) => void;
  onDeleteRequest: (download: DownloadInfo) => void;
}

export function DownloadTable({
  downloads,
  onAction,
  onReveal,
  onDeleteRequest,
}: DownloadTableProps) {
  return (
    <Table className="min-w-[600px]">
      <TableHeader className="sticky top-0 z-10 bg-card">
        <TableRow className="hover:bg-transparent">
          <TableHead className="w-[34%] pl-4">文件</TableHead>
          <TableHead className="hidden w-24 min-[1000px]:table-cell">
            大小
          </TableHead>
          <TableHead className="w-40">进度</TableHead>
          <TableHead className="w-24">速度</TableHead>
          <TableHead className="w-24">状态</TableHead>
          <TableHead className="hidden w-36 min-[1150px]:table-cell">
            添加时间
          </TableHead>
          <TableHead className="w-12 pr-4" />
        </TableRow>
      </TableHeader>
      <TableBody>
        {downloads.map((download) => {
          const percent = percentOf(download.downloaded, download.size);
          const running = download.status === "Downloading";
          const canPause = running || download.status === "Queued";
          const canStart =
            download.status === "Stopped" ||
            download.status === "Error" ||
            download.status === "Queued";

          return (
            <TableRow key={download.id}>
              <TableCell className="pl-4">
                <div className="flex items-center gap-3">
                  <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted">
                    <FileIcon file={download.file} className="size-4" />
                  </div>
                  <div className="min-w-0">
                    <div className="max-w-[24rem] truncate font-medium">
                      {download.file}
                    </div>
                    <div className="max-w-[24rem] truncate text-xs text-muted-foreground">
                      {download.error ? (
                        <span className="text-destructive">{download.error}</span>
                      ) : (
                        <>
                          {hostOf(download.url)} ·{" "}
                          {categoryLabel(download.category)}
                        </>
                      )}
                    </div>
                  </div>
                </div>
              </TableCell>
              <TableCell className="hidden tabular-nums text-muted-foreground min-[1000px]:table-cell">
                {formatBytes(download.size)}
              </TableCell>
              <TableCell>
                <div className="flex flex-col gap-1.5">
                  <Progress value={percent} className="h-1.5" />
                  <div className="flex items-center justify-between text-xs text-muted-foreground tabular-nums">
                    <span>{percent}%</span>
                    <span className="hidden min-[1000px]:inline">
                      {formatBytes(download.downloaded)}
                      {download.size ? ` / ${formatBytes(download.size)}` : ""}
                    </span>
                  </div>
                </div>
              </TableCell>
              <TableCell className="tabular-nums">
                {formatSpeed(download.speed)}
              </TableCell>
              <TableCell>
                <StatusBadge status={download.status} />
              </TableCell>
              <TableCell className="hidden text-xs text-muted-foreground min-[1150px]:table-cell">
                {download.added ?? "—"}
              </TableCell>
              <TableCell className="pr-4">
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={`${download.file} 的操作`}
                    >
                      <MoreHorizontalIcon />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" className="w-48">
                    <DropdownMenuGroup>
                      {canPause && (
                        <DropdownMenuItem
                          onSelect={() => onAction("pause", [download.id])}
                        >
                          <PauseIcon />
                          暂停
                        </DropdownMenuItem>
                      )}
                      {canStart && (
                        <DropdownMenuItem
                          onSelect={() => onAction("start", [download.id])}
                        >
                          <PlayIcon />
                          继续
                        </DropdownMenuItem>
                      )}
                      <DropdownMenuItem
                        disabled={!download.path}
                        onSelect={() => onReveal(download.path)}
                      >
                        <FolderOpenIcon />
                        在文件夹中显示
                      </DropdownMenuItem>
                    </DropdownMenuGroup>
                    <DropdownMenuSeparator />
                    <DropdownMenuGroup>
                      <DropdownMenuItem
                        variant="destructive"
                        onSelect={() => onAction("remove", [download.id])}
                      >
                        <Trash2Icon />
                        从列表移除
                      </DropdownMenuItem>
                      <DropdownMenuItem
                        variant="destructive"
                        onSelect={() => onDeleteRequest(download)}
                      >
                        <Trash2Icon />
                        删除文件…
                      </DropdownMenuItem>
                    </DropdownMenuGroup>
                  </DropdownMenuContent>
                </DropdownMenu>
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
