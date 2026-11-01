import { fireEvent, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { ChipsSnapshot } from '@/lib/generated/ChipsSnapshot'
import type { PrinterSettings } from '@/lib/generated/PrinterSettings'
import { queryKeys } from '@/lib/api/queries'
import { chipsSnapshot, printerSettings } from '@/test/capture'
import type { ApiCall, Reply } from '@/test/http'
import { renderRoute } from '@/test/render'
import { apiPath, healthyStatus, stubApiWithStatus } from '@/test/status'

interface Options {
  settings?: PrinterSettings
  chips?: ChipsSnapshot
  checked?: ChipsSnapshot
  start?: Reply | ((call: ApiCall) => Reply)
}

function api({ settings = printerSettings(), chips = chipsSnapshot(), checked, start }: Options = {}) {
  return stubApiWithStatus((call) => {
    const path = apiPath(call)
    if (path === 'settings') return { body: settings }
    if (path === 'chips') return { body: chips }
    if (path === 'chips/check') return { body: checked ?? chips }
    if (path === 'tests' && call.method === 'POST') {
      const reply = typeof start === 'function' ? start(call) : start
      return reply ?? { status: 202, body: { id: 'test-1' } }
    }
    return { status: 404, body: { error: 'not_found', message: 'no' } }
  })
}

const startButton = () => screen.getByRole('button', { name: /Start test|Checking printer/ })
const field = (label: RegExp | string) => screen.getByLabelText(label) as HTMLInputElement
const posted = (calls: ApiCall[]) => calls.filter((c) => c.method === 'POST' && apiPath(c) === 'tests')

async function open(options?: Options) {
  const calls = api(options)
  renderRoute('/')
  await screen.findByRole('button', { name: 'Start test' })
  return calls
}

describe('the run screen', () => {
  it('shows the printer values as placeholders, preselects the mode and enables Start', async () => {
    await open()
    expect(field(/End frequency/).placeholder).toBe('135')
    expect(field(/Acceleration per Hz/).placeholder).toBe('60')
    expect((screen.getByLabelText('Sweep') as HTMLInputElement).checked).toBe(true)
    expect(field('Use lis2dw ebb36').checked).toBe(true)
    expect(field('Use adxl345').checked).toBe(false)
    expect((startButton() as HTMLButtonElement).disabled).toBe(false)
  })

  it('preselects pulse when the printer sweeps with a period of 0', async () => {
    await open({ settings: { ...printerSettings(), default_mode: 'Pulse' } })
    expect((screen.getByLabelText('Pulse') as HTMLInputElement).checked).toBe(true)
  })

  it('shows the emergency stop consequence before a test starts', async () => {
    await open()
    expect(screen.getByText(/Emergency stop puts Klipper into shutdown/)).toBeTruthy()
    expect(screen.getByText(/FIRMWARE_RESTART/)).toBeTruthy()
  })

  it('marks an unreadable freq_end as required and keeps Start disabled until it has a value', async () => {
    await open({
      settings: printerSettings({ freq_end: { value: null, overridable: true, needed: true } }),
    })
    const input = field(/End frequency/)
    expect(input.placeholder).toBe('required')
    expect(input.getAttribute('aria-required')).toBe('true')
    expect((startButton() as HTMLButtonElement).disabled).toBe(true)
    fireEvent.change(input, { target: { value: '100' } })
    expect((startButton() as HTMLButtonElement).disabled).toBe(false)
  })

  it('shows two points and two axes as 4 segments and changes the time when hz_per_sec is overridden', async () => {
    await open()
    fireEvent.click(screen.getByRole('button', { name: 'Add a point' }))
    // 130 s of sweep plus 15 s of overhead per segment.
    expect(screen.getByText(/4 segments · about 9 min 40 s/)).toBeTruthy()
    fireEvent.change(field(/Hz per second/), { target: { value: '2' } })
    // 65 s plus 15 s per segment.
    expect(screen.getByText(/4 segments · about 5 min 20 s/)).toBeTruthy()
  })

  it('posts only the overridden fields with the chosen chips and points', async () => {
    const calls = await open()
    fireEvent.change(field(/End frequency/), { target: { value: '100' } })
    fireEvent.click(screen.getByLabelText('Y'))
    fireEvent.click(startButton())
    await waitFor(() => expect(posted(calls)).toHaveLength(1))
    expect(JSON.parse(String(posted(calls)[0].body))).toEqual({
      chips: ['lis2dw ebb36'],
      axes: ['X'],
      mode: 'Sweep',
      overrides: { freq_end: 100 },
      points: [[150.5, 154, 20]],
    })
  })

  it('shows a daemon message beside the field it names', async () => {
    await open({
      start: {
        status: 400,
        body: { error: 'invalid_request', message: 'hz_per_sec: must be above 0 and at most 2' },
      },
    })
    fireEvent.click(startButton())
    const alert = await screen.findByText('must be above 0 and at most 2')
    expect(alert.id).toBe('error-hz_per_sec')
    expect(field(/Hz per second/).getAttribute('aria-invalid')).toBe('true')
  })

  it('lists every failed pre-flight check', async () => {
    await open({
      start: {
        status: 409,
        body: {
          error: 'preflight_failed',
          message: '2 checks failed',
          failures: [
            { check: 'homed', message: 'home X, Y and Z first; not homed: Z' },
            { check: 'toolhead_idle', message: 'the toolhead is Printing; wait until it is idle' },
          ],
        },
      },
    })
    fireEvent.click(startButton())
    expect(await screen.findByText('home X, Y and Z first; not homed: Z')).toBeTruthy()
    expect(screen.getByText('the toolhead is Printing; wait until it is idle')).toBeTruthy()
  })

  it('disables Start while the request is pending and sends nothing on a second click', async () => {
    let release: (response: Response) => void = () => {}
    const pending = new Promise<Response>((resolve) => {
      release = resolve
    })
    let posts = 0
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const path = new URL(String(input)).pathname.replace(/^.*\/api\//, '')
        if (path === 'tests') {
          posts += 1
          return pending
        }
        const body =
          path === 'status' ? healthyStatus : path === 'settings' ? printerSettings() : chipsSnapshot()
        return new Response(JSON.stringify(body), { status: 200 })
      }),
    )
    renderRoute('/')
    fireEvent.click(await screen.findByRole('button', { name: 'Start test' }))
    const busy = await screen.findByRole('button', { name: 'Checking printer…' })
    expect((busy as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(busy)
    fireEvent.submit(busy.closest('form')!)
    expect(posts).toBe(1)
    release(new Response(JSON.stringify({ id: 'test-1' }), { status: 202 }))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Start test' })).toBeTruthy())
    expect(posts).toBe(1)
  })

  it('keeps the sweeping fields off in pulse mode', async () => {
    await open()
    fireEvent.click(screen.getByLabelText('Pulse'))
    expect(field(/Sweeping period/).disabled).toBe(true)
    expect(field(/Sweeping acceleration/).disabled).toBe(true)
    expect(within(document.body).getByLabelText(/End frequency/)).toBeTruthy()
  })

  it('shows the form once the printer becomes ready after the page opened during startup', async () => {
    let ready = false
    const starting = {
      moonraker: { state: 'connecting' as const, message: null, version: null },
      klippy: { state: 'unknown' as const, message: null },
      storage: { state: 'waiting' as const },
    }
    stubApiWithStatus(
      (call) => {
        const path = apiPath(call)
        if (path === 'settings') {
          return ready
            ? { body: printerSettings() }
            : { status: 503, body: { error: 'moonraker_unavailable', message: 'the daemon is not connected to Moonraker' } }
        }
        if (path === 'chips') {
          return { body: ready ? chipsSnapshot() : chipsSnapshot({ known: false, current: false, chips: [] }) }
        }
        return { status: 404, body: { error: 'not_found', message: 'no' } }
      },
      () => ({ body: ready ? healthyStatus : starting }),
    )
    const { client } = renderRoute('/')
    expect(await screen.findByText('the daemon is not connected to Moonraker')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Start test' })).toBeNull()

    ready = true
    await client.invalidateQueries({ queryKey: queryKeys.status })
    expect(await screen.findByRole('button', { name: 'Start test' })).toBeTruthy()
    expect((screen.getByLabelText('Use lis2dw ebb36') as HTMLInputElement).checked).toBe(true)
  })
})
