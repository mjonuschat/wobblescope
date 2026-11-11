import { Link } from '@tanstack/react-router'
import { useState } from 'react'

import { useSessionAnalysis } from '@/analysis/useSessionAnalysis'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useSession } from '@/lib/api/queries'
import type { Axis } from '@/lib/generated/Axis'
import type { FitParams } from '@/lib/generated/FitParams'
import type { SessionMeta } from '@/lib/generated/SessionMeta'

import { AxisResults } from './AxisResults'
import { FitSettingsForm } from './FitSettingsForm'
import { MetaPanel } from './MetaPanel'
import { SaveResultDialog } from './SaveResultDialog'
import { defaultFitParams } from './fit'

interface AnalysisProps {
  id: string
  meta: SessionMeta
  axis: Axis
  params: FitParams
  overrides: string[]
  onOverridesChange: (overrides: string[]) => void
}

function Analysis({ id, meta, axis, params, overrides, onOverridesChange }: AnalysisProps) {
  const analysis = useSessionAnalysis({ sessionId: id, meta, axis, params, overrides })
  if (analysis.isPending) {
    return <p className="text-sm text-muted-foreground">Analysing axis {axis}…</p>
  }
  if (analysis.isError) {
    return (
      <p role="alert" className="rounded-lg border border-destructive p-4 text-sm text-destructive">
        The analysis failed: {analysis.error.message}
      </p>
    )
  }
  return (
    <div className="space-y-4">
      <SaveResultDialog
        sessionId={id}
        meta={meta}
        axis={axis}
        params={params}
        overrides={overrides}
        result={analysis.data}
      />
      <AxisResults
        result={analysis.data}
        meta={meta}
        overrides={overrides}
        onOverridesChange={onOverridesChange}
      />
    </div>
  )
}

export function SessionPage({ id }: { id: string }) {
  const session = useSession(id)
  const [selected, setSelected] = useState<Axis | null>(null)
  const [params, setParams] = useState<FitParams>(defaultFitParams)
  const [overrides, setOverrides] = useState<string[]>([])

  if (session.isPending) return <p className="text-sm text-muted-foreground">Loading session…</p>
  if (session.isError) {
    return (
      <div className="space-y-2">
        <p role="alert" className="text-sm text-destructive">
          {session.error.message}
        </p>
        <Link to="/sessions" className="text-sm underline">
          Back to sessions
        </Link>
      </div>
    )
  }

  const { meta } = session.data
  const axes = [...new Set(meta.segments.map((s) => s.axis))]
  const axis = selected && axes.includes(selected) ? selected : (axes[0] ?? null)

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Link to="/sessions" className="text-sm underline">
          Sessions
        </Link>
        <h1 className="text-xl font-semibold">{meta.id}</h1>
      </div>
      <MetaPanel meta={meta} />
      {axis === null ? (
        <p className="text-sm">This session has no recordings to analyse.</p>
      ) : (
        <>
          <Tabs value={axis} onValueChange={(value) => setSelected(value as Axis)}>
            <TabsList>
              {axes.map((a) => (
                <TabsTrigger key={a} value={a}>
                  Axis {a}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <FitSettingsForm onApply={setParams} />
          <Analysis
            id={id}
            meta={meta}
            axis={axis}
            params={params}
            overrides={overrides}
            onOverridesChange={setOverrides}
          />
        </>
      )}
    </div>
  )
}
