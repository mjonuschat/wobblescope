import { useCallback, useEffect, useRef, useState } from "react";

/** How long the second press of a confirm button counts as confirming the first. */
export const CONFIRM_WINDOW_MS = 2000;

/**
 * A button that needs two presses within [`CONFIRM_WINDOW_MS`]. The first arms it, the second
 * calls `onConfirm`, and it disarms by itself when the window passes.
 */
export function useConfirmClick(
  onConfirm: () => void,
  windowMs: number = CONFIRM_WINDOW_MS,
): { armed: boolean; click: () => void } {
  const [armed, setArmed] = useState(false);
  const armedRef = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }, []);

  useEffect(() => clearTimer, [clearTimer]);

  const click = useCallback(() => {
    clearTimer();
    if (armedRef.current) {
      armedRef.current = false;
      setArmed(false);
      onConfirm();
      return;
    }
    armedRef.current = true;
    setArmed(true);
    timer.current = setTimeout(() => {
      armedRef.current = false;
      setArmed(false);
    }, windowMs);
  }, [clearTimer, onConfirm, windowMs]);

  return { armed, click };
}
