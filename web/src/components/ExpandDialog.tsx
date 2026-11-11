import { Maximize2Icon } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";

export const EXPANDED_CHART_HEIGHT = "clamp(280px, calc(100vh - 30rem), 640px)";

export function ExpandDialog({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Dialog>
      <DialogTrigger
        render={<Button type="button" variant="outline" size="icon" aria-label="Expand the chart and table" />}
      >
        <Maximize2Icon />
      </DialogTrigger>
      <DialogContent className="max-h-[calc(100vh-4rem)] gap-4 overflow-auto sm:max-w-[min(calc(100vw-8rem),1500px)]">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <div className="space-y-4">{children}</div>
      </DialogContent>
    </Dialog>
  );
}
