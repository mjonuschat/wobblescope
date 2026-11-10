import type { Chip } from '@/lib/generated/Chip'
import type { ChipsSnapshot } from '@/lib/generated/ChipsSnapshot'
import type { PrinterSettings } from '@/lib/generated/PrinterSettings'
import type { Setting } from '@/lib/generated/Setting'

const setting = (value: number | null, needed = false): Setting => ({
  value,
  overridable: true,
  needed,
})

/** The test printer's settings: sweep mode, a 5 to 135 Hz range at 1 Hz/s, travel 301 x 308. */
export function printerSettings(overrides: Partial<PrinterSettings['settings']> = {}): PrinterSettings {
  return {
    variant: 'mainline',
    default_mode: 'Sweep',
    settings: {
      freq_start: setting(5, true),
      freq_end: setting(135, true),
      hz_per_sec: setting(1, true),
      accel_per_hz: setting(60, true),
      sweeping_accel: setting(400),
      sweeping_period: setting(1.2),
      move_speed: setting(50),
      ...overrides,
    },
    axis_limits: {
      x: { min: 0, max: 301 },
      y: { min: 0, max: 308 },
      z: { min: -2, max: 300 },
    },
    default_point: [150.5, 154, 20],
    segment_overhead_seconds: 15,
  }
}

export function chip(section: string, over: Partial<Chip> = {}): Chip {
  return {
    section,
    chip_type: section.startsWith('lis2dw') ? 'lis2dw' : 'adxl345',
    sensor: section.split(' ').pop() ?? null,
    dump_endpoint: 'x/dump',
    nominal_rate_hz: 3200,
    mcu: 'mcu',
    axes_map: null,
    availability: { state: 'unchecked' },
    ...over,
  }
}

/** Two chips, both unchecked, with `lis2dw ebb36` the default on both axes. */
export function chipsSnapshot(over: Partial<ChipsSnapshot> = {}): ChipsSnapshot {
  return {
    known: true,
    current: true,
    chips: [chip('adxl345'), chip('lis2dw ebb36')],
    skipped: [],
    defaults: { x: ['lis2dw ebb36'], y: ['lis2dw ebb36'] },
    ...over,
  }
}
