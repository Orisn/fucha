import { toast } from "sonner";
import { PuzzleIcon, RefreshCwIcon } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import {
  CHROMIUM_EXTENSION_URL,
  FIREFOX_EXTENSION_URL,
} from "@/lib/extension-links";
import { openExternal } from "@/lib/mdm/client";
import type { BrowserIntegration } from "@/lib/mdm/types";

function openStore(url: string) {
  openExternal(url).catch(() => toast.error("无法打开浏览器"));
}

function browserSummary(integration: BrowserIntegration): string {
  return integration.browsers
    .map((browser) =>
      browser.version ? `${browser.name} ${browser.version}` : browser.name,
    )
    .join("、");
}

interface BrowserIntegrationAlertProps {
  integration: BrowserIntegration | null;
  onDismiss: () => void;
}

export function BrowserIntegrationAlert({
  integration,
  onDismiss,
}: BrowserIntegrationAlertProps) {
  if (!integration || integration.seen) return null;

  return (
    <Alert>
      <PuzzleIcon />
      <AlertTitle>让浏览器下载直接进浮槎</AlertTitle>
      <AlertDescription>
        安装 MDM 的浏览器扩展后，在浏览器里点击下载链接会自动交给浮槎管理。
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <Button size="sm" onClick={() => openStore(CHROMIUM_EXTENSION_URL)}>
            Chrome / Edge 安装
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => openStore(FIREFOX_EXTENSION_URL)}
          >
            Firefox 安装
          </Button>
          <Button size="sm" variant="ghost" onClick={onDismiss}>
            以后再说
          </Button>
        </div>
      </AlertDescription>
    </Alert>
  );
}

interface BrowserIntegrationSectionProps {
  integration: BrowserIntegration | null;
  onRefresh: () => void;
}

export function BrowserIntegrationSection({
  integration,
  onRefresh,
}: BrowserIntegrationSectionProps) {
  return (
    <Field>
      <FieldLabel>浏览器接管</FieldLabel>
      <div className="flex flex-wrap items-center gap-2 text-sm">
        {integration === null ? (
          <span className="text-muted-foreground">正在检测…</span>
        ) : integration.seen ? (
          <>
            <Badge variant="secondary">已检测到</Badge>
            <span className="text-muted-foreground">
              {browserSummary(integration)}
            </span>
          </>
        ) : (
          <span className="text-muted-foreground">
            还没有浏览器扩展连接过 MDM。
          </span>
        )}
      </div>
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => openStore(CHROMIUM_EXTENSION_URL)}
        >
          Chrome / Edge 安装
        </Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => openStore(FIREFOX_EXTENSION_URL)}
        >
          Firefox 安装
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onRefresh}>
          <RefreshCwIcon data-icon="inline-start" />
          重新检测
        </Button>
      </div>
      <FieldDescription>
        扩展由 Mister Download Manager 提供，不是浮槎的一部分。安装后，浏览器里的下载会先交给
        MDM，浮槎会自动显示。商店装不了时，在浏览器扩展页开启开发者模式，加载 MDM
        安装目录下的 extension/chromium 文件夹。
      </FieldDescription>
    </Field>
  );
}
