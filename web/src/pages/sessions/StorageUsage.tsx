import { formatBytes } from "@/lib/format";
import type { Usage } from "@/lib/generated/Usage";

export function StorageUsage({ usage }: { usage: Usage }) {
  const committed = usage.used_bytes + usage.reserved_bytes;
  const percent = usage.cap_bytes > 0 ? Math.min(100, (committed / usage.cap_bytes) * 100) : 0;
  const pinned = usage.cap_bytes > 0 ? Math.min(100, (usage.pinned_bytes / usage.cap_bytes) * 100) : 0;
  return (
    <section aria-label="Storage use" className="space-y-2 rounded-lg border p-4">
      <div className="flex items-baseline justify-between text-sm">
        <span>
          {formatBytes(committed)} of {formatBytes(usage.cap_bytes)} used
        </span>
        <span className="text-muted-foreground">{usage.session_count} sessions</span>
      </div>
      <div
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={usage.cap_bytes}
        aria-valuenow={committed}
        className="relative h-2 overflow-hidden rounded bg-muted"
      >
        <div className="absolute inset-y-0 left-0 bg-primary/60" style={{ width: `${percent}%` }} />
        <div className="absolute inset-y-0 left-0 bg-primary" style={{ width: `${pinned}%` }} />
      </div>
      <dl className="grid grid-cols-2 gap-x-6 text-sm text-muted-foreground sm:grid-cols-4">
        <div>
          <dt className="inline">Stored: </dt>
          <dd className="inline">{formatBytes(usage.used_bytes)}</dd>
        </div>
        <div>
          <dt className="inline">Reserved: </dt>
          <dd className="inline">{formatBytes(usage.reserved_bytes)}</dd>
        </div>
        <div>
          <dt className="inline">Pinned: </dt>
          <dd className="inline">{formatBytes(usage.pinned_bytes)}</dd>
        </div>
        <div>
          <dt className="inline">Free disk: </dt>
          <dd className="inline">{formatBytes(usage.free_bytes)}</dd>
        </div>
      </dl>
    </section>
  );
}
