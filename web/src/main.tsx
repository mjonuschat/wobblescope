import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createHashHistory, createRouter } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createQueryClient } from "./lib/api/queries";
import "./index.css";
import { routeTree } from "./routes";
import { ThemeProvider } from "./ui-kit/ThemeProvider";

const queryClient = createQueryClient();
const router = createRouter({
  routeTree,
  history: createHashHistory(),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider storageKey="wobblescope-theme">
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>,
);
