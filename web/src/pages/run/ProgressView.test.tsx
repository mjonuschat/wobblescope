import { act, fireEvent, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'

import type { FinishedTest } from '@/lib/generated/FinishedTest'
import type { TestState } from '@/lib/generated/TestState'
import { chipsSnapshot, printerSettings } from '@/test/capture'
import type { ApiCall, Reply } from '@/test/http'
import { renderRoute } from '@/test/render'
import { apiPath, stubApiWithStatus } from '@/test/status'
import { FakeSocket, stubWebSocket } from '@/test/websocket'

const running: TestState = {
  id: 'test-1',
  phase: 'measuring',
  segment: 3,
  segments: 4,
  axis: 'X',
  position: [150.5, 154, 20],
  expected_segment_seconds: 145,
  elapsed_seconds: 12,
  chips: [
    { chip: 'adxl345', level: [120, 40, 8], warnings: [] },
    { chip: 'lis2dw ebb36', level: null, warnings: [] },
  ],
}

const done: FinishedTest = {
  id: 'test-1',
  outcome: 'complete',
  reason: null,
  session_id: 'test-1',
  motion_uncertain: false,
}

function api(stop: Reply = { body: { dispatched: true } }, start: Reply = { status: 202, body: { id: 'test-1' } }) {
  return stubApiWithStatus((call: ApiCall) => {
    const path = apiPath(call)
    if (path === 'settings') return { body: printerSettings() }
    if (path === 'chips') return { body: chipsSnapshot() }
    if (path === 'tests') return start
    if (path.endsWith('/emergency-stop')) return stop
    return { status: 404, body: { error: 'not_found', message: 'no' } }
  })
}

const stops = (calls: ApiCall[]) => calls.filter((c) => apiPath(c).endsWith('/emergency-stop'))

/** Renders the run screen and opens the live socket with `snapshot`. */
async function mount(snapshot: { test: TestState | null; last: FinishedTest | null }, stop?: Reply) {
  const calls = api(stop)
  renderRoute('/')
  await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0))
  act(() => FakeSocket.latest.open())
  act(() => FakeSocket.latest.send(snapshot))
  return calls
}

const send = (snapshot: { test: TestState | null; last: FinishedTest | null }) =>
  act(() => FakeSocket.latest.send(snapshot))

beforeEach(() => stubWebSocket())

describe('a running test', () => {
  it('shows the segment, the elapsed time and a level bar per chip that follow the latest event', async () => {
    await mount({ test: running, last: null })
    expect(await screen.findByText('Segment 3 of 4 (X at 150.5, 154, 20)')).toBeTruthy()
    expect(screen.getByText('12 s of about 2 min 25 s')).toBeTruthy()
    const bar = screen.getByRole('meter', { name: 'adxl345 signal level' })
    const before = Number(bar.getAttribute('aria-valuenow'))
    expect(before).toBeGreaterThan(0)
    expect(screen.getByText('no signal yet')).toBeTruthy()

    send({
      test: {
        ...running,
        segment: 4,
        chips: [
          { chip: 'adxl345', level: [3000, 1, 1], warnings: [] },
          { chip: 'lis2dw ebb36', level: [10, 1, 1], warnings: [] },
        ],
      },
      last: null,
    })
    expect(screen.getByText(/Segment 4 of 4/)).toBeTruthy()
    expect(Number(screen.getByRole('meter', { name: 'adxl345 signal level' }).getAttribute('aria-valuenow'))).toBeGreaterThan(before)
    expect(screen.queryByText('no signal yet')).toBeNull()
  })

  it('opened mid-test goes straight to progress', async () => {
    await mount({ test: running, last: null })
    expect(await screen.findByText('Test running')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Start test' })).toBeNull()
  })

  it('stops with one click and no dialog, and a failed finish shows the shutdown notice', async () => {
    const calls = await mount({ test: running, last: null })
    fireEvent.click(await screen.findByRole('button', { name: 'Emergency stop' }))
    expect(screen.queryByRole('dialog')).toBeNull()
    await waitFor(() => expect(stops(calls)).toHaveLength(1))
    expect(apiPath(stops(calls)[0])).toBe('tests/test-1/emergency-stop')
    expect(await screen.findByText(/FIRMWARE_RESTART/)).toBeTruthy()

    send({ test: null, last: { ...done, outcome: 'failed', reason: 'emergency stop' } })
    expect(await screen.findByText('The test failed.')).toBeTruthy()
    expect(screen.getAllByText(/FIRMWARE_RESTART/).length).toBeGreaterThan(0)
  })

  it('shows a failed stop request with the daemon message and keeps the button enabled', async () => {
    await mount(
      { test: running, last: null },
      { status: 503, body: { error: 'moonraker_unavailable', message: 'Moonraker did not take the emergency stop' } },
    )
    fireEvent.click(await screen.findByRole('button', { name: 'Emergency stop' }))
    expect(await screen.findByText('Moonraker did not take the emergency stop')).toBeTruthy()
    expect((screen.getByRole('button', { name: 'Emergency stop' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('says the failure is recorded while the sweep ends, with the stop button enabled', async () => {
    await mount({ test: { ...running, phase: 'waiting_for_sweep' }, last: null })
    expect(await screen.findByText('Failure recorded, waiting for the sweep to end')).toBeTruthy()
    expect((screen.getByRole('button', { name: 'Emergency stop' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('keeps a health warning under its chip', async () => {
    await mount({ test: running, last: null })
    send({
      test: {
        ...running,
        chips: [
          { chip: 'adxl345', level: [1, 1, 1], warnings: ['the chip\'s errors counter rose by 3'] },
          running.chips[1],
        ],
      },
      last: null,
    })
    expect(screen.getByText("the chip's errors counter rose by 3")).toBeTruthy()
    send({
      test: { ...running, segment: 4, chips: [{ chip: 'adxl345', level: [1, 1, 1], warnings: ["the chip's errors counter rose by 3"] }, running.chips[1]] },
      last: null,
    })
    expect(screen.getByText("the chip's errors counter rose by 3")).toBeTruthy()
  })

  it('keeps the last state and the stop button when the socket drops, and a new snapshot clears the mark', async () => {
    const calls = await mount({ test: running, last: null })
    await screen.findByText('Test running')
    act(() => FakeSocket.latest.drop())
    expect(await screen.findByText(/Connection lost, this may be out of date/)).toBeTruthy()
    expect(screen.getByText(/Segment 3 of 4/)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Emergency stop' }))
    await waitFor(() => expect(stops(calls)).toHaveLength(1))

    // The page reconnects on its own; the new snapshot replaces the old state.
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(1), { timeout: 3000 })
    act(() => FakeSocket.latest.open())
    send({ test: { ...running, segment: 4 }, last: null })
    expect(screen.queryByText(/Connection lost/)).toBeNull()
    expect(screen.getByText(/Segment 4 of 4/)).toBeTruthy()
  })
})

describe('a finished test', () => {
  it('links to the session when one exists', async () => {
    await mount({ test: null, last: done })
    const link = await screen.findByRole('link', { name: 'Open the session' })
    expect(link.getAttribute('href')).toBe('/sessions/test-1')
  })

  it('shows only the reason when no session exists', async () => {
    await mount({
      test: null,
      last: { ...done, outcome: 'no_session', session_id: null, reason: 'Klipper is shutdown, not ready' },
    })
    expect(await screen.findByText('Klipper is shutdown, not ready')).toBeTruthy()
    expect(screen.queryByRole('link', { name: 'Open the session' })).toBeNull()
    expect(screen.getByText('No session was saved.')).toBeTruthy()
  })

  it('keeps the stop button and a warning while motion is uncertain, until an event clears it', async () => {
    const calls = await mount({
      test: null,
      last: { ...done, outcome: 'failed', reason: 'the sweep\'s outcome is unknown', motion_uncertain: true },
    })
    expect(await screen.findByText('The printer may still be moving.')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Emergency stop' }))
    await waitFor(() => expect(stops(calls)).toHaveLength(1))

    send({ test: null, last: { ...done, outcome: 'failed', reason: 'x', motion_uncertain: false } })
    expect(screen.queryByText('The printer may still be moving.')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Emergency stop' })).toBeNull()
  })

  it('does not let New test hide the stop button while motion is uncertain', async () => {
    await mount({
      test: null,
      last: { ...done, outcome: 'failed', reason: "the sweep's outcome is unknown", motion_uncertain: true },
    })
    const newTest = await screen.findByRole('button', { name: 'New test' })
    expect((newTest as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(newTest)
    expect(screen.getByRole('button', { name: 'Emergency stop' })).toBeTruthy()
    expect(screen.getByText('The printer may still be moving.')).toBeTruthy()

    send({ test: null, last: { ...done, outcome: 'failed', reason: 'x', motion_uncertain: false } })
    expect((screen.getByRole('button', { name: 'New test' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('stays on screen until New test is clicked, and a reload after the end shows it too', async () => {
    await mount({ test: null, last: done })
    expect(await screen.findByText('The test finished.')).toBeTruthy()
    // Nothing else on the page takes it away.
    send({ test: null, last: done })
    expect(screen.getByText('The test finished.')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'New test' }))
    expect(await screen.findByRole('button', { name: 'Start test' })).toBeTruthy()
    expect(screen.queryByText('The test finished.')).toBeNull()
  })
})

describe('starting a test', () => {
  it('keeps the stop button when the live socket never delivers a snapshot', async () => {
    const calls = api()
    renderRoute('/')
    // The socket is never opened, as when a proxy refuses the upgrade.
    fireEvent.click(await screen.findByRole('button', { name: 'Start test' }))
    expect(await screen.findByText('Starting…')).toBeTruthy()
    expect(screen.getByText(/The live view is not connected/)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Emergency stop' }))
    await waitFor(() => expect(stops(calls)).toHaveLength(1))
    expect(apiPath(stops(calls)[0])).toBe('tests/test-1/emergency-stop')
  })

  it('shows Starting… after the 202 and progress once the live state names the test', async () => {
    api()
    renderRoute('/')
    await waitFor(() => expect(FakeSocket.instances.length).toBeGreaterThan(0))
    act(() => FakeSocket.latest.open())
    send({ test: null, last: null })
    fireEvent.click(await screen.findByRole('button', { name: 'Start test' }))
    expect(await screen.findByText('Starting…')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Start test' })).toBeNull()

    send({ test: running, last: null })
    expect(await screen.findByText('Test running')).toBeTruthy()
    expect(screen.queryByText('Starting…')).toBeNull()
  })
})
