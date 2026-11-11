import type { ReactNode } from "react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface Props {
  title: string;
  label?: string;
  action?: ReactNode;
  children: ReactNode;
}

export function Panel({ title, label, action, children }: Props) {
  return (
    <Card role="region" aria-label={label ?? title}>
      <CardHeader className="flex items-center justify-between gap-4">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{title}</CardTitle>
        {action}
      </CardHeader>
      <CardContent className="space-y-4">{children}</CardContent>
    </Card>
  );
}
