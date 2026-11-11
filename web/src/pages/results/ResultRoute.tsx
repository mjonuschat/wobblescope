import { getRouteApi } from '@tanstack/react-router'

import { ResultPage } from './ResultPage'

const route = getRouteApi('/results/$rid')

/** Reads the result id from the URL for the page. */
export function ResultRoute() {
  const { rid } = route.useParams()
  return <ResultPage rid={rid} />
}
