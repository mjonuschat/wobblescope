import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { motorResult, vibrationsResult } from "@/test/vibrations";

import { VibrationsSummary } from "./VibrationsSummary";

describe("the vibrations summary", () => {
  it("lists_peaks_zones_angles_symmetry_and_motor", () => {
    render(<VibrationsSummary result={vibrationsResult({ motor: motorResult({ low_freq_warning: true }) })} />);
    expect(screen.getByText("Speeds to avoid").parentElement?.textContent).toContain("1: 40 mm/s");
    expect(screen.getByText("Speeds to avoid").parentElement?.textContent).toContain("2: 80 mm/s");
    expect(screen.getByText(/avoid setting a speed near these values/i)).toBeTruthy();
    expect(screen.getByText("Zone 1").parentElement?.textContent).toContain("60 to 80 mm/s");
    expect(screen.getByText("Zone 1").parentElement?.textContent).toContain("12.5% of max energy");
    expect(screen.getByText("Angle range 1").parentElement?.textContent).toContain("45 to 135 degrees");
    expect(screen.getByText("Angle range 1").parentElement?.textContent).toContain("20.0% of max energy");
    expect(screen.getByText("Symmetry").nextElementSibling?.textContent).toBe("87.7%");
    expect(screen.getByText("Motor resonance").nextElementSibling?.textContent).toBe("30.0 Hz");
    expect(screen.getByText("Motor damping ratio").nextElementSibling?.textContent).toBe("0.123");
    expect(screen.getByText(/resonance is low/i)).toBeTruthy();
  });

  it("says_not_available_without_symmetry_and_hides_the_motor_of_a_worst_case", () => {
    render(<VibrationsSummary result={vibrationsResult({ symmetry: null, motor: null })} />);
    expect(screen.getByText("Symmetry").nextElementSibling?.textContent).toBe("not available");
    expect(screen.queryByText("Motor resonance")).toBeNull();
  });

  it("says_so_when_there_are_no_peaks_or_zones", () => {
    render(
      <VibrationsSummary result={vibrationsResult({ peaks: [], peak_speeds: [], good_speeds: [], good_angles: [] })} />,
    );
    expect(screen.getByText("No vibration peaks were found.")).toBeTruthy();
    expect(screen.getByText("No good speed ranges were found.")).toBeTruthy();
    expect(screen.getByText("No good angle ranges were found.")).toBeTruthy();
  });
});
