import { useEffect, useState, type FormEvent } from "react";
import { toast } from "sonner";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { downloadDir as systemDownloadDir } from "@tauri-apps/api/path";
import { cn } from "cn";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Spinner } from "@/components/ui/spinner";
import { CircleAlertIcon, FolderOpenIcon } from "lucide-react";
import {
  applyMdmSettings,
  isTauri,
  readMdmDownloadDir,
} from "@/lib/mdm/client";
import { useStudioSettings, type MdmMode } from "@/lib/studio-settings";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  activeCount: number;
  onSaved: () => void;
}

function messageOf(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

const MODES: { value: MdmMode; title: string; description: string }[] = [
  {
    value: "background",
    title: "后台引擎（推荐）",
    description:
      "MDM 不显示任何窗口，所有添加和进度都在 Fucha 里，不再有经典 Windows 弹窗。",
  },
  {
    value: "window",
    title: "MDM 窗口",
    description:
      "保留 MDM 自己的主窗口、设置和下载弹窗（经典 Windows 风格），在托盘运行。",
  },
];

export function SettingsDialog({
  open,
  onOpenChange,
  activeCount,
  onSaved,
}: SettingsDialogProps) {
  const { settings, save } = useStudioSettings();
  const [dir, setDir] = useState(settings.downloadDir);
  const [mode, setMode] = useState<MdmMode>(settings.mode);
  const [picking, setPicking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    setMode(settings.mode);
    setDir(settings.downloadDir);
    if (settings.downloadDir) return;

    let cancelled = false;
    void (async () => {
      try {
        const configured = await readMdmDownloadDir();
        if (!cancelled && configured) {
          setDir(configured);
          return;
        }
      } catch {
        // The API is off or MDM is not running; fall back to the system folder.
      }
      try {
        const fallback = await systemDownloadDir();
        if (!cancelled) setDir(fallback);
      } catch {
        // Browser preview: no Tauri path API.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, settings.downloadDir, settings.mode]);

  async function pickFolder() {
    if (!isTauri) {
      toast.info("浏览器预览模式不能打开文件夹选择器，请直接输入路径。");
      return;
    }
    setPicking(true);
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: dir.trim() || undefined,
      });
      if (typeof selected === "string") setDir(selected);
    } catch (cause) {
      toast.error("无法选择文件夹", { description: messageOf(cause) });
    } finally {
      setPicking(false);
    }
  }

  async function commit() {
    setConfirmOpen(false);
    setSaving(true);
    const downloadDir = dir.trim();
    try {
      await applyMdmSettings({ mode, downloadDir });
      save({ downloadDir, mode });
      toast.success("设置已保存，MDM 已重启", {
        description:
          mode === "background"
            ? "MDM 正在后台运行，不再显示自己的窗口。"
            : "MDM 已回到托盘运行。",
      });
      onOpenChange(false);
      onSaved();
    } catch (cause) {
      toast.error("保存失败", { description: messageOf(cause) });
    } finally {
      setSaving(false);
    }
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!dir.trim()) {
      toast.error("请选择下载目录");
      return;
    }
    if (activeCount > 0) {
      setConfirmOpen(true);
      return;
    }
    void commit();
  }

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>设置</DialogTitle>
            <DialogDescription>
              下载目录和 MDM 的运行方式。保存会重启 MDM。
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={submit} className="flex flex-col gap-5">
            <FieldGroup className="gap-5">
              <Field>
                <FieldLabel htmlFor="download-dir">默认下载目录</FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id="download-dir"
                    value={dir}
                    onChange={(event) => setDir(event.target.value)}
                    placeholder="例如 F:\下载"
                  />
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void pickFolder()}
                    disabled={picking}
                  >
                    {picking ? (
                      <Spinner data-icon="inline-start" />
                    ) : (
                      <FolderOpenIcon data-icon="inline-start" />
                    )}
                    选择…
                  </Button>
                </div>
                <FieldDescription>
                  MDM 的 General 分类目录；其他分类没有单独设置时都用它。
                </FieldDescription>
              </Field>

              <Field>
                <FieldLabel>MDM 运行方式</FieldLabel>
                <RadioGroup
                  value={mode}
                  onValueChange={(value) => setMode(value as MdmMode)}
                  className="gap-2"
                >
                  {MODES.map((item) => (
                    <label
                      key={item.value}
                      htmlFor={`mode-${item.value}`}
                      className={cn(
                        "flex cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors",
                        mode === item.value && "border-primary bg-primary/5",
                      )}
                    >
                      <RadioGroupItem
                        id={`mode-${item.value}`}
                        value={item.value}
                        className="mt-0.5"
                      />
                      <span className="flex flex-col gap-1">
                        <span className="text-sm font-medium">
                          {item.title}
                        </span>
                        <span className="text-sm text-muted-foreground">
                          {item.description}
                        </span>
                      </span>
                    </label>
                  ))}
                </RadioGroup>
                <FieldDescription>
                  后台模式由 Fucha 管理下载；关掉 Fucha 后 MDM
                  仍会继续运行。
                </FieldDescription>
              </Field>

              {activeCount > 0 && (
                <Alert>
                  <CircleAlertIcon />
                  <AlertTitle>
                    有 {activeCount} 个任务正在下载
                  </AlertTitle>
                  <AlertDescription>
                    保存会重启 MDM，进行中的下载会中断，可以之后继续。
                  </AlertDescription>
                </Alert>
              )}
            </FieldGroup>

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
              >
                取消
              </Button>
              <Button type="submit" disabled={saving}>
                {saving && <Spinner data-icon="inline-start" />}
                保存并重启 MDM
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>重启 MDM？</AlertDialogTitle>
            <AlertDialogDescription>
              有 {activeCount} 个任务正在下载，重启会中断它们。设置会保存。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction onClick={() => void commit()}>
              继续
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
