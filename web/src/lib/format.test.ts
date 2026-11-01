import { describe, expect, it } from 'vitest'

import { formatBytes, formatHz } from './format'

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

describe('formatHz', () => {
  it('rounds to 0.1 Hz so float noise never shows', () => {
    expect(formatHz(52.60000000000019)).toBe('52.6')
    expect(formatHz(48.2)).toBe('48.2')
  })
})
