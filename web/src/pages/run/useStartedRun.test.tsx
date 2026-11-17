import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { FinishedTest } from "@/lib/generated/FinishedTest";
import type { TestState } from "@/lib/generated/TestState";

import { useStartedRun } from "./useStartedRun";

interface Live {
  seq: number;
  test: TestState | null;
  last: FinishedTest | null;
}

const running = (id: string) => ({ id }) as TestState;
const finished = (id: string) => ({ id }) as FinishedTest;

function setup(initial: Live) {
  return renderHook(({ live }) => useStartedRun(live), { initialProps: { live: initial } });
}

describe("useStartedRun", () => {
  it("keeps the id while no newer snapshot has arrived, as right after the form posted", () => {
    const view = setup({ seq: 4, test: null, last: finished("old") });
    act(() => view.result.current[1]("new"));
    expect(view.result.current[0]).toBe("new");
    view.rerender({ live: { seq: 4, test: null, last: finished("old") } });
    expect(view.result.current[0]).toBe("new");
  });

  it("keeps the id while the snapshot names the running test, and after it ends", () => {
    const view = setup({ seq: 4, test: null, last: null });
    act(() => view.result.current[1]("new"));
    view.rerender({ live: { seq: 5, test: running("new"), last: null } });
    expect(view.result.current[0]).toBe("new");
    view.rerender({ live: { seq: 6, test: null, last: finished("new") } });
    expect(view.result.current[0]).toBe("new");
  });

  it("drops the id when a newer snapshot knows nothing of the run, as after a daemon restart", () => {
    const view = setup({ seq: 4, test: null, last: null });
    act(() => view.result.current[1]("new"));
    view.rerender({ live: { seq: 5, test: null, last: null } });
    expect(view.result.current[0]).toBeNull();
  });

  it("drops the id when another run holds the slot or finished last", () => {
    const view = setup({ seq: 4, test: null, last: null });
    act(() => view.result.current[1]("mine"));
    view.rerender({ live: { seq: 5, test: running("theirs"), last: null } });
    expect(view.result.current[0]).toBeNull();
    act(() => view.result.current[1]("mine"));
    view.rerender({ live: { seq: 6, test: null, last: finished("theirs") } });
    expect(view.result.current[0]).toBeNull();
  });

  it("can be cleared by the page", () => {
    const view = setup({ seq: 1, test: null, last: null });
    act(() => view.result.current[1]("new"));
    act(() => view.result.current[1](null));
    expect(view.result.current[0]).toBeNull();
  });
});
