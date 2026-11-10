import { getRouteApi } from "@tanstack/react-router";

import { SessionPage } from "./SessionPage";

const route = getRouteApi("/sessions/$id");

/** Reads the session id from the URL for the page. */
export function SessionRoute() {
  const { id } = route.useParams();
  return <SessionPage id={id} />;
}
