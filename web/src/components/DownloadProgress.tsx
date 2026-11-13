import { formatBytes } from "@/lib/format";

import { Progress } from "./ui/progress";

export function DownloadProgress({ loaded, total }: { loaded: number; total: number | null }) {
  const value = total ? Math.min(100, (loaded / total) * 100) : null;
  const byteLine = total ? `${formatBytes(loaded)} of ${formatBytes(total)}` : formatBytes(loaded);
  return (
    <div className="space-y-2">
      <Progress value={value} aria-label="Download progress" />
      <p className="text-sm text-muted-foreground">{byteLine}</p>
    </div>
  );
}
