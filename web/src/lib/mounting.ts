import type { Mounting } from "@/lib/generated/Mounting";

/** The places a chip can be mounted, as the form offers them. */
export const MOUNTINGS: { value: Mounting; text: string }[] = [
  { value: "toolhead", text: "Toolhead" },
  { value: "bed", text: "Bed" },
];

/** A recorded mounting for display. Older sessions may hold another string, which is shown as it is. */
export function mountingLabel(mounting: string): string {
  return MOUNTINGS.find((m) => m.value === mounting)?.text ?? mounting;
}
