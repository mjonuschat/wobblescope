import { describe, expect, it } from 'vitest'

import { formatBytes } from './format'

describe('formatBytes', () => {
  it.each([
    [0, '0 B'],
    [1536, '1.5 KiB'],
    [100 * 1024 * 1024, '100 MiB'],
    [1.5 * 1024 ** 3, '1.5 GiB'],
  ])('formats %d as %s', (bytes, text) => {
    expect(formatBytes(bytes)).toBe(text)
  })
})
