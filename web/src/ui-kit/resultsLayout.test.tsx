import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { PreferencesMenu } from "./PreferencesMenu";
import { parseLayout, setResultsLayout, useResultsLayout } from "./resultsLayout";
import { ThemeProvider } from "./ThemeProvider";

afterEach(() => {
  window.localStorage.clear();
  act(() => setResultsLayout("tabs"));
  window.localStorage.clear();
});

function Probe() {
  return <p>{useResultsLayout()}</p>;
}

describe("the results layout", () => {
  it("is Tabs unless the stored value says otherwise", () => {
    expect(parseLayout(null)).toBe("tabs");
    expect(parseLayout("nonsense")).toBe("tabs");
    expect(parseLayout("side-by-side")).toBe("side-by-side");
    render(<Probe />);
    expect(screen.getByText("tabs")).toBeTruthy();
  });

  it("is stored, and a component that reads it follows a change", () => {
    render(<Probe />);
    act(() => setResultsLayout("side-by-side"));
    expect(screen.getByText("side-by-side")).toBeTruthy();
    expect(window.localStorage.getItem("wobblescope-results-layout")).toBe("side-by-side");
  });
});

describe("the preferences menu", () => {
  function open() {
    render(
      <ThemeProvider storageKey="test-theme">
        <PreferencesMenu />
        <Probe />
      </ThemeProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Preferences" }));
  }

  it("offers the theme and the results layout, with Tabs chosen by default", async () => {
    open();
    expect(await screen.findByRole("menuitemradio", { name: "Dark" })).toBeTruthy();
    expect(screen.getByRole("menuitemradio", { name: "Tabs" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("menuitemradio", { name: "Side by side" }).getAttribute("aria-checked")).toBe("false");
  });

  it("changes the layout", async () => {
    open();
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "Side by side" }));
    expect(await screen.findByText("side-by-side")).toBeTruthy();
  });
});
