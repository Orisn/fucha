import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Spinner } from "@/components/ui/spinner";
import { addDownload } from "@/lib/mdm/client";
import { categoryLabel } from "@/lib/mdm/format";
import type { CategoryInfo } from "@/lib/mdm/types";

interface AddDownloadDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  categories: CategoryInfo[];
  onAdded: () => void;
}

export function AddDownloadDialog({
  open,
  onOpenChange,
  categories,
  onAdded,
}: AddDownloadDialogProps) {
  const [url, setUrl] = useState("");
  const [dir, setDir] = useState("");
  const [name, setName] = useState("");
  const [category, setCategory] = useState("auto");
  const [later, setLater] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!url.trim()) return;
    setSubmitting(true);
    try {
      await addDownload({
        url,
        dir: dir.trim() || undefined,
        name: name.trim() || undefined,
        category: category === "auto" ? undefined : category,
        later,
      });
      toast.success("已添加到 MDM", {
        description: later ? "任务已加入队列，等待启动。" : "下载已开始。",
      });
      setUrl("");
      setDir("");
      setName("");
      setCategory("auto");
      setLater(false);
      onOpenChange(false);
      onAdded();
    } catch (cause) {
      toast.error("添加失败", {
        description: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>添加下载</DialogTitle>
          <DialogDescription>
            地址会发送给正在运行的 Mister Download Manager。
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex flex-col gap-5">
          <FieldGroup className="gap-4">
            <Field>
              <FieldLabel htmlFor="download-url">下载地址</FieldLabel>
              <Input
                id="download-url"
                autoFocus
                required
                placeholder="https://example.com/file.zip"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
              />
              <FieldDescription>
                支持 http、https、ftp、ftps；磁力链接会交给 MDM 的种子窗口处理。
              </FieldDescription>
            </Field>

            <div className="grid gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor="download-dir">保存到</FieldLabel>
                <Input
                  id="download-dir"
                  placeholder="留空使用分类默认文件夹"
                  value={dir}
                  onChange={(event) => setDir(event.target.value)}
                />
              </Field>

              <Field>
                <FieldLabel htmlFor="download-name">文件名</FieldLabel>
                <Input
                  id="download-name"
                  placeholder="留空使用服务器文件名"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </Field>
            </div>

            <Field>
              <FieldLabel htmlFor="download-category">分类</FieldLabel>
              <Select value={category} onValueChange={setCategory}>
                <SelectTrigger id="download-category" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="auto">自动识别</SelectItem>
                    {categories.map((item) => (
                      <SelectItem key={item.key} value={item.key}>
                        {categoryLabel(item.key, item.name)}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
              <FieldDescription>
                分类决定文件保存到哪个文件夹。
              </FieldDescription>
            </Field>

            <Field orientation="horizontal">
              <FieldContent>
                <FieldLabel htmlFor="download-later">稍后开始</FieldLabel>
                <FieldDescription>
                  加入主下载队列，稍后再启动。
                </FieldDescription>
              </FieldContent>
              <Switch
                id="download-later"
                checked={later}
                onCheckedChange={setLater}
              />
            </Field>
          </FieldGroup>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              取消
            </Button>
            <Button type="submit" disabled={submitting || !url.trim()}>
              {submitting && <Spinner data-icon="inline-start" />}
              添加下载
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
