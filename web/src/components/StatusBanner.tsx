import type { KlippyState } from "@/lib/generated/KlippyState";
import { useStatus } from "@/lib/api/queries";
import { BUILD_VERSION } from "@/lib/version";
import { Button } from "@/components/ui/button";
import type { Tone } from "@/ui-kit/tone";
import { Alert } from "@/ui-kit/Alert";

const KLIPPY_WORDS: Record<KlippyState, string> = {
  unknown: "Klipper's state is unknown",
  disconnected: "Klipper is disconnected",
  startup: "Klipper is starting up",
  ready: "Klipper is ready",
  error: "Klipper reports an error",
  shutdown: "Klipper is shut down",
};

interface Notice {
  tone: Tone;
  /** `alert` for a failure the user may have to act on, `status` for a wait. */
  role: "alert" | "status";
  text: string;
  detail?: string | null;
  /** Offers a reload of the page. */
  reload?: boolean;
}

/** The banner's content, by the precedence of the status request, Moonraker, then Klipper. */
function notice(status: ReturnType<typeof useStatus>): Notice | null {
  if (status.isError) return { tone: "danger", role: "alert", text: status.error.message };
  if (!status.data) return null;
  const { moonraker, klippy } = status.data;
  if (status.data.version !== BUILD_VERSION) {
    return {
      tone: "info",
      role: "alert",
      text: "Wobblescope was updated since this page loaded",
      detail: "Reload to use the new version.",
      reload: true,
    };
  }
  if (moonraker.state === "connecting") {
    return { tone: "warning", role: "status", text: "Connecting to Moonraker" };
  }
  if (moonraker.state !== "connected") {
    return {
      tone: "danger",
      role: "alert",
      text: moonraker.message ?? `Moonraker is ${moonraker.state}`,
    };
  }
  if (klippy.state !== "ready") {
    return {
      tone: klippy.state === "error" || klippy.state === "shutdown" ? "danger" : "warning",
      role: "status",
      text: KLIPPY_WORDS[klippy.state],
      detail: klippy.message,
    };
  }
  return null;
}

/** Tells the user when the daemon, Moonraker or Klipper is unavailable, and nothing otherwise. */
export function StatusBanner() {
  const content = notice(useStatus());
  if (!content) return null;
  return (
    <Alert
      tone={content.tone}
      variant="banner"
      role={content.role}
      action={
        content.reload ? (
          <Button type="button" variant="outline" size="sm" onClick={() => window.location.reload()}>
            Reload
          </Button>
        ) : undefined
      }
    >
      <span className="font-medium">{content.text}</span>
      {content.detail ? <span className="ml-2 text-muted-foreground">{content.detail}</span> : null}
    </Alert>
  );
}
