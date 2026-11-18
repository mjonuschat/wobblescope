import { MonitorIcon, MoonIcon, SunIcon, type LucideIcon } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTheme } from "@/ui-kit/ThemeProvider";
import { parseMode, THEME_MODES, type ThemeMode } from "@/ui-kit/theme";

const options: Record<ThemeMode, { label: string; icon: LucideIcon }> = {
  system: { label: "System", icon: MonitorIcon },
  light: { label: "Light", icon: SunIcon },
  dark: { label: "Dark", icon: MoonIcon },
};

export function ThemeToggle() {
  const { mode, resolved, setMode } = useTheme();
  const Icon = resolved === "dark" ? MoonIcon : SunIcon;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label="Theme"
        className="flex h-9 items-center rounded-md border px-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2"
      >
        <Icon className="size-5" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup value={mode} onValueChange={(value) => setMode(parseMode(value))}>
          {THEME_MODES.map((m) => {
            const { label, icon: ModeIcon } = options[m];
            return (
              <DropdownMenuRadioItem key={m} value={m}>
                <ModeIcon />
                {label}
              </DropdownMenuRadioItem>
            );
          })}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
