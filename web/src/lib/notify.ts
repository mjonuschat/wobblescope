import { toast } from "sonner";

/** Shows a failure that stays until it is closed, since the message may be long and say what to do. */
export function reportError(message: string) {
  toast.error(message, { id: message, duration: Number.POSITIVE_INFINITY, closeButton: true });
}
