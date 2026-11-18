import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";
import { toast } from "sonner";

import { AppSidebar } from "@/components/AppSidebar";
import { PrinterControls } from "@/components/PrinterControls";
import { StatusBanner } from "@/components/StatusBanner";
import { StatusDots } from "@/components/StatusDots";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SidebarProvider } from "@/components/ui/sidebar";
import { useRecoverStorage } from "@/lib/api/queries";
import { ThemeToggle } from "@/ui-kit/ThemeToggle";
import { useTheme } from "@/ui-kit/ThemeProvider";

/** The History tabs share one mounted layout, so the sessions and results lists count as one page. */
const pageKey = (path: string) => (/^\/(sessions|results)$/.test(path) ? "history" : path);

export function RootLayout() {
  useRecoverStorage();
  const { resolved } = useTheme();
  const path = useRouterState({ select: (state) => state.location.pathname });
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new path is the trigger
  useEffect(() => {
    toast.dismiss();
  }, [path]);
  return (
    <TooltipProvider delay={0}>
      <SidebarProvider className="min-h-screen flex-col [--header-height:--spacing(14)]">
        <header className="sticky top-0 z-50 flex h-(--header-height) items-center border-b bg-background text-sm">
          <div className="flex h-full w-(--sidebar-width) shrink-0 items-center border-r px-4">
            <Link to="/" aria-label="Wobblescope">
              <img
                src={`${import.meta.env.BASE_URL}${resolved === "dark" ? "logo-dark" : "logo-light"}.svg`}
                alt="Wobblescope"
                className="h-11 w-auto"
              />
            </Link>
          </div>
          <div className="flex flex-1 items-center px-4">
            <StatusDots />
            <PrinterControls />
            <div className="ml-3">
              <ThemeToggle />
            </div>
          </div>
        </header>
        <div className="flex flex-1">
          <AppSidebar />
          <div className="min-w-0 flex-1">
            <StatusBanner />
            <main key={pageKey(path)} className="animate-in p-6 duration-200 fade-in-0 slide-in-from-bottom-1">
              <Outlet />
            </main>
          </div>
        </div>
        <Toaster theme={resolved} position="bottom-right" offset={{ bottom: 88, right: 24 }} />
      </SidebarProvider>
    </TooltipProvider>
  );
}
