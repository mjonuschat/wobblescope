import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SpectrumChart } from './SpectrumChart'

vi.mock('@/components/charts/EChart', () => ({ EChart: () => <div /> }))

describe('SpectrumChart', () => {
  it('calls onResetPeakHold from its button', () => {
    const reset = vi.fn()
    render(<SpectrumChart chips={[]} maxHz={200} onResetPeakHold={reset} />)
    fireEvent.click(screen.getByRole('button', { name: 'Reset peak hold' }))
    expect(reset).toHaveBeenCalledOnce()
  })
})
