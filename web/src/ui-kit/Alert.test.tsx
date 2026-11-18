import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Alert } from "./Alert";

describe("Alert", () => {
  it("announces_a_warning_as_status_and_a_danger_as_alert_with_its_title_and_lines", () => {
    const { rerender } = render(
      <Alert tone="warning" title="Check this">
        <p>one</p>
        <p>two</p>
      </Alert>,
    );
    expect(screen.getByRole("status").textContent).toBe("Check thisonetwo");
    rerender(
      <Alert tone="danger" title="Stop">
        <p>one</p>
      </Alert>,
    );
    expect(screen.getByRole("alert").textContent).toBe("Stopone");
  });

  it("lets_the_caller_set_the_role_and_shows_an_action", () => {
    render(
      <Alert tone="warning" variant="inline" role="alert" action={<button type="button">Reload</button>}>
        Slow
      </Alert>,
    );
    expect(screen.getByRole("alert").textContent).toContain("Slow");
    expect(screen.getByRole("button", { name: "Reload" })).toBeTruthy();
  });
});
