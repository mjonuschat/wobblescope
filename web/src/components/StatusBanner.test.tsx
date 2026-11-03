import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { createQueryClient, queryKeys } from '@/lib/api/queries'
import type { StatusResponse } from '@/lib/generated/StatusResponse'
import { BUILD_VERSION } from '@/lib/version'
import { healthyStatus, stubApiWithStatus } from '@/test/status'

import { StatusBanner } from './StatusBanner'

function renderBanner() {
  const client = createQueryClient()
  render(
    <QueryClientProvider client={client}>
      <StatusBanner />
    </QueryClientProvider>,
  )
  return client
}

/** Serves `status`, renders the banner and waits until the status query has an outcome. */
async function showing(status: StatusResponse) {
  stubApiWithStatus(() => ({ status: 404 }), status)
  const client = renderBanner()
  await waitFor(() => expect(client.getQueryState(queryKeys.status)?.status).toBe('success'))
  return client
}

const unreachable: StatusResponse = {
  ...healthyStatus,
  moonraker: {
    state: 'unreachable',
    message: 'cannot connect to Moonraker at http://127.0.0.1:7125: connection refused',
    version: null,
  },
  klippy: { state: 'unknown', message: null },
}

describe('the status banner', () => {
  it('offers a reload when the daemon runs another build than this page', async () => {
    const reload = vi.fn()
    vi.stubGlobal('location', { ...window.location, reload })
    await showing({ ...healthyStatus, version: `${BUILD_VERSION}-newer` })
    const banner = await screen.findByRole('alert')
    expect(banner.textContent).toContain('updated')
    fireEvent.click(screen.getByRole('button', { name: 'Reload' }))
    expect(reload).toHaveBeenCalledTimes(1)
  })

  it('does not offer a reload when the builds match', async () => {
    await showing(healthyStatus)
    expect(screen.queryByRole('button', { name: 'Reload' })).toBeNull()
  })

  it('puts the stale page ahead of Moonraker trouble', async () => {
    await showing({ ...unreachable, version: `${BUILD_VERSION}-newer` })
    expect(await screen.findByRole('button', { name: 'Reload' })).toBeTruthy()
  })

  it('shows nothing while the first status request is pending', () => {
    vi.stubGlobal('fetch', vi.fn(() => new Promise(() => {})))
    renderBanner()
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByRole('status')).toBeNull()
  })

  it('shows nothing when Moonraker is connected and Klipper is ready', async () => {
    await showing(healthyStatus)
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByRole('status')).toBeNull()
  })

  it('shows the reason when Moonraker is unreachable, as an alert', async () => {
    await showing(unreachable)
    const banner = await screen.findByRole('alert')
    expect(banner.textContent).toContain(
      'cannot connect to Moonraker at http://127.0.0.1:7125: connection refused',
    )
  })

  it('shows the daemon message, naming both fixes, when Moonraker is unauthorized', async () => {
    await showing({
      ...unreachable,
      moonraker: {
        state: 'unauthorized',
        message: 'Add the daemon to trusted_clients, or set moonraker_api_key.',
        version: null,
      },
    })
    const banner = await screen.findByRole('alert')
    expect(banner.textContent).toContain('trusted_clients')
    expect(banner.textContent).toContain('moonraker_api_key')
  })

  it('says it is connecting, as a status, while Moonraker is connecting', async () => {
    await showing({
      ...unreachable,
      moonraker: { state: 'connecting', message: null, version: null },
    })
    const banner = await screen.findByRole('status')
    expect(banner.textContent).toContain('Connecting to Moonraker')
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('names the Klipper state and its message when Klipper is not ready', async () => {
    await showing({
      ...healthyStatus,
      klippy: { state: 'shutdown', message: 'Timer too close' },
    })
    const banner = await screen.findByRole('status')
    expect(banner.textContent).toContain('Klipper is shut down')
    expect(banner.textContent).toContain('Timer too close')
  })

  it('shows a message when the status request is rejected', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new TypeError('Failed to fetch'))))
    renderBanner()
    const banner = await screen.findByRole('alert')
    expect(banner.textContent).toContain('The daemon could not be reached. Is it running?')
  })

  it('clears by itself when the next poll is healthy', async () => {
    let answer: StatusResponse = unreachable
    stubApiWithStatus(() => ({ status: 404 }), () => ({ body: answer }))
    const client = renderBanner()
    await screen.findByRole('alert')
    answer = healthyStatus
    await client.invalidateQueries({ queryKey: queryKeys.status })
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull())
  })

  it('fails a status request that never settles after its deadline, replacing a healthy answer', async () => {
    let hang = false
    vi.stubGlobal(
      'fetch',
      vi.fn((): Promise<Response> =>
        hang
          ? new Promise(() => {})
          : Promise.resolve(new Response(JSON.stringify(healthyStatus), { status: 200 })),
      ),
    )
    const client = renderBanner()
    await waitFor(() => expect(client.getQueryState(queryKeys.status)?.status).toBe('success'))
    expect(screen.queryByRole('alert')).toBeNull()

    vi.useFakeTimers()
    try {
      hang = true
      void client.invalidateQueries({ queryKey: queryKeys.status })
      await vi.advanceTimersByTimeAsync(4100)
    } finally {
      vi.useRealTimers()
    }
    const banner = await screen.findByRole('alert')
    expect(banner.textContent).toContain('The daemon did not answer in time')
  })
})
