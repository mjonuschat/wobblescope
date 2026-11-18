import type { ReactNode } from "react";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { CardLabel } from "@/ui-kit/CardLabel";

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
        <CardLabel>{title}</CardLabel>
        {action}
      </CardHeader>
      <CardContent className="space-y-4">{children}</CardContent>
    </Card>
  );
}
