import { useNavigate } from "@tanstack/react-router";
import { SaveIcon } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { toast } from "sonner";

import type { AnalysisResult } from "@/analysis/protocol";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useSaveResult } from "@/lib/api/queries";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SaveRequest } from "@/lib/generated/SaveRequest";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

interface Props {
  sessionId: string;
  meta: SessionMeta;
  axis: Axis;
  params: FitParams;
  overrides: string[];
  /** The shown analysis; null while it is not there, which disables the button. */
  result: AnalysisResult | null;
}

/**
 * A button that asks for a name in a dialog and then saves the shown axis, as the worker computed
 * it, so the daemon keeps what the user saw.
 */
export function SaveResultButton({ sessionId, meta, axis, params, overrides, result }: Props) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [savedKey, setSavedKey] = useState<string | null>(null);
  const save = useSaveResult();

  const body = useMemo<SaveRequest | null>(
    () =>
      result && {
        label: null,
        // The version that computed the axes, which can differ from this app's own version.
        core_version: result.analysis.core_version,
        session_created_at: meta.created_at,
        fit_params: params,
        overrides,
        axes: [
          {
            axis,
            analysis: result.analysis,
            snippets: {
              worst_case: result.snippets.worstCase,
              worst_case_performance: result.snippets.worstCasePerformance,
              per_chip: result.snippets.perChip,
            },
          },
        ],
      },
    [axis, meta.created_at, overrides, params, result],
  );
  // Identifies what a save would store. A second save of the same key would duplicate a result.
  const key = body ? `${sessionId}\n${JSON.stringify(body)}` : null;
  const saved = key !== null && savedKey === key;

  function close(next: boolean) {
    setOpen(next);
    if (!next) save.reset();
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!body || key === null || save.isPending) return;
    const trimmed = name.trim();
    save.mutate(
      { sessionId, body: { ...body, label: trimmed === "" ? null : trimmed } },
      {
        onSuccess: (summary) => {
          setSavedKey(key);
          setName("");
          close(false);
          toast.success(`Saved as ${trimmed === "" ? summary.id : trimmed}`, {
            action: {
              label: "Open",
              onClick: () => void navigate({ to: "/results/$rid", params: { rid: summary.id } }),
            },
          });
        },
      },
    );
  }

  const unavailable = !body ? "The analysis is not ready." : saved ? "This analysis is already saved." : null;
  return (
    <>
      <Tooltip>
        <TooltipTrigger render={<span />}>
          <Button type="button" variant="outline" disabled={unavailable !== null} onClick={() => setOpen(true)}>
            <SaveIcon data-icon="inline-start" />
            Save
          </Button>
        </TooltipTrigger>
        {unavailable && <TooltipContent side="top">{unavailable}</TooltipContent>}
      </Tooltip>
      <Dialog open={open} onOpenChange={close}>
        <DialogContent>
          <form onSubmit={submit} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>Save result</DialogTitle>
              <DialogDescription>
                Keeps this analysis and its recordings on the printer's disk, outside the storage limit.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-1">
              <Label htmlFor="save-result-name">Name</Label>
              <Input
                id="save-result-name"
                value={name}
                placeholder="Optional, such as after belt swap"
                autoFocus
                onChange={(event) => setName(event.target.value)}
              />
            </div>
            {save.isError && (
              <p role="alert" className="text-sm text-destructive">
                {save.error.message}
              </p>
            )}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => close(false)} disabled={save.isPending}>
                Cancel
              </Button>
              <Button type="submit" disabled={save.isPending}>
                {save.isPending ? "Saving…" : "Save"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  );
}
