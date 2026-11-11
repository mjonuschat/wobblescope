import { Link, useRouterState } from "@tanstack/react-router";
import {
  ActivityIcon,
  AudioLinesIcon,
  FlaskConicalIcon,
  FolderClockIcon,
  PrinterIcon,
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
import { useStatus } from "@/lib/api/queries";

type Path = "/" | "/run/shaper" | "/stream" | "/sessions" | "/results";

interface Page {
  to: Path;
  label: string;
  icon: LucideIcon;
  children?: { to: Path; label: string; icon: LucideIcon }[];
}

const pages: Page[] = [
  { to: "/", label: "Printer", icon: PrinterIcon },
  {
    to: "/run/shaper",
    label: "Measure",
    icon: FlaskConicalIcon,
    children: [
      { to: "/run/shaper", label: "Input Shaper", icon: ActivityIcon },
      { to: "/stream", label: "Live view", icon: AudioLinesIcon },
    ],
  },
  { to: "/sessions", label: "History", icon: FolderClockIcon },
];

export function AppSidebar() {
  const version = useStatus().data?.version;
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
                      {children.map((child) => (
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
