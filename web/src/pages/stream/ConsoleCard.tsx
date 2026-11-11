interface Props {
  lines: string[]
}

const PREFIXES = ['// ', 'echo: ']

function strip(line: string): string {
  const prefix = PREFIXES.find((p) => line.startsWith(p))
  return prefix ? line.slice(prefix.length) : line
}

/** Klipper's recent console lines in a corner card. Errors (`!!`) are red. */
export function ConsoleCard({ lines }: Props) {
  if (lines.length === 0) return null
  return (
    <aside
      aria-label="Klipper console"
      className="fixed right-4 bottom-4 w-96 max-w-[90vw] space-y-1 rounded-md border border-neutral-700 bg-neutral-900 p-3 font-mono text-xs"
    >
      {lines.map((line, i) => (
        <p key={i} className={line.startsWith('!!') ? 'text-red-400' : 'text-neutral-300'}>
          {strip(line)}
        </p>
      ))}
    </aside>
  )
}
