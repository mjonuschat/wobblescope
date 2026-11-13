import { Link } from "@tanstack/react-router";
import { DownloadIcon, PinIcon, PinOffIcon, Trash2Icon, UploadIcon } from "lucide-react";
import { type ReactNode, useState } from "react";

import { Panel } from "@/components/Panel";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ApiClientError } from "@/lib/api/client";
import type { SessionFilters as Filters } from "@/lib/api/endpoints";
import { sessionDownloadUrl } from "@/lib/api/endpoints";
import { usePinSession, useSessions } from "@/lib/api/queries";
import { formatBytes, formatTime } from "@/lib/format";
import type { OutcomeKind } from "@/lib/generated/OutcomeKind";

import { DeleteSessionDialog } from "./DeleteSessionDialog";
import { ImportDialog } from "./ImportDialog";
import { ColumnFilter, DateFilter, type FilterColumn } from "./SessionFilters";

const PIN_HINTS = {
  pin: "Never delete this session to make room for new ones.",
  unpin: "Allow this session to be deleted again.",
};

const OUTCOME_CLASS: Record<OutcomeKind, string> = {
  Complete: "border-emerald-500/50 text-emerald-400",
  Incomplete: "border-amber-500/50 text-amber-400",
  Failed: "border-red-500/50 text-red-400",
};

function IconAction({
  label,
  hint,
  disabled,
  href,
  highlighted,
  onClick,
  children,
}: {
  label: string;
  hint?: string;
  disabled?: boolean;
  href?: string;
  highlighted?: boolean;
  onClick?: () => void;
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
            className={highlighted ? "border-primary dark:border-primary" : undefined}
            disabled={disabled}
            onClick={onClick}
            // biome-ignore lint/a11y/useAnchorContent: the Button puts its children inside this anchor
            {...(href ? { render: <a href={href} download />, nativeButton: false } : {})}
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
  } else if (list.data.sessions.length === 0 && !filtered) {
    body = (
      <div className="space-y-2 text-sm">
        <p>No sessions yet.</p>
        <Button variant="outline" onClick={() => setImporting(true)}>
          Import a CSV
        </Button>
      </div>
    );
  } else {
    const head = "h-9 text-xs font-medium uppercase tracking-wide text-muted-foreground";
    const filter = (column: FilterColumn) => (
      <ColumnFilter column={column} filters={filters} onChange={setFilters} all={all.data?.sessions ?? []} />
    );
    body = (
      <Table>
        <TableHeader className="bg-muted/40">
          <TableRow className="hover:bg-transparent">
            <TableHead className={head}>
              <span className="inline-flex items-center gap-1">
                Session <DateFilter filters={filters} onChange={setFilters} />
              </span>
            </TableHead>
            <TableHead className={head}>Source</TableHead>
            <TableHead className={head}>Mode</TableHead>
            <TableHead className={head}>
              <span className="inline-flex items-center gap-1">Chips {filter("chip")}</span>
            </TableHead>
            <TableHead className={head}>
              <span className="inline-flex items-center gap-1">Printer {filter("printer")}</span>
            </TableHead>
            <TableHead className={head}>Outcome</TableHead>
            <TableHead className={head}>Size</TableHead>
            <TableHead className={`${head} text-right`}>
              <span className="inline-flex items-center gap-1">Actions {filter("pinned")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {list.data.sessions.length === 0 && (
            <TableRow className="hover:bg-transparent">
              <TableCell colSpan={8} className="py-6 text-center text-sm text-muted-foreground">
                No sessions match these filters.
              </TableCell>
            </TableRow>
          )}
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
                <TableCell>
                  {s.mode ?? "–"}
                  {s.kind === "belts" && (
                    <Badge variant="outline" className="ml-2">
                      Belts
                    </Badge>
                  )}
                </TableCell>
                <TableCell>{s.chips.join(", ")}</TableCell>
                <TableCell>{s.printer ?? "–"}</TableCell>
                <TableCell>
                  <Badge variant="outline" className={OUTCOME_CLASS[s.outcome]}>
                    {s.outcome}
                  </Badge>
                </TableCell>
                <TableCell>{formatBytes(s.size_bytes)}</TableCell>
                <TableCell className="space-x-2 text-right">
                  <IconAction
                    label={s.pinned ? "Unpin" : "Pin"}
                    hint={s.pinned ? PIN_HINTS.unpin : PIN_HINTS.pin}
                    highlighted={s.pinned}
                    disabled={pinning}
                    onClick={() => pin.mutate({ id: s.id, pinned: !s.pinned })}
                  >
                    {s.pinned ? <PinOffIcon /> : <PinIcon />}
                  </IconAction>
                  <IconAction label="Download" hint="Download the session as a zip" href={sessionDownloadUrl(s.id)}>
                    <DownloadIcon />
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
    <div className="space-y-4">
      <div className="flex justify-end">
        <Button onClick={() => setImporting(true)}>
          <UploadIcon data-icon="inline-start" />
          Import CSV
        </Button>
      </div>
      <Panel title="Sessions">{body}</Panel>
      <ImportDialog open={importing} onOpenChange={setImporting} />
      <DeleteSessionDialog sessionId={deleting} onClose={() => setDeleting(null)} />
    </div>
  );
}
