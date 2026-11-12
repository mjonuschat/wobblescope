import { FileTextIcon, PencilIcon, XIcon } from "lucide-react";

import {
  Attachment,
  AttachmentAction,
  AttachmentActions,
  AttachmentContent,
  AttachmentDescription,
  AttachmentMedia,
  AttachmentTitle,
} from "@/components/ui/attachment";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { formatBytes } from "@/lib/format";

import { ImportFileEditor } from "./ImportFileEditor";
import type { ImportEntry } from "./importModel";

interface Props {
  entry: ImportEntry;
  known: string[];
  /** The other groups this file can move to. */
  targets: { id: string; label: string }[];
  /** Why the file cannot be imported, or empty. */
  reasons: string[];
  onEdit: (patch: Partial<Pick<ImportEntry, "chip" | "axis" | "point">>) => void;
  onMove: (target: string) => void;
  onRemove: () => void;
  /** True while groups are being sent. */
  disabled: boolean;
}

export function ImportFileRow({ entry, known, targets, reasons, onEdit, onMove, onRemove, disabled }: Props) {
  const name = entry.file.name;
  return (
    <Attachment state={reasons.length > 0 ? "error" : "done"} className="w-full">
      <AttachmentMedia>
        <FileTextIcon />
      </AttachmentMedia>
      <AttachmentContent>
        <AttachmentTitle>{name}</AttachmentTitle>
        <AttachmentDescription>
          {entry.chip || "no chip"}
          {entry.chipGuessed && entry.chip !== "" && <span className="ml-1 italic">guess</span>}
          {` · ${entry.axis || "no axis"} · ${entry.point || "no point"} · ${formatBytes(entry.file.size)}`}
        </AttachmentDescription>
        {reasons.map((reason) => (
          <span key={reason} className="mt-0.5 block text-xs text-destructive">
            {reason}
          </span>
        ))}
      </AttachmentContent>
      <AttachmentActions>
        <Popover>
          <PopoverTrigger render={<AttachmentAction aria-label={`Edit ${name}`} disabled={disabled} />}>
            <PencilIcon />
          </PopoverTrigger>
          <PopoverContent className="w-auto">
            <ImportFileEditor entry={entry} known={known} onEdit={onEdit} />
          </PopoverContent>
        </Popover>
        <select
          aria-label={`Move ${name}`}
          className="h-6 rounded-md border border-input bg-transparent px-1 text-xs"
          value=""
          disabled={disabled}
          onChange={(e) => e.target.value && onMove(e.target.value)}
        >
          <option value="">Move</option>
          {targets.map((target) => (
            <option key={target.id} value={target.id}>
              {target.label}
            </option>
          ))}
          <option value="new">New group</option>
        </select>
        <AttachmentAction aria-label={`Remove ${name}`} disabled={disabled} onClick={onRemove}>
          <XIcon />
        </AttachmentAction>
      </AttachmentActions>
    </Attachment>
  );
}
