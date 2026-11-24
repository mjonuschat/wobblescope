import type { ECharts } from "echarts/core";

interface LegendState {
  data?: unknown[];
  selected?: Record<string, boolean>;
}

/** The charts of each group, so a legend change can reach all of them. */
const members = new Map<string, Set<ECharts>>();

function legendsOf(chart: ECharts): LegendState[] {
  return [(chart.getOption() as { legend?: unknown } | undefined)?.legend].flat().filter(Boolean) as LegendState[];
}

export function namesOf(legend: LegendState | undefined): string[] {
  return (legend?.data ?? []).flatMap((d) => {
    const name = typeof d === "string" ? d : (d as { name?: unknown } | null)?.name;
    return typeof name === "string" ? [name] : [];
  });
}

/** Makes `to` show and hide the curves that `from` shows and hides, for the curves they share. */
function copySelection(from: ECharts, to: ECharts) {
  const held = legendsOf(from);
  legendsOf(to).forEach((legend, index) => {
    const listed = new Set(namesOf(held[index]));
    for (const name of namesOf(legend)) {
      if (!listed.has(name)) continue;
      const want = held[index]?.selected?.[name] !== false;
      const have = legend.selected?.[name] !== false;
      if (want !== have)
        to.dispatchAction({ type: want ? "legendSelect" : "legendUnSelect", legendIndex: index, name });
    }
  });
}

/**
 * Adds `chart` to `group`. It first takes over what another chart of the group shows, because a
 * legend click is forwarded between charts as a toggle, and two charts that start out different
 * would stay opposite.
 */
export function joinGroup(chart: ECharts, group: string): () => void {
  const set = members.get(group) ?? new Set<ECharts>();
  const peer = [...set][0];
  if (peer) copySelection(peer, chart);
  chart.group = group;
  set.add(chart);
  members.set(group, set);
  return () => {
    set.delete(chart);
    chart.group = "";
    if (set.size === 0) members.delete(group);
  };
}

/**
 * Brings the shared curves of `chart` in line with another chart of its group, for when its legend
 * has changed. The linked toggles would otherwise leave the two opposite.
 */
export function syncWithGroup(chart: ECharts) {
  const peer = chart.group ? [...(members.get(chart.group) ?? [])].find((c) => c !== chart) : undefined;
  if (peer) copySelection(peer, chart);
}

/**
 * Shows or hides every curve of legend row `index` on `chart` and on the charts of its group, each
 * by its own legend entries, since the charts of a group can list different curves.
 * `ownNames` are the entries of `chart` itself.
 */
export function setRow(chart: ECharts, index: number, show: boolean, ownNames: string[]) {
  const peers = chart.group ? [...(members.get(chart.group) ?? [])].filter((c) => c !== chart) : [];
  const targets: [ECharts, string[]][] = [
    [chart, ownNames],
    ...peers.map((c): [ECharts, string[]] => [c, namesOf(legendsOf(c)[index])]),
  ];
  for (const [target, names] of targets) {
    if (show) target.dispatchAction({ type: "legendAllSelect", legendIndex: index });
    else for (const name of names) target.dispatchAction({ type: "legendUnSelect", legendIndex: index, name });
  }
}
