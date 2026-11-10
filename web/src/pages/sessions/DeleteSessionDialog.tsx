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
import { useDeleteSession } from "@/lib/api/queries";

interface Props {
  /** The id to delete, or null while the dialog is closed. */
  sessionId: string | null;
  onClose: () => void;
}

export function DeleteSessionDialog({ sessionId, onClose }: Props) {
  const remove = useDeleteSession();
  const close = () => {
    remove.reset();
    onClose();
  };
  return (
    <AlertDialog open={sessionId !== null} onOpenChange={(open) => !open && close()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete session {sessionId}?</AlertDialogTitle>
          <AlertDialogDescription>
            Its recordings are removed from the printer's disk and cannot be restored.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {remove.isError && (
          <p role="alert" className="text-sm text-destructive">
            {remove.error.message}
          </p>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={remove.isPending}
            onClick={() => sessionId && remove.mutate(sessionId, { onSuccess: close })}
          >
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
