import * as rotateCcw from "lucide-react/icons/rotate-ccw";
import * as zoomIn from "lucide-react/icons/zoom-in";

type IconNode = [tag: string, attrs: Record<string, string>][];

// The per-icon modules carry their node list as `__iconData`, which the package types do not declare.
const nodesOf = (icon: object): IconNode => (icon as { __iconData: { node: IconNode } }).__iconData.node;

const num = (value: string | undefined): number => Number(value ?? 0);

function shapeToPath([tag, attrs]: IconNode[number]): string {
  switch (tag) {
    case "path":
      return attrs.d ?? "";
    case "line":
      return `M${attrs.x1} ${attrs.y1}L${attrs.x2} ${attrs.y2}`;
    case "polyline":
      return `M${(attrs.points ?? "")
        .trim()
        .split(/\s+/)
        .reduce((out, n, i) => out + (i === 0 ? "" : i % 2 ? " " : "L") + n, "")}`;
    case "circle": {
      const [cx, cy, r] = [num(attrs.cx), num(attrs.cy), num(attrs.r)];
      return `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
    }
    default:
      throw new Error(`Unsupported lucide shape: ${tag}`);
  }
}

/** Merge a lucide icon's shapes into one SVG path, the only form an ECharts `path://` icon takes. */
export function iconNodeToPath(node: IconNode): string {
  return node.map(shapeToPath).join("");
}

export const ZOOM_IN_ICON = `path://${iconNodeToPath(nodesOf(zoomIn))}`;
export const RESET_ZOOM_ICON = `path://${iconNodeToPath(nodesOf(rotateCcw))}`;
