import { useEffect, useRef } from "react";
import { toast } from "sonner";

import type { ChipLive } from "@/lib/generated/ChipLive";

/**
 * Shows each chip warning once as a toast that stays until it is closed or the user leaves the
 * page. `testId` keeps a warning of an earlier test from counting as seen.
 */
export function useWarningToasts(testId: string, chips: ChipLive[]) {
  const shown = useRef(new Set<string>());
  useEffect(() => {
    for (const chip of chips) {
      for (const warning of chip.warnings) {
        const id = `${testId}:${chip.chip}:${warning}`;
        if (shown.current.has(id)) continue;
        shown.current.add(id);
        toast.warning(chip.chip, { id, description: warning, duration: Number.POSITIVE_INFINITY, closeButton: true });
      }
    }
  }, [testId, chips]);
}
