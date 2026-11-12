import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ApiClientError } from "@/lib/api/client";
import { useChips, useImportGroups } from "@/lib/api/queries";
import { formatBytes } from "@/lib/format";
import type { Summary } from "@/lib/generated/Summary";

import { ImportDropzone } from "./ImportDropzone";
import { ImportGroupCard } from "./ImportGroupCard";
import { addFiles, canImport, type ImportGroupState, type ImportState } from "./importModel";

function describe(error: Error): string {
  if (!(error instanceof ApiClientError) || !error.body) return error.message;
  const body = error.body;
  switch (body.error) {
    case "busy":
      return `Another import is running. Try again in a moment. ${body.message}`;
    case "cap_exceeded":
      return `${body.message} The import needs ${formatBytes(body.need)} and the cap is ${formatBytes(body.cap)}.`;
    case "insufficient_disk":
      return `${body.message} The import needs ${formatBytes(body.need)} and ${formatBytes(body.available)} is free.`;
    default:
      return body.message;
  }
}

/** Identifies a group's content, so an error from the daemon is dropped once the group changes. */
function signature(group: ImportGroupState): string {
  return JSON.stringify([
    group.createdAt.getTime(),
    group.confirmed,
    group.entries.map((e) => [e.id, e.chip, e.axis, e.point]),
  ]);
}

interface Failure {
  signature: string;
  text: string;
  entryId: string | null;
}

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** The dialog's content is only mounted while it is open, so closing it clears the files. */
export function ImportDialog({ open, onOpenChange }: Props) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <ImportForm onDone={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}

function ImportForm({ onDone }: { onDone: () => void }) {
  const navigate = useNavigate();
  const chips = useChips();
  const { run, isRunning } = useImportGroups();
  const [groups, setGroups] = useState<ImportState>([]);
  const [refused, setRefused] = useState<{ name: string; reason: string }[]>([]);
  const [failures, setFailures] = useState<Record<string, Failure>>({});
  const known = chips.data?.known ? chips.data.chips.map((c) => c.section) : [];
  const defaults = chips.data?.defaults ?? { x: [], y: [] };

  function add(files: File[]) {
    const added = addFiles(groups, files, { known, defaults, now: new Date() });
    setGroups(added.state);
    setRefused(added.refused);
  }

  async function submit() {
    const stored: Summary[] = [];
    const { failed } = await run(groups, (groupId, summary) => {
      stored.push(summary);
      setGroups((current) => current.filter((g) => g.id !== groupId));
    });
    const next: Record<string, Failure> = {};
    for (const { groupId, error, entryIndex } of failed) {
      const group = groups.find((g) => g.id === groupId);
      if (!group) continue;
      next[groupId] = {
        signature: signature(group),
        text: describe(error),
        entryId: entryIndex === null ? null : group.entries[entryIndex].id,
      };
    }
    setFailures(next);
    if (failed.length === 0) {
      onDone();
      if (stored.length === 1) void navigate({ to: "/sessions/$id", params: { id: stored[0].id } });
      else if (stored.length > 1) {
        void navigate({ to: "/sessions" });
        toast.success(`Imported ${stored.length} sessions`);
      }
    } else if (stored.length > 0) {
      const only = stored.length === 1 ? stored[0] : null;
      toast.success(`Imported ${stored.length} ${stored.length === 1 ? "session" : "sessions"}`, {
        action: only
          ? { label: "Open", onClick: () => void navigate({ to: "/sessions/$id", params: { id: only.id } }) }
          : undefined,
      });
    }
  }

  const labels = groups.map((g, i) => ({ id: g.id, label: `Group ${i + 1}` }));
  return (
    <div className="space-y-4">
      <DialogHeader>
        <DialogTitle>Import Klipper CSV files</DialogTitle>
        <DialogDescription>
          Raw accelerometer files from <code>TEST_RESONANCES OUTPUT=raw_data</code>. Each group becomes one session.
        </DialogDescription>
      </DialogHeader>
      <ImportDropzone onFiles={add} />
      {refused.map((r) => (
        <p key={r.name} role="alert" className="text-sm text-destructive">
          {r.name}: {r.reason}
        </p>
      ))}
      {groups.map((group, i) => {
        const failure = failures[group.id];
        return (
          <ImportGroupCard
            key={group.id}
            group={group}
            number={i + 1}
            labels={labels}
            known={known}
            failure={failure && failure.signature === signature(group) ? failure : null}
            update={(change) => setGroups(change)}
          />
        );
      })}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button
          type="button"
          disabled={isRunning || groups.length === 0 || !groups.every(canImport)}
          onClick={() => void submit()}
        >
          {isRunning ? "Importing…" : "Import"}
        </Button>
      </DialogFooter>
    </div>
  );
}
