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

export function RootLayout() {
  useRecoverStorage();
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
              <img src={`${import.meta.env.BASE_URL}logo-dark.svg`} alt="Wobblescope" className="h-11 w-auto" />
            </Link>
          </div>
          <div className="flex flex-1 items-center px-4">
            <StatusDots />
            <PrinterControls />
          </div>
        </header>
        <div className="flex flex-1">
          <AppSidebar />
          <div className="min-w-0 flex-1">
            <StatusBanner />
            <main className="p-6">
              <Outlet />
            </main>
          </div>
        </div>
        <Toaster position="bottom-right" offset={{ bottom: 88, right: 24 }} />
      </SidebarProvider>
    </TooltipProvider>
  );
}
