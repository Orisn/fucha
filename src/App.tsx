import { useMemo, useState } from "react";
import { toast } from "sonner";
import {
  CircleAlertIcon,
  EllipsisIcon,
  InboxIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Spinner } from "@/components/ui/spinner";
import { AddDownloadDialog } from "@/components/add-download-dialog";
import { AppSidebar } from "@/components/app-sidebar";
import { BrowserIntegrationAlert } from "@/components/browser-integration";
import { ConnectionScreen } from "@/components/connection-screen";
import { DeleteDialog } from "@/components/delete-dialog";
import { DownloadTable } from "@/components/download-table";
import { SettingsDialog } from "@/components/settings-dialog";
import { StatsCards } from "@/components/stats-cards";
import { STATUS_LABELS } from "@/components/status-badge";
import { useBrowserIntegration } from "@/hooks/use-browser-integration";
import { useMdm } from "@/hooks/use-mdm";
import {
  act,
  revealInFolder,
  setQueue,
} from "@/lib/mdm/client";
import { categoryLabel, formatSpeed } from "@/lib/mdm/format";
import type {
  DownloadAction,
  DownloadInfo,
  Filter,
} from "@/lib/mdm/types";

const ACTION_LABELS: Record<DownloadAction, string> = {
  pause: "已暂停",
  start: "已开始",
  remove: "已从列表移除",
  delete: "已删除文件",
};

const BROWSER_GUIDE_KEY = "fucha.browser-guide.dismissed";

function messageOf(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

export default function App() {
  const { data, error, connected, refresh } = useMdm();
  const { integration, refresh: refreshIntegration } = useBrowserIntegration();
  const [filter, setFilter] = useState<Filter>({ kind: "all" });
  const [search, setSearch] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<DownloadInfo | null>(null);
  const [browserGuideDismissed, setBrowserGuideDismissed] = useState(
    () => localStorage.getItem(BROWSER_GUIDE_KEY) === "1",
  );

  const downloads = data?.downloads ?? [];

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return downloads.filter((item) => {
      if (filter.kind === "status" && item.status !== filter.status) {
        return false;
      }
      if (filter.kind === "category" && item.category !== filter.category) {
        return false;
      }
      if (
        query &&
        !`${item.file} ${item.url}`.toLowerCase().includes(query)
      ) {
        return false;
      }
      return true;
    });
  }, [downloads, filter, search]);

  const title =
    filter.kind === "all"
      ? "全部下载"
      : filter.kind === "status"
        ? STATUS_LABELS[filter.status]
        : categoryLabel(
            filter.category,
            data?.categories.find((item) => item.key === filter.category)?.name,
          );

  const downloadingIds = downloads
    .filter((item) => item.status === "Downloading")
    .map((item) => item.id);
  const resumableIds = downloads
    .filter((item) => item.status === "Stopped" || item.status === "Error")
    .map((item) => item.id);
  const completedIds = downloads
    .filter((item) => item.status === "Complete")
    .map((item) => item.id);

  async function handleAction(action: DownloadAction, ids: number[]) {
    if (ids.length === 0) return;
    try {
      const answer = await act(action, ids);
      if (answer.skipped.length > 0) {
        toast.warning(`${ACTION_LABELS[action]} ${answer.done.length} 项`, {
          description: `跳过 ${answer.skipped.length} 项：${answer.skipped
            .map((item) => `#${item.id} ${item.why}`)
            .join("，")}`,
        });
      } else {
        toast.success(`${ACTION_LABELS[action]} ${answer.done.length} 项`);
      }
      await refresh();
    } catch (cause) {
      toast.error("操作失败", { description: messageOf(cause) });
    }
  }

  async function handleQueue(
    action: "start_queue" | "stop_queue",
    queue: string,
  ) {
    try {
      await setQueue(action, queue);
      toast.success(
        action === "start_queue" ? `已启动「${queue}」` : `已停止「${queue}」`,
      );
      await refresh();
    } catch (cause) {
      toast.error("队列操作失败", { description: messageOf(cause) });
    }
  }

  async function handleReveal(path: string) {
    try {
      await revealInFolder(path);
    } catch (cause) {
      toast.error("无法打开文件夹", { description: messageOf(cause) });
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    const target = deleteTarget;
    setDeleteTarget(null);
    await handleAction("delete", [target.id]);
  }

  if (connected === null) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-background">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner />
          正在连接 MDM…
        </div>
      </div>
    );
  }

  if (connected === false && !data) {
    return <ConnectionScreen error={error} onRetry={() => void refresh()} />;
  }

  const activeCount = downloads.filter(
    (item) => item.status === "Downloading",
  ).length;

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background text-foreground">
      <AppSidebar
        data={data}
        filter={filter}
        onFilter={setFilter}
        connected={connected}
        onQueue={(action, queue) => void handleQueue(action, queue)}
        onRefresh={() => void refresh()}
        onSettings={() => setSettingsOpen(true)}
      />

      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center gap-3 border-b px-4 py-3 lg:px-6">
          <div className="min-w-0 flex-1">
            <h1 className="truncate font-heading text-lg font-semibold tracking-tight">
              {title}
            </h1>
            <p className="truncate text-xs text-muted-foreground">
              {filtered.length} 项
              {search.trim() ? ` · 搜索「${search.trim()}」` : ""}
            </p>
          </div>

          <InputGroup className="w-56 lg:w-64">
            <InputGroupAddon>
              <SearchIcon />
            </InputGroupAddon>
            <InputGroupInput
              placeholder="搜索文件或地址"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
            />
          </InputGroup>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline">
                <EllipsisIcon data-icon="inline-start" />
                更多
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuGroup>
                <DropdownMenuItem
                  disabled={downloadingIds.length === 0}
                  onSelect={() => void handleAction("pause", downloadingIds)}
                >
                  全部暂停
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={resumableIds.length === 0}
                  onSelect={() => void handleAction("start", resumableIds)}
                >
                  全部继续
                </DropdownMenuItem>
              </DropdownMenuGroup>
              <DropdownMenuSeparator />
              <DropdownMenuGroup>
                <DropdownMenuItem
                  variant="destructive"
                  disabled={completedIds.length === 0}
                  onSelect={() => void handleAction("remove", completedIds)}
                >
                  清理已完成
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>

          <Button onClick={() => setAddOpen(true)}>
            <PlusIcon data-icon="inline-start" />
            添加下载
          </Button>
        </header>

        <div className="flex min-h-0 flex-1 flex-col gap-4 p-4 lg:p-6">
          {connected === false && (
            <Alert>
              <CircleAlertIcon />
              <AlertTitle>与 MDM 的连接中断</AlertTitle>
              <AlertDescription>
                正在自动重试… 列表可能不是最新的。
              </AlertDescription>
            </Alert>
          )}

          {connected && !browserGuideDismissed && (
            <BrowserIntegrationAlert
              integration={integration}
              onDismiss={() => {
                localStorage.setItem(BROWSER_GUIDE_KEY, "1");
                setBrowserGuideDismissed(true);
              }}
            />
          )}

          <StatsCards data={data} />

          <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border bg-card">
            {filtered.length === 0 ? (
              <div className="flex min-h-0 flex-1 items-center justify-center p-6">
                <Empty className="border border-dashed">
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <InboxIcon />
                    </EmptyMedia>
                    <EmptyTitle>
                      {search.trim() ? "没有匹配的下载" : "这里还没有下载"}
                    </EmptyTitle>
                    <EmptyDescription>
                      {search.trim()
                        ? "换个关键词试试，或清除搜索。"
                        : "添加一个地址，MDM 会用多连接把文件拉下来。"}
                    </EmptyDescription>
                  </EmptyHeader>
                  <EmptyContent>
                    <Button onClick={() => setAddOpen(true)}>
                      <PlusIcon data-icon="inline-start" />
                      添加下载
                    </Button>
                  </EmptyContent>
                </Empty>
              </div>
            ) : (
              <div className="min-h-0 flex-1 overflow-auto">
                <DownloadTable
                  downloads={filtered}
                  onAction={(action, ids) => void handleAction(action, ids)}
                  onReveal={(path) => void handleReveal(path)}
                  onDeleteRequest={setDeleteTarget}
                />
              </div>
            )}
          </section>
        </div>

        <footer className="flex items-center justify-between border-t px-4 py-2 text-xs text-muted-foreground lg:px-6">
          <span>
            共 {downloads.length} 项 · {activeCount} 个进行中
          </span>
          <span className="tabular-nums">
            {formatSpeed(data?.speed ?? 0)}
          </span>
        </footer>
      </main>

      <AddDownloadDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        categories={data?.categories ?? []}
        onAdded={() => void refresh()}
      />

      <DeleteDialog
        download={deleteTarget}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
        onConfirm={() => void confirmDelete()}
      />

      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        activeCount={activeCount}
        onSaved={() => void refresh()}
        integration={integration}
        onRefreshIntegration={() => void refreshIntegration()}
      />
    </div>
  );
}
