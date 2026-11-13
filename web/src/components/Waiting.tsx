import type { ReactNode } from "react";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

import { DownloadProgress } from "./DownloadProgress";

export function Waiting({
  children,
  download,
}: {
  children: ReactNode;
  download?: { loaded: number; total: number | null };
}) {
  return (
    <Card role="status" aria-busy="true">
      <CardHeader>
        <Skeleton className="h-3 w-40" />
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
        </div>
        <Skeleton className="h-24 w-full" />
        <p className="text-sm text-muted-foreground">{children}</p>
        {download && <DownloadProgress loaded={download.loaded} total={download.total} />}
      </CardContent>
    </Card>
  );
}
