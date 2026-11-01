import {
  Link,
  Outlet,
  createRootRoute,
  createRoute,
} from '@tanstack/react-router'

import { SessionsPage } from '@/pages/sessions/SessionsPage'

const rootRoute = createRootRoute({
  component: () => (
    <div className="min-h-screen bg-neutral-950 text-neutral-100">
      <nav className="flex gap-4 border-b border-neutral-800 px-6 py-3">
        <Link to="/">Run test</Link>
        <Link to="/sessions">Sessions</Link>
      </nav>
      <main className="p-6">
        <Outlet />
      </main>
    </div>
  ),
})

const runRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: () => <h1 className="text-xl font-semibold">Run test</h1>,
})

const sessionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/sessions',
  component: SessionsPage,
})

const sessionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/sessions/$id',
  component: () => <h1 className="text-xl font-semibold">Session</h1>,
})

export const routeTree = rootRoute.addChildren([runRoute, sessionsRoute, sessionRoute])
