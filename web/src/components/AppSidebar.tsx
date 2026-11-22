import { Link, useRouterState } from "@tanstack/react-router";
import { useState } from "react";
import {
  ActivityIcon,
  AudioLinesIcon,
  ChevronsLeftRightIcon,
  CompassIcon,
  FlaskConicalIcon,
  FolderClockIcon,
  MenuIcon,
  PrinterIcon,
  WavesIcon,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/sidebar";
import { offersBelts, offersVibrations } from "@/pages/run/kinds";
import { useSettings, useStatus } from "@/lib/api/queries";

type Path =
  | "/"
  | "/run/shaper"
  | "/run/belts"
  | "/run/axes-map"
  | "/run/vibrations"
  | "/stream"
  | "/sessions"
  | "/results";

interface Page {
  to: Path;
  label: string;
  icon: LucideIcon;
  children?: { to: Path; label: string; icon: LucideIcon; needsBelts?: boolean; needsVibrations?: boolean }[];
}

const pages: Page[] = [
  { to: "/", label: "Printer", icon: PrinterIcon },
  {
    to: "/run/shaper",
    label: "Measure",
    icon: FlaskConicalIcon,
    children: [
      { to: "/run/axes-map", label: "Axes map calibration", icon: CompassIcon },
      { to: "/run/belts", label: "Belts", icon: ChevronsLeftRightIcon, needsBelts: true },
      { to: "/run/shaper", label: "Input shaper", icon: ActivityIcon },
      { to: "/run/vibrations", label: "Vibrations profile", icon: WavesIcon, needsVibrations: true },
      { to: "/stream", label: "Live view", icon: AudioLinesIcon },
    ],
  },
  { to: "/sessions", label: "History", icon: FolderClockIcon },
];

function NavContent() {
  const version = useStatus().data?.version;
  const kinematics = useSettings().data?.machine.kinematics;
  const beltsOffered = offersBelts(kinematics);
  const vibrationsOffered = offersVibrations(kinematics);
  const path = useRouterState({ select: (state) => state.location.pathname });
  return (
    <>
      <SidebarContent>
        <SidebarGroup>
          <nav aria-label="Pages">
            <SidebarMenu>
              {pages.map(({ to, label, icon: Icon, children }) => (
                <SidebarMenuItem key={label}>
                  <SidebarMenuButton
                    isActive={to === "/sessions" ? /\/(sessions|results)(\/|$)/.test(path) : undefined}
                    render={
                      <Link
                        to={to}
                        activeOptions={{ exact: to === "/" }}
                        // A page with sub-pages leaves the highlight to the one that is open.
                        activeProps={children ? undefined : { "data-active": true }}
                      />
                    }
                  >
                    <Icon />
                    <span>{label}</span>
                  </SidebarMenuButton>
                  {children && (
                    <SidebarMenuSub aria-label={`${label} pages`}>
                      {children
                        .filter(
                          (child) =>
                            (!child.needsBelts || beltsOffered) && (!child.needsVibrations || vibrationsOffered),
                        )
                        .map((child) => (
                          <SidebarMenuSubItem key={child.to}>
                            <SidebarMenuSubButton render={<Link to={child.to} activeProps={{ "data-active": true }} />}>
                              <child.icon />
                              <span>{child.label}</span>
                            </SidebarMenuSubButton>
                          </SidebarMenuSubItem>
                        ))}
                    </SidebarMenuSub>
                  )}
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </nav>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <p className="p-2 font-mono text-sm text-muted-foreground">Version {version ?? "unknown"}</p>
      </SidebarFooter>
    </>
  );
}

/** The page links, in a column beside the page from the `md` breakpoint up. */
export function AppSidebar() {
  return (
    <Sidebar
      collapsible="none"
      className="sticky top-(--header-height) hidden h-[calc(100svh-var(--header-height))] shrink-0 border-r md:flex"
    >
      <NavContent />
    </Sidebar>
  );
}

/**
 * Below `md` the sidebar does not fit beside a page, so its links open from a menu button in the header.
 * The caller keys it by the path, so a new page remounts it closed.
 */
export function MobileNav() {
  const [open, setOpen] = useState(false);
  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger render={<Button variant="ghost" size="icon" aria-label="Open menu" className="md:hidden" />}>
        <MenuIcon />
      </SheetTrigger>
      <SheetContent side="left" showCloseButton={false} className="w-72 bg-sidebar p-0 text-sidebar-foreground">
        <SheetTitle className="sr-only">Pages</SheetTitle>
        <NavContent />
      </SheetContent>
    </Sheet>
  );
}
