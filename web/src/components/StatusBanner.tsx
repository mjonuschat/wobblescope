import type { KlippyState } from '@/lib/generated/KlippyState'
import { useStatus } from '@/lib/api/queries'

const KLIPPY_WORDS: Record<KlippyState, string> = {
  unknown: "Klipper's state is unknown",
  disconnected: 'Klipper is disconnected',
  startup: 'Klipper is starting up',
  ready: 'Klipper is ready',
  error: 'Klipper reports an error',
  shutdown: 'Klipper is shut down',
}

interface Notice {
  /** `alert` for a failure the user may have to act on, `status` for a wait. */
  role: 'alert' | 'status'
  text: string
  detail?: string | null
}

/** The banner's content, by the precedence of the status request, Moonraker, then Klipper. */
function notice(
  status: ReturnType<typeof useStatus>,
): Notice | null {
  if (status.isError) return { role: 'alert', text: status.error.message }
  if (!status.data) return null
  const { moonraker, klippy } = status.data
  if (moonraker.state === 'connecting') {
    return { role: 'status', text: 'Connecting to Moonraker' }
  }
  if (moonraker.state !== 'connected') {
    return {
      role: 'alert',
      text: moonraker.message ?? `Moonraker is ${moonraker.state}`,
    }
  }
  if (klippy.state !== 'ready') {
    return { role: 'status', text: KLIPPY_WORDS[klippy.state], detail: klippy.message }
  }
  return null
}

/** Tells the user when the daemon, Moonraker or Klipper is unavailable, and nothing otherwise. */
export function StatusBanner() {
  const content = notice(useStatus())
  if (!content) return null
  return (
    <div
      role={content.role}
      className="border-b border-amber-800 bg-amber-950 px-6 py-2 text-sm text-amber-100"
    >
      <span className="font-medium">{content.text}</span>
      {content.detail ? <span className="ml-2 opacity-80">{content.detail}</span> : null}
    </div>
  )
}
