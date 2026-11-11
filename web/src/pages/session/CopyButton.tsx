import { CheckIcon, CopyIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/** How long "Copied" stays on screen, in milliseconds. */
const CONFIRMATION_MS = 2000;

/**
 * Copies text to the clipboard. Where the browser has none (a page served over plain HTTP has
 * no clipboard), it says so and the text stays selectable on screen.
 */
export function CopyButton({ text }: { text: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  useEffect(() => {
    if (state !== "copied") return;
    const timer = setTimeout(() => setState("idle"), CONFIRMATION_MS);
    return () => clearTimeout(timer);
  }, [state]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setState("copied");
    } catch {
      setState("failed");
    }
  }

  return (
    <span className="inline-flex items-center gap-2">
      <span role="status" className="text-xs text-muted-foreground">
        {state === "copied" && "Copied"}
        {state === "failed" && "Copy failed. Select the text and copy it."}
      </span>
      <Tooltip>
        <TooltipTrigger
          render={<Button type="button" variant="outline" size="icon" aria-label="Copy" onClick={() => void copy()} />}
        >
          {state === "copied" ? <CheckIcon className="text-emerald-400" /> : <CopyIcon />}
        </TooltipTrigger>
        <TooltipContent side="top">Copy to clipboard</TooltipContent>
      </Tooltip>
    </span>
  );
}
