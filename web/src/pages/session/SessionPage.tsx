import { Link } from "@tanstack/react-router";
import { ArrowLeftIcon, DownloadIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { sessionDownloadUrl } from "@/lib/api/endpoints";

import { SessionResults } from "./SessionResults";

/** A stored session: its title and its analysis. */
export function SessionPage({ id }: { id: string }) {
  return (
    <div className="mx-auto max-w-5xl space-y-6">
      <div className="flex items-center gap-3">
        <Button render={<Link to="/sessions" />} variant="outline" size="icon" aria-label="Back to sessions">
          <ArrowLeftIcon />
        </Button>
        <h1 className="font-mono text-xl font-semibold">{id}</h1>
        <Button
          render={<a href={sessionDownloadUrl(id)} download />}
          nativeButton={false}
          variant="outline"
          size="icon"
          aria-label="Download"
          className="ml-auto"
        >
          <DownloadIcon />
        </Button>
      </div>
      <SessionResults id={id} />
    </div>
  );
}
