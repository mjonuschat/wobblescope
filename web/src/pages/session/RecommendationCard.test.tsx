import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { candidate, recommendation } from '@/test/fixtures'

import { RecommendationCard } from './RecommendationCard'

describe('RecommendationCard', () => {
  it('shows the max acceleration to the nearest 100 like Klipper', () => {
    const rec = recommendation(candidate('mzv', 48.2, { max_accel: 4537 }))
    render(<RecommendationCard title="t" recommendation={rec} snippet={null} />)
    expect(screen.getByText('4500 mm/s²')).toBeTruthy()
  })

  it('lists each chip behind a worst case and marks the one that sets it', () => {
    render(
      <RecommendationCard
        title="t"
        recommendation={recommendation()}
        snippet={null}
        datasetVibrations={[
          { chip: 'a', point: 0, vibrs: 0.01 },
          { chip: 'b', point: 0, vibrs: 0.032 },
        ]}
      />,
    )
    const items = screen.getAllByRole('listitem')
    expect(items.map((i) => i.textContent)).toEqual([
      'a: 1.0%',
      'b: 3.2% (sets the worst case)',
    ])
    expect(items.map((i) => i.getAttribute('data-limiting'))).toEqual(['false', 'true'])
  })

  it('shows no per-chip list for a single dataset', () => {
    render(
      <RecommendationCard
        title="t"
        recommendation={recommendation()}
        snippet={null}
        datasetVibrations={[{ chip: 'a', point: 0, vibrs: 0.032 }]}
      />,
    )
    expect(screen.queryByRole('listitem')).toBeNull()
  })
})
