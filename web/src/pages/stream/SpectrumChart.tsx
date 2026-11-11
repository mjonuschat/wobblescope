import { useMemo } from 'react'

import { EChart } from '@/components/charts/EChart'
import { liveSpectrumOption } from '@/components/charts/options'
import { Button } from '@/components/ui/button'

type Chips = Parameters<typeof liveSpectrumOption>[0]['chips']

interface Props {
  chips: Chips
  maxHz: number
  onResetPeakHold: () => void
}

/** The live spectra of the chips, with a button that clears the peak-hold lines. */
export function SpectrumChart({ chips, maxHz, onResetPeakHold }: Props) {
  const option = useMemo(() => liveSpectrumOption({ chips, maxHz }), [chips, maxHz])
  return (
    <div className="space-y-2">
      <div className="flex justify-end">
        <Button variant="outline" size="sm" onClick={onResetPeakHold}>
          Reset peak hold
        </Button>
      </div>
      <EChart option={option} label="Live spectrum of the streamed chips" height={420} />
    </div>
  )
}
