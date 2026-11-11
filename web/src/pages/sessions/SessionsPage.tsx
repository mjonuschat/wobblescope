import { Link } from "@tanstack/react-router";
import { PinIcon, PinOffIcon, Trash2Icon, UploadIcon } from "lucide-react";
import { type ReactNode, useState } from "react";

import { Panel } from "@/components/Panel";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ApiClientError } from "@/lib/api/client";
import type { SessionFilters as Filters } from "@/lib/api/endpoints";
import { usePinSession, useSessions, useStorage } from "@/lib/api/queries";
import { formatBytes, formatTime } from "@/lib/format";
import type { OutcomeKind } from "@/lib/generated/OutcomeKind";

import { DeleteSessionDialog } from "./DeleteSessionDialog";
import { ImportDialog } from "./ImportDialog";
import { SessionFilters } from "./SessionFilters";
import { StorageUsage } from "./StorageUsage";

const PIN_HINTS = {
  pin: "Never delete this session to make room for new ones.",
  unpin: "Allow this session to be deleted again.",
};

const OUTCOME_CLASS: Record<OutcomeKind, string> = {
  Complete: "border-emerald-500/50 text-emerald-400",
  Incomplete: "border-amber-500/50 text-amber-400",
  Failed: "border-red-500/50 text-red-400",
};

/** An icon button that names itself in a tooltip. */
function IconAction({
  label,
  hint,
  disabled,
  onClick,
  children,
}: {
  label: string;
  /** What the action does, shown in the tooltip. The label when omitted. */
  hint?: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={label}
            disabled={disabled}
            onClick={onClick}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-64">
        {hint ?? label}
      </TooltipContent>
    </Tooltip>
  );
}

function isWaiting(error: unknown): boolean {
  return error instanceof ApiClientError && error.code === "storage_unavailable";
}

export function SessionsPage() {
  const [filters, setFilters] = useState<Filters>({});
  const [importing, setImporting] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const list = useSessions(filters);
  const all = useSessions({});
  const storage = useStorage();
  const pin = usePinSession();
  const filtered = Object.values(filters).some((v) => v !== undefined);

  let body: ReactNode;
  if (isWaiting(list.error)) {
    body = (
      <p className="text-sm">
        The daemon is waiting for Moonraker to report where to store sessions. They appear here once it does.
      </p>
    );
  } else if (list.isPending) {
    body = <p className="text-sm text-muted-foreground">Loading sessions…</p>;
  } else if (list.isError) {
    body = (
      <div role="alert" className="space-y-2 text-sm">
        <p className="text-destructive">{list.error.message}</p>
        <Button variant="outline" onClick={() => void list.refetch()}>
          Retry
        </Button>
      </div>
    );
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
    );
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
            const pinning = pin.isPending && pin.variables?.id === s.id;
            return (
              <TableRow key={s.id}>
                <TableCell>
                  <Link to="/sessions/$id" params={{ id: s.id }} className="font-mono font-medium underline">
                    {s.id}
                  </Link>
                  <div className="text-xs text-muted-foreground">{formatTime(s.created_at)}</div>
                  {pin.isError && pin.variables?.id === s.id && (
                    <div role="alert" className="text-xs text-destructive">
                      {pin.error.message}
                    </div>
                  )}
                </TableCell>
                <TableCell>{s.source === "CsvImport" ? "CSV" : "Wobblescope"}</TableCell>
                <TableCell>{s.mode ?? "–"}</TableCell>
                <TableCell>{s.chips.join(", ")}</TableCell>
                <TableCell>
                  <Badge variant="outline" className={OUTCOME_CLASS[s.outcome]}>
                    {s.outcome}
                  </Badge>
                </TableCell>
                <TableCell>
                  {formatBytes(s.size_bytes)} {s.pinned && <Badge>Pinned</Badge>}
                </TableCell>
                <TableCell className="space-x-2 text-right">
                  <IconAction
                    label={s.pinned ? "Unpin" : "Pin"}
                    hint={s.pinned ? PIN_HINTS.unpin : PIN_HINTS.pin}
                    disabled={pinning}
                    onClick={() => pin.mutate({ id: s.id, pinned: !s.pinned })}
                  >
                    {s.pinned ? <PinOffIcon /> : <PinIcon />}
                  </IconAction>
                  <IconAction label="Delete" onClick={() => setDeleting(s.id)}>
                    <Trash2Icon />
                  </IconAction>
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    );
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Sessions</h1>
        <Button onClick={() => setImporting(true)}>
          <UploadIcon data-icon="inline-start" />
          Import CSV
        </Button>
      </div>
      <Panel title="Sessions">
        {!isWaiting(list.error) && (
          <SessionFilters filters={filters} onChange={setFilters} all={all.data?.sessions ?? []} />
        )}
        {body}
      </Panel>
      {storage.data && <StorageUsage usage={storage.data} />}
      <ImportDialog open={importing} onOpenChange={setImporting} />
      <DeleteSessionDialog sessionId={deleting} onClose={() => setDeleting(null)} />
    </div>
  );
}
