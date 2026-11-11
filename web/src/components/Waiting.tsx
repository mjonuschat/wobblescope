import { LoaderCircleIcon } from "lucide-react";
import type { ReactNode } from "react";

import { Card, CardContent } from "@/components/ui/card";

/** A card with a spinner and a line of text, for something that is being calculated or loaded. */
export function Waiting({ children }: { children: ReactNode }) {
  return (
    <Card role="status">
      <CardContent className="flex flex-col items-center gap-3 py-10 text-sm text-muted-foreground">
        <LoaderCircleIcon aria-hidden="true" className="size-6 animate-spin" />
        <p>{children}</p>
      </CardContent>
    </Card>
  );
}
