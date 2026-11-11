import { describe, expect, it } from 'vitest'

import { chipColor } from './colors'

describe('chipColor', () => {
  it('is stable and distinct for four chips', () => {
    const colors = [0, 1, 2, 3].map(chipColor)
    expect(new Set(colors).size).toBe(4)
    expect(chipColor(2)).toBe(colors[2])
  })
})
