import { Link } from "@tanstack/react-router";
import { useState } from "react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useDeleteResult, useResults } from "@/lib/api/queries";
import { formatBytes, formatTime } from "@/lib/format";

function DeleteResultDialog({ id, onClose }: { id: string | null; onClose: () => void }) {
  const remove = useDeleteResult();
  const close = () => {
    remove.reset();
    onClose();
  };
  return (
    <AlertDialog open={id !== null} onOpenChange={(open) => !open && close()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete result {id}?</AlertDialogTitle>
          <AlertDialogDescription>
            The saved analysis and its recordings are removed from the printer's disk and cannot be restored. The
            session it came from is not touched.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {remove.isError && (
          <p role="alert" className="text-sm text-destructive">
            {remove.error.message}
          </p>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={remove.isPending}
            onClick={() => id && remove.mutate(id, { onSuccess: close })}
          >
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export function ResultsPage() {
  const list = useResults();
  const [deleting, setDeleting] = useState<string | null>(null);

  let body;
  if (list.isPending) {
    body = <p className="text-sm text-muted-foreground">Loading results…</p>;
  } else if (list.isError) {
    body = (
      <div role="alert" className="space-y-2 text-sm">
        <p className="text-destructive">{list.error.message}</p>
        <Button variant="outline" onClick={() => void list.refetch()}>
          Retry
        </Button>
      </div>
    );
  } else if (list.data.results.length === 0) {
    body = <p className="text-sm">No saved results yet. Open a session, analyse it and use Save result.</p>;
  } else {
    body = (
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Result</TableHead>
            <TableHead>Session</TableHead>
            <TableHead>Core</TableHead>
            <TableHead>Size</TableHead>
            <TableHead />
          </TableRow>
        </TableHeader>
        <TableBody>
          {list.data.results.map((r) => (
            <TableRow key={r.id}>
              <TableCell>
                {r.unusable === null ? (
                  <Link to="/results/$rid" params={{ rid: r.id }} className="font-medium underline">
                    {r.label ?? r.id}
                  </Link>
                ) : (
                  <span className="font-medium">{r.label ?? r.id}</span>
                )}
                {r.created_at && <div className="text-xs text-muted-foreground">{formatTime(r.created_at)}</div>}
                {r.unusable !== null && <div className="text-xs text-destructive">Unusable: {r.unusable}</div>}
              </TableCell>
              <TableCell>{r.session_id ?? "–"}</TableCell>
              <TableCell>{r.core_version ?? "–"}</TableCell>
              <TableCell>{formatBytes(r.size_bytes)}</TableCell>
              <TableCell className="text-right">
                <Button variant="outline" size="sm" onClick={() => setDeleting(r.id)}>
                  Delete
                </Button>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    );
  }

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Results</h1>
      {body}
      <DeleteResultDialog id={deleting} onClose={() => setDeleting(null)} />
    </div>
  );
}
