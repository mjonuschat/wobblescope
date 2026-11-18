import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { motorResult, vibrationsResult } from "@/test/vibrations";

import { GoodAngleRanges, GoodSpeedRanges, VibrationsSummary } from "./VibrationsSummary";

describe("the vibrations summary", () => {
  it("shows_the_recommended_speed_ranges_across_the_whole_card", () => {
    render(<VibrationsSummary result={vibrationsResult()} />);
    const heading = screen.getByText("Recommended speed ranges");
    expect(screen.queryByText("Good speed ranges")).toBeNull();
    // Not in a column of a grid, so the table takes the card's full width.
    expect(heading.parentElement?.parentElement?.className ?? "").not.toContain("grid-cols");
    expect(heading.parentElement?.className).not.toContain("md:");
  });

  it("no_longer_lists_the_speeds_to_avoid_as_the_heatmap_labels_them", () => {
    render(<VibrationsSummary result={vibrationsResult()} />);
    expect(screen.queryByText("Speeds to avoid")).toBeNull();
    expect(screen.queryByText(/avoid setting a speed/i)).toBeNull();
  });

  it("titles_the_value_column_of_both_range_tables_the_same_way", () => {
    render(
      <>
        <GoodSpeedRanges result={vibrationsResult()} />
        <GoodAngleRanges result={vibrationsResult()} />
      </>,
    );
    expect(screen.getAllByText("Avg. vibration (% of max)")).toHaveLength(2);
    expect(screen.queryByText(/Energy \(% of max\)/)).toBeNull();
  });

  it("leaves_the_angle_ranges_to_their_plot_and_the_motor_figures_to_theirs", () => {
    render(<VibrationsSummary result={vibrationsResult({ motor: motorResult({ low_freq_warning: true }) })} />);
    expect(screen.queryByText("Good angle ranges")).toBeNull();
    expect(screen.queryByText("Symmetry")).toBeNull();
    expect(screen.queryByText("Motor resonance")).toBeNull();
    expect(screen.queryByText(/resonance is low/i)).toBeNull();
  });

  it("lists_the_recommended_speed_ranges_as_a_table_of_green_badges", () => {
    render(<GoodSpeedRanges result={vibrationsResult()} />);
    const speeds = screen.getByText("Recommended speed ranges").parentElement as HTMLElement;
    expect(within(speeds).getByText("Range (mm/s)")).toBeTruthy();
    const row = within(speeds).getAllByRole("row")[1];
    expect(row.textContent).toContain("60 to 80");
    expect(row.textContent).toContain("12.5");
    expect((row.querySelector("span") as HTMLElement).style.backgroundColor).not.toBe("cyan");
  });

  it("lists_the_good_angle_ranges_as_a_table", () => {
    render(<GoodAngleRanges result={vibrationsResult()} />);
    const angles = screen.getByText("Good angle ranges").parentElement as HTMLElement;
    expect(within(angles).getByText("Range (degrees)")).toBeTruthy();
    const row = within(angles).getAllByRole("row")[1];
    expect(row.textContent).toContain("45 to 135");
    expect(row.textContent).toContain("20.0");
  });

  it("says_so_when_there_are_no_good_ranges", () => {
    render(
      <>
        <GoodSpeedRanges result={vibrationsResult({ good_speeds: [] })} />
        <GoodAngleRanges result={vibrationsResult({ good_angles: [] })} />
      </>,
    );
    expect(screen.getByText("No recommended speed ranges were found.")).toBeTruthy();
    expect(screen.getByText("No good angle ranges were found.")).toBeTruthy();
  });
});
