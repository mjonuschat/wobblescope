import { Link, Outlet, useRouter, useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";
import { toast } from "sonner";

import { AppSidebar, MobileNav } from "@/components/AppSidebar";
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
  const router = useRouter();
  useEffect(
    () =>
      router.subscribe("onResolved", ({ pathChanged }) => {
        if (pathChanged) toast.dismiss();
      }),
    [router],
  );
  return (
    <TooltipProvider delay={0}>
      <SidebarProvider className="min-h-screen flex-col [--header-height:--spacing(14)]">
        <header className="sticky top-0 z-50 flex h-(--header-height) items-center border-b bg-background text-sm">
          <div className="flex h-full shrink-0 items-center gap-1 px-2 md:w-(--sidebar-width) md:border-r md:px-4">
            <MobileNav key={path} />
            <Link to="/" aria-label="Wobblescope">
              <img src={`${import.meta.env.BASE_URL}favicon.svg`} alt="Wobblescope" className="size-9 sm:hidden" />
              <img
                src={`${import.meta.env.BASE_URL}${resolved === "dark" ? "logo-dark" : "logo-light"}.svg`}
                alt=""
                className="hidden h-9 w-auto sm:block md:h-11"
              />
            </Link>
          </div>
          <div className="flex min-w-0 flex-1 items-center px-3 md:px-4">
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
            <main key={pageKey(path)} className="animate-in p-4 duration-200 md:p-6 fade-in-0 slide-in-from-bottom-1">
              <Outlet />
            </main>
          </div>
        </div>
        <Toaster theme={resolved} position="bottom-right" offset={{ bottom: 88, right: 24 }} />
      </SidebarProvider>
    </TooltipProvider>
  );
}
