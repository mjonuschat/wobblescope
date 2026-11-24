import { EllipsisVerticalIcon, MonitorIcon, MoonIcon, SunIcon, type LucideIcon } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  parseLayout,
  RESULTS_LAYOUTS,
  type ResultsLayout,
  setResultsLayout,
  useResultsLayout,
} from "@/ui-kit/resultsLayout";
import { useTheme } from "@/ui-kit/ThemeProvider";
import { parseMode, THEME_MODES, type ThemeMode } from "@/ui-kit/theme";

const themes: Record<ThemeMode, { label: string; icon: LucideIcon }> = {
  system: { label: "System", icon: MonitorIcon },
  light: { label: "Light", icon: SunIcon },
  dark: { label: "Dark", icon: MoonIcon },
};

const layouts: Record<ResultsLayout, string> = {
  tabs: "Tabs",
  "side-by-side": "Side by side",
};

export function PreferencesMenu() {
  const { mode, setMode } = useTheme();
  const layout = useResultsLayout();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label="Preferences"
        className="flex h-9 items-center rounded-md border px-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2"
      >
        <EllipsisVerticalIcon className="size-5" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Theme</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={mode} onValueChange={(value) => setMode(parseMode(value))}>
            {THEME_MODES.map((m) => {
              const { label, icon: ModeIcon } = themes[m];
              return (
                <DropdownMenuRadioItem key={m} value={m}>
                  <ModeIcon />
                  {label}
                </DropdownMenuRadioItem>
              );
            })}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuLabel>Results layout</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={layout} onValueChange={(value) => setResultsLayout(parseLayout(value))}>
            {RESULTS_LAYOUTS.map((l) => (
              <DropdownMenuRadioItem key={l} value={l}>
                {layouts[l]}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
