import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { twoAxisMeta } from '@/test/fixtures'

import { MetaPanel } from './MetaPanel'

describe('the session details', () => {
  it('lists the firmware of each MCU and each chip\'s axes map', () => {
    const meta = twoAxisMeta()
    meta.printer = {
      name: 'buster',
      klipper_version: 'v0.13.0-820-g0cee841e',
      mcu_firmware: ['ebb36: v0.13.0-820-g0cee841e', 'mcu: v0.13.0-658-gbd09e017'],
    }
    meta.chips[1].axes_map = 'x,z,-y'
    render(<MetaPanel meta={meta} />)
    expect(screen.getByText('MCU firmware')).toBeTruthy()
    expect(screen.getByText('ebb36: v0.13.0-820-g0cee841e')).toBeTruthy()
    expect(screen.getByText('mcu: v0.13.0-658-gbd09e017')).toBeTruthy()
    const rows = screen.getAllByRole('row')
    expect(within(rows[2]).getByText('x,z,-y')).toBeTruthy()
    // A chip whose config sets no axes map shows a dash.
    expect(within(rows[1]).getAllByText('–').length).toBeGreaterThan(0)
  })

  it('shows no firmware row when none was recorded', () => {
    render(<MetaPanel meta={twoAxisMeta()} />)
    expect(screen.queryByText('MCU firmware')).toBeNull()
  })
})
