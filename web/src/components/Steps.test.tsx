import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { Steps } from "./Steps";

const steps = [
  { id: "setup", label: "Setup" },
  { id: "measuring", label: "Measuring" },
  { id: "results", label: "Results" },
];

describe("Steps", () => {
  it("marks the earlier steps done and the current one", () => {
    render(<Steps steps={steps} current="measuring" />);
    const states = [...document.querySelectorAll("[data-state]")].map((el) => el.getAttribute("data-state"));
    expect(states).toEqual(["done", "current", "upcoming"]);
    expect(document.querySelector('[aria-current="step"]')?.textContent).toContain("Measuring");
  });

  it("makes only the selectable steps buttons", () => {
    const onSelect = vi.fn();
    render(<Steps steps={steps} current="results" selectable={["setup"]} onSelect={onSelect} />);
    expect(screen.getAllByRole("button")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: /Setup/ }));
    expect(onSelect).toHaveBeenCalledWith("setup");
  });

  it("has no buttons without selectable steps", () => {
    render(<Steps steps={steps} current="setup" />);
    expect(screen.queryAllByRole("button")).toHaveLength(0);
  });
});
