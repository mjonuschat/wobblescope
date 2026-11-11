import type { ReactNode } from "react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface Props {
  title: string;
  /** The accessible name of the region, when it should differ from the title. */
  label?: string;
  /** Something for the right end of the title row, such as a button. */
  action?: ReactNode;
  children: ReactNode;
}

/** A card with a small uppercase title, and an optional control at the right of the title row. */
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
