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
      '/api': 'http://127.0.0.1:7126',
    },
  },
})
