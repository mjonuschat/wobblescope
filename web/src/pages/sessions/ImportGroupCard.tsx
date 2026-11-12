import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";

import { GroupDatePicker } from "./GroupDatePicker";
import { ImportFileRow } from "./ImportFileRow";
import {
  confirmGroup,
  editEntry,
  type ImportGroupState,
  type ImportState,
  moveEntry,
  needsConfirmation,
  problems,
  removeEntry,
  setCreatedAt,
} from "./importModel";

interface Props {
  group: ImportGroupState;
  number: number;
  /** Labels of every group, for the Move choices. */
  labels: { id: string; label: string }[];
  known: string[];
  /** What the daemon said about this group, and which file it named. */
  failure: { text: string; entryId: string | null } | null;
  update: (change: (state: ImportState) => ImportState) => void;
}

export function ImportGroupCard({ group, number, labels, known, failure, update }: Props) {
  const found = problems(group);
  const groupWide = found.filter((p) => p.entryId === null);
  const targets = labels.filter((l) => l.id !== group.id);
  return (
    <section
      aria-label={`Group ${number}`}
      data-group-id={group.id}
      className="space-y-3 rounded-xl border bg-card p-3"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-medium">Group {number}</h3>
        <GroupDatePicker value={group.createdAt} onChange={(date) => update((s) => setCreatedAt(s, group.id, date))} />
      </div>
      {needsConfirmation(group) && (
        <div className="space-y-1 rounded-lg bg-muted/50 p-2 text-sm">
          <p>These files do not share a run name</p>
          <div className="flex items-center gap-2">
            <Checkbox
              id={`confirm-${group.id}`}
              checked={group.confirmed}
              onCheckedChange={(checked) => update((s) => confirmGroup(s, group.id, checked === true))}
            />
            <Label htmlFor={`confirm-${group.id}`}>I recorded these together</Label>
          </div>
        </div>
      )}
      {[...groupWide.map((p) => p.text), ...(failure ? [failure.text] : [])].map((text) => (
        <p key={text} role="alert" className="text-sm text-destructive">
          {text}
        </p>
      ))}
      <div className="space-y-2">
        {group.entries.map((entry) => (
          <ImportFileRow
            key={entry.id}
            entry={entry}
            known={known}
            targets={targets}
            reasons={[
              ...found.filter((p) => p.entryId === entry.id).map((p) => p.text),
              ...(failure?.entryId === entry.id ? [failure.text] : []),
            ]}
            onEdit={(patch) => update((s) => editEntry(s, entry.id, patch))}
            onMove={(target) => update((s) => moveEntry(s, entry.id, target))}
            onRemove={() => update((s) => removeEntry(s, entry.id))}
          />
        ))}
      </div>
    </section>
  );
}
