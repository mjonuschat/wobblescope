import type { ComponentProps } from "react";

import { CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

/** The title of a card or panel, in sentence case and the primary text color. */
export function CardLabel({ className, ...props }: ComponentProps<typeof CardTitle>) {
  return <CardTitle className={cn("text-base font-medium text-foreground", className)} {...props} />;
}

/** A heading inside a card, below its title. */
export const subheadingClass = "text-xs font-medium text-muted-foreground";
