import type { StatusResponse } from '@/lib/generated/StatusResponse'

import { BUILD_VERSION } from '@/lib/version'
import type { ApiCall, Reply } from './http'
import { stubApi } from './http'

/** A daemon with nothing to report: Moonraker connected, Klipper ready, storage open. */
export const healthyStatus: StatusResponse = {
  version: BUILD_VERSION,
  moonraker: { state: 'connected', message: null, version: 'v0.9.3' },
  klippy: { state: 'ready', message: null },
  storage: { state: 'ready' },
}

/** The path of an API call below `/api/`. */
export function apiPath(call: ApiCall): string {
  return call.url.pathname.replace(/^.*\/api\//, '')
}

/**
 * [`stubApi`] that also answers `GET /api/status` with `status` (healthy by default) and leaves
 * the status calls out of the returned list, so a page test sees only its own requests.
 */
export function stubApiWithStatus(
  handler: (call: ApiCall) => Reply,
  status: StatusResponse | (() => Reply) = healthyStatus,
) {
  const calls: ApiCall[] = []
  stubApi((call) => {
    if (apiPath(call) === 'status') {
      return typeof status === 'function' ? status() : { body: status }
    }
    calls.push(call)
    return handler(call)
  })
  return calls
}
