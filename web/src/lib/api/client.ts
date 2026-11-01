import type { ApiError } from '@/lib/generated/ApiError'

/** What the app sends on every request; the daemon's write guard looks for it. */
const CLIENT_HEADER = 'X-Wobblescope-Client'

/**
 * A failed API call. `body` is the daemon's JSON error when it sent one. `status` is null when
 * the daemon could not be reached.
 */
export class ApiClientError extends Error {
  readonly status: number | null
  readonly body: ApiError | null

  constructor(message: string, status: number | null, body: ApiError | null = null) {
    super(message)
    this.name = 'ApiClientError'
    this.status = status
    this.body = body
  }

  /** The daemon's stable error code, when it sent one. */
  get code(): ApiError['error'] | null {
    return this.body?.error ?? null
  }
}

/** Resolves an API path against the page, so a reverse-proxy path prefix is kept. */
export function apiUrl(path: string): URL {
  return new URL(`api/${path}`, document.baseURI)
}

function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { error?: unknown }).error === 'string' &&
    typeof (value as { message?: unknown }).message === 'string'
  )
}

async function toError(response: Response): Promise<ApiClientError> {
  const text = await response.text().catch(() => '')
  try {
    const parsed: unknown = JSON.parse(text)
    if (isApiError(parsed)) {
      return new ApiClientError(parsed.message, response.status, parsed)
    }
  } catch {
    // Not JSON, such as a proxy's error page.
  }
  return new ApiClientError(
    `The daemon answered ${response.status} ${response.statusText}`.trim(),
    response.status,
  )
}

/** Sends one request to `/api/<path>` with the client header and returns the response. */
export async function apiFetch(path: string | URL, init: RequestInit = {}): Promise<Response> {
  const url = typeof path === 'string' ? apiUrl(path) : path
  const headers = new Headers(init.headers)
  headers.set(CLIENT_HEADER, '1')
  let response: Response
  try {
    response = await fetch(url, { ...init, headers })
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') throw error
    throw new ApiClientError('The daemon could not be reached. Is it running?', null)
  }
  if (!response.ok) throw await toError(response)
  return response
}

/** Sends a request and parses the JSON body. A 204 resolves to `undefined`. */
export async function apiJson<T>(path: string | URL, init: RequestInit = {}): Promise<T> {
  const response = await apiFetch(path, init)
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}
