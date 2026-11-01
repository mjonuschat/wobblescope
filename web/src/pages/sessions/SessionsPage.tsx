import { Link } from '@tanstack/react-router'
import { useState } from 'react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { ApiClientError } from '@/lib/api/client'
import type { SessionFilters as Filters } from '@/lib/api/endpoints'
import { usePinSession, useSessions, useStorage } from '@/lib/api/queries'
import { formatBytes, formatTime } from '@/lib/format'

import { DeleteSessionDialog } from './DeleteSessionDialog'
import { ImportDialog } from './ImportDialog'
import { SessionFilters } from './SessionFilters'
import { StorageUsage } from './StorageUsage'

function isDisabled(error: unknown): boolean {
  return error instanceof ApiClientError && error.code === 'storage_disabled'
}

export function SessionsPage() {
  const [filters, setFilters] = useState<Filters>({})
  const [importing, setImporting] = useState(false)
  const [deleting, setDeleting] = useState<string | null>(null)
  const list = useSessions(filters)
  const all = useSessions({})
  const storage = useStorage()
  const pin = usePinSession()
  const filtered = Object.values(filters).some((v) => v !== undefined)

  let body
  if (isDisabled(list.error)) {
    body = (
      <p className="rounded-lg border p-4 text-sm">
        The daemon has no storage directory configured, so sessions cannot be stored or listed.
        Set <code>storage_dir</code> in its config file and restart it.
      </p>
    )
  } else if (list.isPending) {
    body = <p className="text-sm text-muted-foreground">Loading sessions…</p>
  } else if (list.isError) {
    body = (
      <div role="alert" className="space-y-2 text-sm">
        <p className="text-destructive">{list.error.message}</p>
        <Button variant="outline" onClick={() => void list.refetch()}>
          Retry
        </Button>
      </div>
    )
  } else if (list.data.sessions.length === 0) {
    body = filtered ? (
      <p className="text-sm text-muted-foreground">No sessions match these filters.</p>
    ) : (
      <div className="space-y-2 text-sm">
        <p>No sessions yet.</p>
        <Button variant="outline" onClick={() => setImporting(true)}>
          Import a CSV
        </Button>
      </div>
    )
  } else {
    body = (
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Session</TableHead>
            <TableHead>Source</TableHead>
            <TableHead>Mode</TableHead>
            <TableHead>Chips</TableHead>
            <TableHead>Outcome</TableHead>
            <TableHead>Size</TableHead>
            <TableHead />
          </TableRow>
        </TableHeader>
        <TableBody>
          {list.data.sessions.map((s) => {
            const pinning = pin.isPending && pin.variables?.id === s.id
            return (
              <TableRow key={s.id}>
                <TableCell>
                  <Link to="/sessions/$id" params={{ id: s.id }} className="font-medium underline">
                    {s.id}
                  </Link>
                  <div className="text-xs text-muted-foreground">{formatTime(s.created_at)}</div>
                  {pin.isError && pin.variables?.id === s.id && (
                    <div role="alert" className="text-xs text-destructive">
                      {pin.error.message}
                    </div>
                  )}
                </TableCell>
                <TableCell>{s.source === 'CsvImport' ? 'CSV import' : 'Capture'}</TableCell>
                <TableCell>{s.mode ?? '–'}</TableCell>
                <TableCell>{s.chips.join(', ')}</TableCell>
                <TableCell>{s.outcome}</TableCell>
                <TableCell>
                  {formatBytes(s.size_bytes)} {s.pinned && <Badge>Pinned</Badge>}
                </TableCell>
                <TableCell className="space-x-2 text-right">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={pinning}
                    onClick={() => pin.mutate({ id: s.id, pinned: !s.pinned })}
                  >
                    {s.pinned ? 'Unpin' : 'Pin'}
                  </Button>
                  <Button variant="outline" size="sm" onClick={() => setDeleting(s.id)}>
                    Delete
                  </Button>
                </TableCell>
              </TableRow>
            )
          })}
        </TableBody>
      </Table>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Sessions</h1>
        <Button onClick={() => setImporting(true)}>Import CSV</Button>
      </div>
      {storage.data && <StorageUsage usage={storage.data} />}
      {!isDisabled(list.error) && (
        <SessionFilters filters={filters} onChange={setFilters} all={all.data?.sessions ?? []} />
      )}
      {body}
      <ImportDialog open={importing} onOpenChange={setImporting} />
      <DeleteSessionDialog sessionId={deleting} onClose={() => setDeleting(null)} />
    </div>
  )
}
