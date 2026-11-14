import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { ThemeToggle } from "@/components/theme-toggle";
import { BrandMark } from "@/components/brand-mark";
import {
  CircleAlertIcon,
  CopyIcon,
  ExternalLinkIcon,
  RefreshCwIcon,
} from "lucide-react";
import { ipcPath, openExternal } from "@/lib/mdm/client";

interface ConnectionScreenProps {
  error: string | null;
  onRetry: () => void;
}

export function ConnectionScreen({ error, onRetry }: ConnectionScreenProps) {
  const [path, setPath] = useState("");

  useEffect(() => {
    ipcPath()
      .then(setPath)
      .catch(() => undefined);
  }, []);

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast.success("已复制");
    } catch {
      toast.error("复制失败");
    }
  }

  return (
    <div className="relative flex min-h-screen items-center justify-center bg-background p-6">
      <div className="absolute top-4 right-4">
        <ThemeToggle />
      </div>

      <Card className="w-full max-w-xl">
        <CardHeader>
          <div className="mb-2 flex size-10 items-center justify-center rounded-xl bg-primary text-primary-foreground">
            <BrandMark className="size-6" />
          </div>
          <CardTitle>连接 Mister Download Manager</CardTitle>
          <CardDescription>
            Fucha 只是界面，下载由 MDM 的引擎完成。启动 MDM 并打开它的本地自动化
            API 即可连接。
          </CardDescription>
        </CardHeader>

        <CardContent className="flex flex-col gap-4">
          <Alert variant="destructive">
            <CircleAlertIcon />
            <AlertTitle>无法连接 MDM</AlertTitle>
            <AlertDescription>{error ?? "请确认 MDM 正在运行。"}</AlertDescription>
          </Alert>

          <ol className="flex list-decimal flex-col gap-3 pl-5 text-sm text-muted-foreground">
            <li>
              安装并启动 Mister Download Manager（v0.5.2 或更新）。
            </li>
            <li>
              <div className="flex flex-col gap-2">
                <span>打开自动化 API，只需执行一次：</span>
                <div className="flex items-center gap-2 rounded-lg border bg-muted/50 px-3 py-2">
                  <code className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">
                    misterdownloadmanager-cli api on
                  </code>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label="复制命令"
                    onClick={() => copy("misterdownloadmanager-cli api on")}
                  >
                    <CopyIcon />
                  </Button>
                </div>
              </div>
            </li>
            <li>回到这里点「重试」。</li>
          </ol>

          <div className="flex flex-col gap-1 rounded-lg border bg-muted/30 px-3 py-2">
            <span className="text-xs text-muted-foreground">
              MDM 的端口和 token 从这里读取：
            </span>
            <code className="truncate font-mono text-xs" title={path}>
              {path || "…"}
            </code>
          </div>
        </CardContent>

        <CardFooter className="flex items-center gap-2">
          <Button onClick={onRetry}>
            <RefreshCwIcon data-icon="inline-start" />
            重试
          </Button>
          <Button
            variant="outline"
            onClick={() =>
              openExternal("https://misterdownloadmanager.com").catch(() =>
                toast.error("无法打开浏览器"),
              )
            }
          >
            <ExternalLinkIcon data-icon="inline-start" />
            下载 MDM
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}
