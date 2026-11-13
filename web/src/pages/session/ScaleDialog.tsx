import { ScalingIcon } from "lucide-react";
import { useState } from "react";

import { HintLabel } from "@/components/HintLabel";
import { UnitInput } from "@/components/UnitInput";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { parseNumber } from "@/lib/parse";

/** The chart's maximum from typed text: `null` for empty, `undefined` for text that is not a positive number. */
function parseScale(text: string): number | null | undefined {
  const value = parseNumber(text);
  if (value === undefined) return null;
  return Number.isFinite(value) && value > 0 ? value : undefined;
}

interface Props {
  /** Where the vibration axis ends now; `null` fits it to the data. */
  value: number | null;
  onChange: (value: number | null) => void;
}

/** An icon button that opens a dialog for the top of the vibration axis. */
export function ScaleDialog({ value, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const parsed = parseScale(text);
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) setText(value === null ? "" : String(value));
        setOpen(next);
      }}
    >
      <Tooltip>
        <TooltipTrigger
          render={
            <DialogTrigger
              render={<Button type="button" variant="outline" size="icon" aria-label="Set the chart scale" />}
            />
          }
        >
          <ScalingIcon />
        </TooltipTrigger>
        <TooltipContent side="top" className="max-w-64">
          Set where the vibration axis ends, or go back to fitting it to the data.
        </TooltipContent>
      </Tooltip>
      <DialogContent className="sm:max-w-sm">
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (parsed === undefined) return;
            onChange(parsed);
            setOpen(false);
          }}
        >
          <DialogHeader>
            <DialogTitle>Chart scale</DialogTitle>
            <DialogDescription>
              Set where the vibration axis ends. Give two charts the same maximum to compare them.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2.5">
            <HintLabel label="Maximum" hint="The top of the vibration axis. Empty fits it to the data." />
            <UnitInput
              unit=""
              aria-label="Maximum"
              placeholder="Auto"
              value={text}
              aria-invalid={parsed === undefined}
              onChange={(event) => setText(event.target.value)}
            />
            {parsed === undefined && (
              <p className="text-sm text-destructive">Enter a positive number, or use Restore Auto.</p>
            )}
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                onChange(null);
                setOpen(false);
              }}
            >
              Restore Auto
            </Button>
            <Button type="submit" disabled={parsed === undefined}>
              Apply
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
