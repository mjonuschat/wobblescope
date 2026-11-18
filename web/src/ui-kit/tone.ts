import { CircleCheckIcon, InfoIcon, OctagonXIcon, TriangleAlertIcon, type LucideIcon } from "lucide-react";

/** The four meanings a status color can carry. The class strings are literal so Tailwind sees them. */
export type Tone = "success" | "warning" | "danger" | "info";

export const toneText: Record<Tone, string> = {
  success: "text-success",
  warning: "text-warning",
  danger: "text-destructive",
  info: "text-info",
};

export const toneDot: Record<Tone, string> = {
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-destructive",
  info: "bg-info",
};

export const toneBox: Record<Tone, string> = {
  success: "border-success/40 bg-success/10",
  warning: "border-warning/40 bg-warning/10",
  danger: "border-destructive/40 bg-destructive/10",
  info: "border-info/40 bg-info/10",
};

/** The icon that goes with a tone, so a status never relies on color alone. */
export const toneIcon: Record<Tone, LucideIcon> = {
  success: CircleCheckIcon,
  warning: TriangleAlertIcon,
  danger: OctagonXIcon,
  info: InfoIcon,
};
