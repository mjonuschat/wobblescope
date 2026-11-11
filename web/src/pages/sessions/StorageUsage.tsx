import { Field, Fields } from "@/components/Fields";
import { Panel } from "@/components/Panel";
import { formatBytes } from "@/lib/format";
import type { Usage } from "@/lib/generated/Usage";

export function StorageUsage({ usage }: { usage: Usage }) {
  const committed = usage.used_bytes + usage.reserved_bytes;
  const percent = usage.cap_bytes > 0 ? Math.min(100, (committed / usage.cap_bytes) * 100) : 0;
  const pinned = usage.cap_bytes > 0 ? Math.min(100, (usage.pinned_bytes / usage.cap_bytes) * 100) : 0;
  return (
    <Panel
      title="Storage"
      label="Storage use"
      action={<span className="text-sm text-muted-foreground">{usage.session_count} sessions</span>}
    >
      <p className="text-sm">
        {formatBytes(committed)} of {formatBytes(usage.cap_bytes)} used
      </p>
      <p className="text-sm text-muted-foreground">
        Saved results: {usage.result_count}, {formatBytes(usage.result_bytes)}
      </p>
      <div
        role="progressbar"
        aria-label="Storage used"
        aria-valuemin={0}
        aria-valuemax={usage.cap_bytes}
        aria-valuenow={committed}
        className="relative h-2 overflow-hidden rounded-full bg-muted"
      >
        <div className="absolute inset-y-0 left-0 bg-primary/60" style={{ width: `${percent}%` }} />
        <div className="absolute inset-y-0 left-0 bg-primary" style={{ width: `${pinned}%` }} />
      </div>
      <Fields className="sm:grid-cols-4 lg:grid-cols-4">
        <Field label="Stored">{formatBytes(usage.used_bytes)}</Field>
        <Field label="Reserved">{formatBytes(usage.reserved_bytes)}</Field>
        <Field label="Pinned">{formatBytes(usage.pinned_bytes)}</Field>
        <Field label="Free Disk">{formatBytes(usage.free_bytes)}</Field>
      </Fields>
    </Panel>
  );
}
