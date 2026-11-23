import { CheckIcon, CopyIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

const CONFIRMATION_MS = 2000;

/**
 * Copies with a hidden textarea and `execCommand`, which still works on a page served over
 * plain HTTP, where `navigator.clipboard` does not exist.
 */
function legacyCopy(text: string): boolean {
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  document.body.appendChild(area);
  try {
    area.select();
    return typeof document.execCommand === "function" && document.execCommand("copy");
  } catch {
    return false;
  } finally {
    area.remove();
    previous?.focus();
  }
}

/**
 * Copies text to the clipboard, falling back to `execCommand` where the browser has no
 * clipboard API. If both fail it says so and the text stays selectable on screen.
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
      return;
    } catch {
      // No clipboard API (insecure context) or permission denied: try the legacy path.
    }
    setState(legacyCopy(text) ? "copied" : "failed");
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
          {state === "copied" ? <CheckIcon className="text-success" /> : <CopyIcon />}
        </TooltipTrigger>
        <TooltipContent side="top">Copy to clipboard</TooltipContent>
      </Tooltip>
    </span>
  );
}
