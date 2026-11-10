import { useState } from "react";

import { Button } from "@/components/ui/button";

/**
 * Copies text to the clipboard. Where the browser has none (a page served over plain HTTP has
 * no clipboard), it says so and the text stays selectable on screen.
 */
export function CopyButton({ text }: { text: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

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
      <Button type="button" variant="outline" size="sm" onClick={() => void copy()}>
        Copy
      </Button>
      <span role="status" className="text-xs text-muted-foreground">
        {state === "copied" && "Copied"}
        {state === "failed" && "Copy failed. Select the text and copy it."}
      </span>
    </span>
  );
}
