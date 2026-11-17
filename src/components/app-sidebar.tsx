import {
  ArchiveIcon,
  ArrowDownToLineIcon,
  CheckCircle2Icon,
  ClapperboardIcon,
  Clock3Icon,
  CircleAlertIcon,
  FolderIcon,
  LayoutListIcon,
  MagnetIcon,
  Music2Icon,
  PackageIcon,
  PauseCircleIcon,
  PauseIcon,
  PlayIcon,
  RefreshCwIcon,
  FileTextIcon,
  ListTodoIcon,
  SettingsIcon,
  type LucideIcon,
} from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ThemeToggle } from "@/components/theme-toggle";
import { BrandMark } from "@/components/brand-mark";
import { categoryLabel } from "@/lib/mdm/format";
import type { Filter, MdmList } from "@/lib/mdm/types";

const STATUS_ITEMS: {
  status: "Downloading" | "Queued" | "Complete" | "Stopped" | "Error";
  label: string;
  icon: LucideIcon;
}[] = [
  { status: "Downloading", label: "下载中", icon: ArrowDownToLineIcon },
  { status: "Queued", label: "排队中", icon: Clock3Icon },
  { status: "Complete", label: "已完成", icon: CheckCircle2Icon },
  { status: "Stopped", label: "已停止", icon: PauseCircleIcon },
  { status: "Error", label: "出错", icon: CircleAlertIcon },
];

const CATEGORY_ICONS: Record<string, LucideIcon> = {
  Video: ClapperboardIcon,
  Music: Music2Icon,
  Documents: FileTextIcon,
  Programs: PackageIcon,
  Compressed: ArchiveIcon,
  Torrents: MagnetIcon,
};

interface AppSidebarProps {
  data: MdmList | null;
  filter: Filter;
  onFilter: (filter: Filter) => void;
  connected: boolean | null;
  onQueue: (action: "start_queue" | "stop_queue", queue: string) => void;
  onRefresh: () => void;
  onSettings: () => void;
}

function NavItem({
  icon: Icon,
  label,
  count,
  active,
  onClick,
}: {
  icon: LucideIcon;
  label: string;
  count?: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition-colors",
        "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
        active
          ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
          : "text-sidebar-foreground/80",
      )}
    >
      <Icon className="size-4 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate text-left">{label}</span>
      {count !== undefined && (
        <span className="text-xs tabular-nums text-muted-foreground">
          {count}
        </span>
      )}
    </button>
  );
}

function SectionLabel({ children }: { children: string }) {
  return (
    <div className="px-2.5 pt-5 pb-1.5 text-xs font-medium text-muted-foreground">
      {children}
    </div>
  );
}

export function AppSidebar({
  data,
  filter,
  onFilter,
  connected,
  onQueue,
  onRefresh,
  onSettings,
}: AppSidebarProps) {
  const downloads = data?.downloads ?? [];
  const counts = downloads.reduce<Record<string, number>>((total, item) => {
    total[item.status] = (total[item.status] ?? 0) + 1;
    return total;
  }, {});

  const categoryCounts = downloads.reduce<Record<string, number>>(
    (total, item) => {
      total[item.category] = (total[item.category] ?? 0) + 1;
      return total;
    },
    {},
  );

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r bg-sidebar text-sidebar-foreground">
      <div className="flex items-center gap-2.5 px-4 py-4">
        <div className="flex size-9 items-center justify-center rounded-xl bg-primary text-primary-foreground">
          <BrandMark className="size-5" />
        </div>
        <div className="min-w-0">
          <div className="truncate font-heading text-sm font-semibold tracking-tight">
           浮槎
          </div>
          <div className="truncate text-xs text-muted-foreground">
            the Star Raft
          </div>
        </div>
      </div>

      <Separator />

      <ScrollArea className="min-h-0 flex-1 [&_[data-slot=scroll-area-scrollbar]]:hidden">
        <nav className="flex flex-col gap-0.5 p-3">
          <NavItem
            icon={LayoutListIcon}
            label="全部下载"
            count={downloads.length}
            active={filter.kind === "all"}
            onClick={() => onFilter({ kind: "all" })}
          />
          {STATUS_ITEMS.map((item) => (
            <NavItem
              key={item.status}
              icon={item.icon}
              label={item.label}
              count={counts[item.status] ?? 0}
              active={
                filter.kind === "status" && filter.status === item.status
              }
              onClick={() => onFilter({ kind: "status", status: item.status })}
            />
          ))}

          <SectionLabel>分类</SectionLabel>
          {(data?.categories ?? []).map((category) => (
            <NavItem
              key={category.key}
              icon={CATEGORY_ICONS[category.key] ?? FolderIcon}
              label={categoryLabel(category.key, category.name)}
              count={categoryCounts[category.key] ?? 0}
              active={
                filter.kind === "category" &&
                filter.category === category.key
              }
              onClick={() =>
                onFilter({ kind: "category", category: category.key })
              }
            />
          ))}

          {(data?.queues.length ?? 0) > 0 && (
            <SectionLabel>队列</SectionLabel>
          )}
          {(data?.queues ?? []).map((queue) => (
            <div
              key={queue.name}
              className="flex items-center gap-2 rounded-md px-2.5 py-2 text-sm text-sidebar-foreground/80"
            >
              <ListTodoIcon className="size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate" title={queue.name}>
                {queue.name}
              </span>
              <span
                className={cn(
                  "size-1.5 shrink-0 rounded-full",
                  queue.running ? "bg-primary" : "bg-muted-foreground/40",
                )}
              />
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={queue.running ? "停止队列" : "启动队列"}
                    onClick={() =>
                      onQueue(
                        queue.running ? "stop_queue" : "start_queue",
                        queue.name,
                      )
                    }
                  >
                    {queue.running ? <PauseIcon /> : <PlayIcon />}
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  {queue.running ? "停止队列" : "启动队列"}
                </TooltipContent>
              </Tooltip>
            </div>
          ))}
        </nav>
      </ScrollArea>

      <Separator />

      <div className="flex items-center gap-2.5 px-3.5 py-3">
        <span
          className={cn(
            "size-2 rounded-full",
            connected ? "bg-primary" : "bg-destructive",
          )}
        />
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-medium">
            {connected === null
              ? "正在连接…"
              : connected
                ? "已连接 MDM"
                : "未连接 MDM"}
          </div>
          <div className="truncate text-[11px] text-muted-foreground">
            本地自动化 API
          </div>
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="设置"
              onClick={onSettings}
            >
              <SettingsIcon />
            </Button>
          </TooltipTrigger>
          <TooltipContent>设置</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="刷新"
              onClick={onRefresh}
            >
              <RefreshCwIcon />
            </Button>
          </TooltipTrigger>
          <TooltipContent>刷新</TooltipContent>
        </Tooltip>
        <ThemeToggle />
      </div>
    </aside>
  );
}
