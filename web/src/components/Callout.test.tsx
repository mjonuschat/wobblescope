import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Callout } from "./Callout";

describe("Callout", () => {
  it("announces_a_warning_as_status_and_an_error_as_alert_with_its_title_and_lines", () => {
    const { rerender } = render(
      <Callout tone="warn" title="Check this">
        <p>one</p>
        <p>two</p>
      </Callout>,
    );
    expect(screen.getByRole("status").textContent).toBe("Check thisonetwo");
    rerender(
      <Callout tone="error" title="Stop">
        <p>one</p>
      </Callout>,
    );
    expect(screen.getByRole("alert").textContent).toBe("Stopone");
  });
});
