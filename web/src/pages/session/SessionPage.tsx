import { Link } from "@tanstack/react-router";
import { ArrowLeftIcon, DownloadIcon } from "lucide-react";
import { useState } from "react";
import { createPortal } from "react-dom";

import { Button } from "@/components/ui/button";
import { sessionDownloadUrl } from "@/lib/api/endpoints";
import { useSession } from "@/lib/api/queries";
import { useResultsLayout } from "@/ui-kit/resultsLayout";

import { sessionAxes } from "./axes";
import { SessionResults } from "./SessionResults";
import { PageHeader } from "@/ui-kit/PageHeader";
import { Panel } from "@/ui-kit/Panel";

export function SessionPage({ id }: { id: string }) {
  // The Save button belongs to the analysis, which lives below. It is drawn into the page header, left of Download.
  const [saveSlot, setSaveSlot] = useState<HTMLElement | null>(null);
  // Both axes side by side need more room than the page's usual width.
  const session = useSession(id).data;
  const wide =
    useResultsLayout() === "side-by-side" && session?.summary.kind === "axes" && sessionAxes(session.meta).length >= 2;
  return (
    <div className={`mx-auto space-y-6 ${wide ? "max-w-[1600px]" : "max-w-5xl"}`}>
      <PageHeader
        title={id}
        mono
        leading={
          <Button render={<Link to="/sessions" />} variant="outline" size="icon" aria-label="Back to sessions">
            <ArrowLeftIcon />
          </Button>
        }
        actions={
          <>
            <span ref={setSaveSlot} className="contents" />
            <Button
              render={<a href={sessionDownloadUrl(id)} download />}
              nativeButton={false}
              variant="outline"
              size="icon"
              aria-label="Download"
            >
              <DownloadIcon />
            </Button>
          </>
        }
      />
      <SessionResults
        id={id}
        header={(save, summary) => (
          <>
            {save && saveSlot ? createPortal(save, saveSlot) : null}
            {summary ? <Panel title="Result">{summary}</Panel> : null}
          </>
        )}
      />
    </div>
  );
}
