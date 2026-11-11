import { createRootRoute, createRoute } from "@tanstack/react-router";

import { RootLayout } from "@/components/RootLayout";
import { PrinterPage } from "@/pages/printer/PrinterPage";
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
  component: RunPage,
});

const sessionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sessions",
  component: SessionsPage,
});

const sessionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/sessions/$id",
  component: SessionRoute,
});

export const routeTree = rootRoute.addChildren([printerRoute, runRoute, sessionsRoute, sessionRoute]);
