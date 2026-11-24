import type { ECharts } from "echarts/core";
import { describe, expect, it, vi } from "vitest";

import { joinGroup, setRow, syncWithGroup } from "./legendGroup";

function fake(data: string[], selected: Record<string, boolean> = {}) {
  return {
    group: "",
    getOption: () => ({ legend: [{ data, selected }] }),
    dispatchAction: vi.fn(),
  } as unknown as ECharts & { dispatchAction: ReturnType<typeof vi.fn> };
}

describe("joinGroup", () => {
  it("takes over what a chart of the group shows, without sending the change to that chart", () => {
    const first = fake(["X", "Y"], { X: false });
    const leaveFirst = joinGroup(first, "g1");
    const second = fake(["X", "Y"]);
    const leaveSecond = joinGroup(second, "g1");
    expect(second.dispatchAction).toHaveBeenCalledWith({ type: "legendUnSelect", legendIndex: 0, name: "X" });
    expect(second.dispatchAction).toHaveBeenCalledTimes(1);
    expect(first.dispatchAction).not.toHaveBeenCalled();
    expect(second.group).toBe("g1");
    leaveSecond();
    leaveFirst();
  });

  it("changes nothing for a chart that joins an empty group, and clears the group on leaving", () => {
    const only = fake(["X"], { X: false });
    const leave = joinGroup(only, "g2");
    expect(only.dispatchAction).not.toHaveBeenCalled();
    leave();
    expect(only.group).toBe("");
  });

  it("only touches the curves both charts list, and leaves one the other chart lacks as it was", () => {
    const first = fake(["X", "Y"], { Y: false });
    const leaveFirst = joinGroup(first, "g3");
    const second = fake(["X", "Z"], { Z: false });
    const leaveSecond = joinGroup(second, "g3");
    expect(second.dispatchAction).not.toHaveBeenCalled();
    leaveSecond();
    leaveFirst();
  });
});

describe("syncWithGroup", () => {
  it("brings a curve that appeared after the group formed in line with the other chart", () => {
    const first = fake(["X"], { X: false });
    const second = fake(["Z"]);
    const leaves = [joinGroup(first, "g6"), joinGroup(second, "g6")];
    second.dispatchAction.mockClear();
    // The second chart's legend changes: it now lists X as well, shown.
    (second as unknown as { getOption: () => unknown }).getOption = () => ({
      legend: [{ data: ["Z", "X"], selected: {} }],
    });
    syncWithGroup(second);
    expect(second.dispatchAction).toHaveBeenCalledWith({ type: "legendUnSelect", legendIndex: 0, name: "X" });
    for (const leave of leaves) leave();
  });

  it("does nothing for a chart that is alone in its group or has none", () => {
    const alone = fake(["X"]);
    syncWithGroup(alone);
    const leave = joinGroup(alone, "g7");
    syncWithGroup(alone);
    leave();
    expect(alone.dispatchAction).not.toHaveBeenCalled();
  });
});

describe("setRow", () => {
  it("hides each curve of the row on every chart of the group by that chart's own entries", () => {
    const first = fake(["zv", "mzv"]);
    const second = fake(["zv", "mzv", "3hump_ei"]);
    const leaves = [joinGroup(first, "g4"), joinGroup(second, "g4")];
    first.dispatchAction.mockClear();
    second.dispatchAction.mockClear();
    setRow(first, 0, false, ["zv", "mzv"]);
    expect(first.dispatchAction.mock.calls.map((c) => c[0].name)).toEqual(["zv", "mzv"]);
    expect(second.dispatchAction.mock.calls.map((c) => c[0].name)).toEqual(["zv", "mzv", "3hump_ei"]);
    for (const leave of leaves) leave();
  });

  it("shows the whole row on every chart of the group", () => {
    const first = fake(["X"]);
    const second = fake(["X"]);
    const leaves = [joinGroup(first, "g5"), joinGroup(second, "g5")];
    setRow(second, 0, true, ["X"]);
    expect(first.dispatchAction).toHaveBeenCalledWith({ type: "legendAllSelect", legendIndex: 0 });
    expect(second.dispatchAction).toHaveBeenCalledWith({ type: "legendAllSelect", legendIndex: 0 });
    for (const leave of leaves) leave();
  });

  it("acts on one chart that has no group", () => {
    const alone = fake(["X"]);
    setRow(alone, 0, false, ["X"]);
    expect(alone.dispatchAction).toHaveBeenCalledTimes(1);
  });
});
