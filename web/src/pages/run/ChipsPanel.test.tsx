import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { chip, chipsSnapshot } from '@/test/capture'

import { ChipsPanel } from './ChipsPanel'

function panel(over: Partial<React.ComponentProps<typeof ChipsPanel>> = {}) {
  const props = {
    chips: chipsSnapshot(),
    selected: ['lis2dw ebb36'],
    onToggle: vi.fn(),
    onCheck: vi.fn(),
    checking: false,
    checkBlocked: null,
    ...over,
  }
  render(<ChipsPanel {...props} />)
  return props
}

describe('the chips panel', () => {
  it('lists each chip with its type, rate and availability', () => {
    panel({
      chips: chipsSnapshot({
        chips: [
          chip('adxl345', { availability: { state: 'available', observed_rate_hz: 3198.4, checked_at: 'x' } }),
          chip('lis2dw ebb36', { availability: { state: 'unavailable', reason: 'no valid samples arrived within 3 s' } }),
        ],
      }),
    })
    expect(screen.getByText('available · 3198 Hz')).toBeTruthy()
    expect(screen.getByText('unavailable')).toBeTruthy()
    expect(screen.getByText('no valid samples arrived within 3 s')).toBeTruthy()
  })

  it('disables the checkbox of an unavailable chip', () => {
    panel({
      chips: chipsSnapshot({
        chips: [chip('adxl345'), chip('lis2dw ebb36', { availability: { state: 'unavailable', reason: 'gone' } })],
      }),
    })
    expect((screen.getByLabelText('Use lis2dw ebb36') as HTMLInputElement).disabled).toBe(true)
    expect((screen.getByLabelText('Use adxl345') as HTMLInputElement).disabled).toBe(false)
  })

  it('shows the defaults per axis and the sections left out with their reasons', () => {
    panel({
      chips: chipsSnapshot({
        skipped: [{ section: 'beacon cartographer', reason: 'named Beacon devices are not supported yet' }],
      }),
    })
    expect(screen.getByText(/X uses lis2dw ebb36; Y uses lis2dw ebb36/)).toBeTruthy()
    expect(screen.getByText('beacon cartographer: named Beacon devices are not supported yet')).toBeTruthy()
  })

  it('asks for a check, and cannot while a check runs or the printer is not ready', () => {
    const props = panel()
    fireEvent.click(screen.getByRole('button', { name: 'Check chips' }))
    expect(props.onCheck).toHaveBeenCalledOnce()
  })

  it('disables the check with its reason when Klipper is not ready', () => {
    const props = panel({ checkBlocked: 'Klipper is not ready.' })
    const button = screen.getByRole('button', { name: 'Check chips' }) as HTMLButtonElement
    expect(button.disabled).toBe(true)
    expect(screen.getByText('Klipper is not ready.')).toBeTruthy()
    fireEvent.click(button)
    expect(props.onCheck).not.toHaveBeenCalled()
  })

  it('reports a list that is not current', () => {
    panel({ chips: chipsSnapshot({ current: false }) })
    expect(screen.getByText(/last list the daemon read/)).toBeTruthy()
  })
})
