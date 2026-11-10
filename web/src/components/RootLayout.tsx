import { Link, Outlet } from "@tanstack/react-router";

import { StatusBanner } from "@/components/StatusBanner";
import { useRecoverStorage } from "@/lib/api/queries";

/** The frame of every page: the navigation, the status banner and the page itself. */
export function RootLayout() {
  useRecoverStorage();
  return (
    <div className="min-h-screen bg-neutral-950 text-neutral-100">
      <nav className="flex gap-4 border-b border-neutral-800 px-6 py-3">
        <Link to="/">Run test</Link>
        <Link to="/sessions">Sessions</Link>
      </nav>
      <StatusBanner />
      <main className="p-6">
        <Outlet />
      </main>
    </div>
  );
}
