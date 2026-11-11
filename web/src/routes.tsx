import { createRootRoute, createRoute, redirect } from "@tanstack/react-router";

import { RootLayout } from "@/components/RootLayout";
import { HistoryLayout } from "@/pages/history/HistoryLayout";
import { PrinterPage } from "@/pages/printer/PrinterPage";
import { ResultRoute } from "@/pages/results/ResultRoute";
import { ResultsPage } from "@/pages/results/ResultsPage";
import { RunPage } from "@/pages/run/RunPage";
import { SessionRoute } from "@/pages/session/SessionRoute";
import { SessionsPage } from "@/pages/sessions/SessionsPage";

const rootRoute = createRootRoute({ component: RootLayout });

const printerRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: PrinterPage,
});

const runRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/run",
  beforeLoad: () => {
    throw redirect({ to: "/run/shaper" });
  },
});

const shaperRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/run/shaper",
  component: RunPage,
});

const historyRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "history",
  component: HistoryLayout,
});

const sessionsRoute = createRoute({
  getParentRoute: () => historyRoute,
  path: "/sessions",
  component: SessionsPage,
});

const sessionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sessions/$id",
  component: SessionRoute,
});

const resultsRoute = createRoute({
  getParentRoute: () => historyRoute,
  path: "/results",
  component: ResultsPage,
});

const resultRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/results/$rid",
  component: ResultRoute,
});

export const routeTree = rootRoute.addChildren([
  printerRoute,
  runRoute,
  shaperRoute,
  historyRoute.addChildren([sessionsRoute, resultsRoute]),
  sessionRoute,
  resultRoute,
]);
