import { CheckIcon } from "lucide-react";

import { cn } from "@/lib/utils";

export interface Step {
  id: string;
  label: string;
}

interface Props {
  steps: Step[];
  current: string;
  selectable?: string[];
  onSelect?: (id: string) => void;
}

export function Steps({ steps, current, selectable = [], onSelect }: Props) {
  const at = steps.findIndex((s) => s.id === current);
  return (
    <nav aria-label="Steps">
      <ol
        className="grid items-center gap-3"
        style={{ gridTemplateColumns: `repeat(${steps.length}, minmax(0, 1fr))` }}
      >
        {steps.map((step, index) => {
          const state = at < 0 ? "upcoming" : index < at ? "done" : index === at ? "current" : "upcoming";
          const clickable = selectable.includes(step.id) && onSelect !== undefined;
          const content = (
            <>
              <span
                className={cn(
                  "box-border flex size-7 shrink-0 items-center justify-center rounded-full border-2 text-xs font-medium",
                  state === "current" && "border-primary bg-primary text-primary-foreground",
                  state === "done" && "border-primary bg-card text-primary",
                  state === "upcoming" && "border-border bg-card text-muted-foreground",
                )}
              >
                {state === "done" ? <CheckIcon aria-hidden="true" className="size-4" /> : null}
                {state === "done" ? <span className="sr-only">{index + 1}</span> : index + 1}
              </span>
              <span
                className={cn(
                  state === "current" ? "font-medium" : "text-muted-foreground",
                  clickable && "group-hover:text-foreground",
                )}
              >
                {step.label}
              </span>
            </>
          );
          const label = "flex shrink-0 items-center gap-2 rounded-md text-sm";
          return (
            <li key={step.id} className="flex items-center gap-3">
              {clickable ? (
                <button
                  type="button"
                  data-state={state}
                  aria-current={state === "current" ? "step" : undefined}
                  onClick={() => onSelect(step.id)}
                  className={cn(label, "group outline-none focus-visible:ring-2 focus-visible:ring-ring")}
                >
                  {content}
                </button>
              ) : (
                <span data-state={state} aria-current={state === "current" ? "step" : undefined} className={label}>
                  {content}
                </span>
              )}
              {index < steps.length - 1 && (
                <span aria-hidden="true" className={cn("h-px flex-1", state === "done" ? "bg-primary" : "bg-border")} />
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
