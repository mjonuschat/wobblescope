import { readFileSync } from 'node:fs'
import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

/** The workspace version in `Cargo.toml`, which is also the daemon's version. */
function workspaceVersion(): string {
  const manifest = readFileSync(path.resolve(import.meta.dirname, '../Cargo.toml'), 'utf8')
  const section = manifest.split('[workspace.package]')[1] ?? ''
  const version = /^version\s*=\s*"([^"]+)"/m.exec(section)?.[1]
  if (!version) throw new Error('no [workspace.package] version in Cargo.toml')
  return version
}

/**
 * Where `/api` is proxied. Set `WOBBLESCOPE_TARGET` to a printer's daemon to iterate on the UI
 * against real data, e.g. `WOBBLESCOPE_TARGET=http://voron.local:7126 pnpm dev`. Defaults to a
 * daemon on this machine.
 */
const apiTarget = process.env.WOBBLESCOPE_TARGET ?? 'http://127.0.0.1:7126'

export default defineConfig({
  define: { __WOBBLESCOPE_VERSION__: JSON.stringify(workspaceVersion()) },
  base: './',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': path.resolve(import.meta.dirname, 'src') },
  },
  worker: { format: 'es' },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
  server: {
    proxy: {
      // `changeOrigin` rewrites `Host` to the target, which the daemon's host check requires;
      // `ws` forwards the `/api/live` socket.
      '/api': {
        target: apiTarget,
        changeOrigin: true,
        ws: true,
        // A browser sends the dev server's `Origin` on a write, and the daemon refuses an origin that
        // is not its own host or in Moonraker's `cors_domains`. The proxy presents the target's origin.
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            if (proxyReq.getHeader('origin')) proxyReq.setHeader('origin', apiTarget)
          })
        },
      },
    },
  },
})
