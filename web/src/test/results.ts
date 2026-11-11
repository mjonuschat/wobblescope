import type { ResultDetail } from '@/lib/generated/ResultDetail'
import type { ResultSummary } from '@/lib/generated/ResultSummary'

import { analysis, defaultParams, twoAxisMeta } from './fixtures'

export const SAVED_SNIPPET = '[input_shaper]\nshaper_type_x: mzv\nshaper_freq_x: 48.2\n'

export function resultSummary(id: string, over: Partial<ResultSummary> = {}): ResultSummary {
  return {
    id,
    label: null,
    created_at: '2026-11-10T12:00:00Z',
    session_id: 's1',
    core_version: '0.1.0',
    size_bytes: 2048,
    unusable: null,
    ...over,
  }
}

/** A saved axis X result of session `s1`, with chip `a` recommending and chip `b` not. */
export function resultDetail(over: { coreVersion?: string } = {}): ResultDetail {
  const saved = analysis()
  return {
    record: {
      version: 1,
      id: 'r1',
      created_at: '2026-11-10T12:00:00Z',
      label: 'after belts',
      session_id: 's1',
      core_version: over.coreVersion ?? '0.1.0',
      daemon_version: '0.1.0',
      fit_params: defaultParams,
      overrides: [],
      axes: [
        {
          axis: 'X',
          analysis: saved,
          snippets: { worst_case: SAVED_SNIPPET, per_chip: [SAVED_SNIPPET, null] },
        },
      ],
      manifest: {},
    },
    meta: twoAxisMeta('s1'),
  }
}
