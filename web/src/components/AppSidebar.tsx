import { Link, useRouterState } from "@tanstack/react-router";
import {
  ActivityIcon,
  AudioLinesIcon,
  ChevronsLeftRightIcon,
  CompassIcon,
  FlaskConicalIcon,
  FolderClockIcon,
  PrinterIcon,
  WavesIcon,
  type LucideIcon,
} from "lucide-react";

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
      { to: "/run/shaper", label: "Input shaper", icon: ActivityIcon },
      { to: "/run/belts", label: "Belts", icon: ChevronsLeftRightIcon, needsBelts: true },
      { to: "/run/axes-map", label: "Axes map calibration", icon: CompassIcon },
      { to: "/run/vibrations", label: "Vibrations profile", icon: WavesIcon, needsVibrations: true },
      { to: "/stream", label: "Live view", icon: AudioLinesIcon },
    ],
  },
  { to: "/sessions", label: "History", icon: FolderClockIcon },
];

export function AppSidebar() {
  const version = useStatus().data?.version;
  const kinematics = useSettings().data?.machine.kinematics;
  const beltsOffered = offersBelts(kinematics);
  const vibrationsOffered = offersVibrations(kinematics);
  const path = useRouterState({ select: (state) => state.location.pathname });
  return (
    <Sidebar
      collapsible="none"
      className="sticky top-(--header-height) h-[calc(100svh-var(--header-height))] shrink-0 border-r"
    >
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
    </Sidebar>
  );
}
