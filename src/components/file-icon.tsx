import {
  ArchiveIcon,
  FileIcon as LucideFileIcon,
  FilmIcon,
  MusicIcon,
  PackageIcon,
  FileTextIcon,
  MagnetIcon,
  type LucideIcon,
} from "lucide-react";
import { cn } from "cn";
import { extensionOf } from "@/lib/mdm/format";

const ICONS: Record<string, LucideIcon> = {
  mp4: FilmIcon,
  mkv: FilmIcon,
  webm: FilmIcon,
  avi: FilmIcon,
  mov: FilmIcon,
  mp3: MusicIcon,
  flac: MusicIcon,
  wav: MusicIcon,
  m4a: MusicIcon,
  opus: MusicIcon,
  pdf: FileTextIcon,
  doc: FileTextIcon,
  docx: FileTextIcon,
  xls: FileTextIcon,
  xlsx: FileTextIcon,
  csv: FileTextIcon,
  zip: ArchiveIcon,
  rar: ArchiveIcon,
  "7z": ArchiveIcon,
  tar: ArchiveIcon,
  gz: ArchiveIcon,
  xz: ArchiveIcon,
  exe: PackageIcon,
  msi: PackageIcon,
  iso: PackageIcon,
  dmg: PackageIcon,
  torrent: MagnetIcon,
};

export function FileIcon({ file, className }: { file: string; className?: string }) {
  const Icon = ICONS[extensionOf(file)] ?? LucideFileIcon;
  return <Icon className={cn("text-muted-foreground", className)} />;
}
