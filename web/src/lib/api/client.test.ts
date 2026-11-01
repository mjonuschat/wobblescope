import { describe, expect, it, vi } from 'vitest'

import { setBase, stubFetch } from '@/test/http'

import { ApiClientError, apiJson } from './client'

describe('the fetch wrapper', () => {
  it('resolves the path against the page so a proxy prefix is kept', async () => {
    setBase('http://printer.local/wobblescope/')
    const http = stubFetch({ body: {} })
    await apiJson('storage')
    expect(http.url()).toBe('http://printer.local/wobblescope/api/storage')
  })

  it.each(['GET', 'PATCH', 'DELETE', 'POST'])('sends the client header on %s', async (method) => {
    const http = stubFetch({ body: {} })
    await apiJson('storage', { method })
    expect(http.header(0, 'X-Wobblescope-Client')).toBe('1')
  })

  it('turns a JSON error body into an ApiClientError with status and code', async () => {
    stubFetch({ status: 409, body: { error: 'pinned', message: 'm' } })
    const error = await apiJson('sessions/s1').catch((e: unknown) => e)
    expect(error).toBeInstanceOf(ApiClientError)
    expect(error).toMatchObject({ status: 409, code: 'pinned', message: 'm' })
  })

  it('exposes the byte counts of a cap refusal', async () => {
    const body = {
      error: 'cap_exceeded',
      message: 'too big',
      need: 5,
      cap: 10,
      pinned_bytes: 7,
      evictable_bytes: 1,
    }
    stubFetch({ status: 507, body })
    const error = (await apiJson('sessions').catch((e: unknown) => e)) as ApiClientError
    expect(error.body).toMatchObject({ need: 5, cap: 10, pinned_bytes: 7, evictable_bytes: 1 })
  })

  it('names the status when the error body is an HTML page', async () => {
    stubFetch({ status: 502, text: '<html>Bad gateway</html>' })
    const error = (await apiJson('sessions').catch((e: unknown) => e)) as ApiClientError
    expect(error.code).toBeNull()
    expect(error.status).toBe(502)
    expect(error.message).toContain('502')
  })

  it('says the daemon could not be reached when fetch rejects', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('failed')))
    const error = (await apiJson('sessions').catch((e: unknown) => e)) as ApiClientError
    expect(error).toBeInstanceOf(ApiClientError)
    expect(error.status).toBeNull()
    expect(error.message).toContain('could not be reached')
  })

  it('lets an abort through unchanged', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockRejectedValue(new DOMException('aborted', 'AbortError')),
    )
    const error = await apiJson('sessions').catch((e: unknown) => e)
    expect(error).toBeInstanceOf(DOMException)
  })

  it('resolves a 204 without reading a body', async () => {
    stubFetch({ status: 204 })
    await expect(apiJson('sessions/s1', { method: 'DELETE' })).resolves.toBeUndefined()
  })
})
