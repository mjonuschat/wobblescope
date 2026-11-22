import { Link } from "@tanstack/react-router";
import { ArrowLeftIcon, DownloadIcon } from "lucide-react";
import { useState } from "react";
import { createPortal } from "react-dom";

import { Button } from "@/components/ui/button";
import { sessionDownloadUrl } from "@/lib/api/endpoints";

import { SessionResults } from "./SessionResults";
import { PageHeader } from "@/ui-kit/PageHeader";
import { Panel } from "@/ui-kit/Panel";

export function SessionPage({ id }: { id: string }) {
  // The Save button belongs to the analysis, which lives below. It is drawn into the page header, left of Download.
  const [saveSlot, setSaveSlot] = useState<HTMLElement | null>(null);
  return (
    <div className="mx-auto max-w-5xl space-y-6">
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
