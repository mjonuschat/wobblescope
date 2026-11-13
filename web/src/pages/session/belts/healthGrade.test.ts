import { describe, expect, it } from "vitest";
import { healthGrade } from "./healthGrade";

describe("healthGrade", () => {
  it.each([
    ["Excellent mechanical health", "Excellent"],
    ["Good mechanical health", "Good"],
    ["Acceptable mechanical health", "Acceptable"],
    ["Potential signs of a mechanical issue", "Potential signs of an issue"],
    ["Likely a mechanical issue", "Likely an issue"],
    ["Mechanical issue detected", "Issue detected"],
    ["Unknown mechanical health", "Unknown"],
  ])("maps %s to %s", (text, grade) => {
    expect(healthGrade(text)).toBe(grade);
  });

  it("returns any other text unchanged", () => {
    expect(healthGrade("Something new")).toBe("Something new");
  });
});
