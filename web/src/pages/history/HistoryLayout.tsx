import { Outlet, useNavigate, useRouterState } from "@tanstack/react-router";

import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useStorage } from "@/lib/api/queries";

import { StorageUsage } from "../sessions/StorageUsage";

export function HistoryLayout() {
  const navigate = useNavigate();
  const tab = useRouterState({ select: (s) => (s.location.pathname.endsWith("/results") ? "results" : "sessions") });
  const storage = useStorage();
  return (
    <div className="mx-auto max-w-5xl space-y-4">
      <h1 className="text-xl font-semibold">History</h1>
      <Tabs
        value={tab}
        onValueChange={(value) => void navigate({ to: value === "results" ? "/results" : "/sessions" })}
      >
        <TabsList variant="line">
          <TabsTrigger value="sessions" className="after:bg-primary">
            Sessions
          </TabsTrigger>
          <TabsTrigger value="results" className="after:bg-primary">
            Results
          </TabsTrigger>
        </TabsList>
      </Tabs>
      <Outlet />
      {storage.data && <StorageUsage usage={storage.data} />}
    </div>
  );
}
