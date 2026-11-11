import { Card } from "@/components/ui/card";

interface Props {
  lines: string[];
}

const PREFIXES = ["// ", "echo: "];

function strip(line: string): string {
  const prefix = PREFIXES.find((p) => line.startsWith(p));
  return prefix ? line.slice(prefix.length) : line;
}

/** Klipper's recent console lines in a corner card. Errors (`!!`) are red. */
export function ConsoleCard({ lines }: Props) {
  if (lines.length === 0) return null;
  return (
    <aside
      aria-label="Klipper console"
      className="fixed bottom-4 left-4 z-40 w-96 max-w-[calc(100vw-2rem)] md:left-[calc(var(--sidebar-width)+--spacing(4))]"
    >
      <Card className="gap-1 p-3 font-mono text-xs">
        {lines.map((line, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the same line can repeat, and lines only append at the end
          <p key={i} className={line.startsWith("!!") ? "text-destructive" : "text-muted-foreground"}>
            {strip(line)}
          </p>
        ))}
      </Card>
    </aside>
  );
}
