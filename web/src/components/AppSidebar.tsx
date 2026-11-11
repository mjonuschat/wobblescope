import { Link } from "@tanstack/react-router";
import { FlaskConicalIcon, FolderClockIcon, PrinterIcon, type LucideIcon } from "lucide-react";

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { useStatus } from "@/lib/api/queries";

const pages: { to: "/" | "/run" | "/sessions"; label: string; icon: LucideIcon }[] = [
  { to: "/", label: "Printer", icon: PrinterIcon },
  { to: "/run", label: "Run test", icon: FlaskConicalIcon },
  { to: "/sessions", label: "Sessions", icon: FolderClockIcon },
];

export function AppSidebar() {
  const version = useStatus().data?.version;
  return (
    <Sidebar
      collapsible="none"
      className="sticky top-(--header-height) h-[calc(100svh-var(--header-height))] shrink-0 border-r"
    >
      <SidebarContent>
        <SidebarGroup>
          <nav aria-label="Pages">
            <SidebarMenu>
              {pages.map(({ to, label, icon: Icon }) => (
                <SidebarMenuItem key={to}>
                  <SidebarMenuButton
                    render={
                      <Link to={to} activeOptions={{ exact: to === "/" }} activeProps={{ "data-active": true }} />
                    }
                  >
                    <Icon />
                    <span>{label}</span>
                  </SidebarMenuButton>
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
