import { Link } from "@tanstack/react-router";
import { useMemo, useState } from "react";

import type { AnalysisResult } from "@/analysis/protocol";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
  result: AnalysisResult;
}

/** Saves the shown axis, as the worker computed it, so the daemon keeps what the user saw. */
export function SaveResultDialog({ sessionId, meta, axis, params, overrides, result }: Props) {
  const [label, setLabel] = useState("");
  const [savedKey, setSavedKey] = useState<string | null>(null);
  const save = useSaveResult();
  const trimmed = label.trim();

  const body = useMemo<SaveRequest>(
    () => ({
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
            per_chip: result.snippets.perChip,
          },
        },
      ],
    }),
    [axis, meta.created_at, overrides, params, result],
  );
  const bodyKey = useMemo(() => JSON.stringify(body), [body]);
  // Identifies what a save would store. A second save of the same key would duplicate a result.
  const key = `${sessionId}\n${JSON.stringify(trimmed)}\n${bodyKey}`;
  const saved = savedKey === key;

  function submit() {
    save.mutate(
      { sessionId, body: { ...body, label: trimmed === "" ? null : trimmed } },
      { onSuccess: () => setSavedKey(key) },
    );
  }

  return (
    <section aria-label="Save result" className="flex flex-wrap items-end gap-3">
      <div className="space-y-1">
        <Label htmlFor="save-result-label">Label</Label>
        <Input
          id="save-result-label"
          value={label}
          placeholder="Optional"
          onChange={(event) => setLabel(event.target.value)}
        />
      </div>
      <Button onClick={submit} disabled={save.isPending || saved}>
        {save.isPending ? "Saving…" : "Save result"}
      </Button>
      {saved && save.isSuccess && (
        <p className="text-sm">
          Saved as {save.data.id}{" "}
          <Link to="/results/$rid" params={{ rid: save.data.id }} className="underline">
            Open
          </Link>
        </p>
      )}
      {save.isError && (
        <p role="alert" className="w-full text-sm text-destructive">
          {save.error.message}
        </p>
      )}
    </section>
  );
}
