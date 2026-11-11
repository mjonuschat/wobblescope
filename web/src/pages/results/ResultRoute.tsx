import { getRouteApi } from "@tanstack/react-router";

import { ResultPage } from "./ResultPage";

const route = getRouteApi("/results/$rid");

export function ResultRoute() {
  const { rid } = route.useParams();
  return <ResultPage rid={rid} />;
}
