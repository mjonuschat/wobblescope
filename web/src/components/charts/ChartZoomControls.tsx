import type { ECharts } from "echarts/core";
import { RotateCcwIcon, ZoomInIcon } from "lucide-react";
import { type RefObject, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/**
 * The page's own buttons for a chart whose option sets up box-select zoom: "Zoom in" arms the
 * selection box, "Reset zoom" restores the full range. They sit top right, clear of the legend.
 */
export function ChartZoomControls({ chart }: { chart: RefObject<ECharts | null> }) {
  const [selecting, setSelecting] = useState(false);

  const setSelectMode = (active: boolean) => {
    chart.current?.dispatchAction({ type: "takeGlobalCursor", key: "dataZoomSelect", dataZoomSelectActive: active });
    setSelecting(active);
  };

  // A finished selection (or the wheel) zooms the chart, which ends the select mode.
  useEffect(() => {
    const instance = chart.current;
    if (!instance || !selecting) return;
    const done = () => {
      instance.dispatchAction({ type: "takeGlobalCursor", key: "dataZoomSelect", dataZoomSelectActive: false });
      setSelecting(false);
    };
    instance.on("dataZoom", done);
    return () => {
      instance.off("dataZoom", done);
    };
  }, [selecting, chart]);

  return (
    <div className="absolute top-0 right-4 flex gap-1">
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              variant="outline"
              size="icon"
              aria-label="Zoom in"
              aria-pressed={selecting}
              className={selecting ? "border-primary text-primary" : undefined}
              onClick={() => setSelectMode(!selecting)}
            />
          }
        >
          <ZoomInIcon />
        </TooltipTrigger>
        <TooltipContent side="top">Zoom in</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              type="button"
              variant="outline"
              size="icon"
              aria-label="Reset zoom"
              onClick={() => {
                setSelectMode(false);
                chart.current?.dispatchAction({ type: "restore" });
              }}
            />
          }
        >
          <RotateCcwIcon />
        </TooltipTrigger>
        <TooltipContent side="top">Reset zoom</TooltipContent>
      </Tooltip>
    </div>
  );
}
