import { describe, expect, it } from 'vitest'

import { parseNumber } from './parse'

describe('parseNumber', () => {
  it('is undefined for an empty field, a number for a number and NaN for anything else', () => {
    expect(parseNumber('  ')).toBeUndefined()
    expect(parseNumber(' 2.5 ')).toBe(2.5)
    expect(parseNumber('abc')).toBeNaN()
    expect(parseNumber('Infinity')).toBeNaN()
  })
})
