import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { candidate } from '@/test/fixtures'
import type { Recommendation } from '@/lib/generated/Recommendation'
import { recommendation } from '@/test/fixtures'

import { ShaperTable } from './ShaperTable'

describe('ShaperTable', () => {
  it('lists every shaper, highlights the recommended one and shows vibrations as a percent', () => {
    const best = candidate('mzv', 48.2, { vibrs: 0.032 })
    const rec: Recommendation = {
      ...recommendation(best),
      all: [
        candidate('zv', 55, { vibrs: 0.1 }),
        best,
        candidate('ei', 52),
        candidate('2hump_ei', 60),
        candidate('3hump_ei', 75),
      ],
    }
    render(<ShaperTable recommendation={rec} />)
    const rows = screen.getAllByRole('row').slice(1)
    expect(rows).toHaveLength(5)
    const highlighted = rows.filter((r) => r.getAttribute('data-best') === 'true')
    expect(highlighted).toHaveLength(1)
    expect(highlighted[0].textContent).toContain('mzv')
    expect(highlighted[0].textContent).toContain('48.2 Hz')
    expect(highlighted[0].textContent).toContain('3.2%')
  })
})
