// Builds crates/wasm and runs the wasm-bindgen CLI over it, writing the browser module to
// web/src/wasm/. The CLI must match the wasm-bindgen crate version locked in Cargo.lock,
// because the generated glue and the compiled module only fit each other at one version.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repoDir = path.resolve(webDir, '..')
const outDir = path.join(webDir, 'src', 'wasm')
const target = 'wasm32-unknown-unknown'

function fail(message) {
  console.error(`build-wasm: ${message}`)
  process.exit(1)
}

function run(command, args, options = {}) {
  try {
    return execFileSync(command, args, {
      cwd: repoDir,
      env: { ...process.env, ...options.env },
      encoding: 'utf8',
      stdio: options.inherit ? 'inherit' : ['ignore', 'pipe', 'pipe'],
    })
  } catch (error) {
    if (error.code === 'ENOENT') return null
    if (options.inherit) process.exit(error.status ?? 1)
    throw error
  }
}

function lockedVersion() {
  const lock = readFileSync(path.join(repoDir, 'Cargo.lock'), 'utf8')
  const match = lock.match(/\[\[package\]\]\s+name = "wasm-bindgen"\s+version = "([^"]+)"/)
  if (!match) fail('Cargo.lock has no wasm-bindgen package')
  return match[1]
}

const wanted = lockedVersion()
const cliOutput = run('wasm-bindgen', ['--version'])
if (cliOutput === null) {
  fail(`the wasm-bindgen CLI is not on PATH. Install version ${wanted}: cargo install wasm-bindgen-cli --version ${wanted}`)
}
const found = cliOutput.trim().split(/\s+/)[1]
if (found !== wanted) {
  fail(`the wasm-bindgen CLI is ${found} but Cargo.lock pins ${wanted}. Install the matching version: cargo install wasm-bindgen-cli --version ${wanted} --force`)
}

const installed = run('rustup', ['target', 'list', '--installed'])
if (installed !== null && !installed.split('\n').includes(target)) {
  fail(`the ${target} Rust target is missing. Install it: rustup target add ${target}`)
}

run('cargo', ['build', '-p', 'wobblescope-wasm', '--target', target, '--release', '--locked'], {
  inherit: true,
  // The release profile strips everything, which also drops the `target_features` section
  // wasm-bindgen reads. Keep that section; only debug info goes.
  env: { CARGO_PROFILE_RELEASE_STRIP: 'debuginfo' },
})

const metadata = JSON.parse(run('cargo', ['metadata', '--format-version', '1', '--no-deps']))
const wasm = path.join(metadata.target_directory, target, 'release', 'wobblescope_wasm.wasm')
run('wasm-bindgen', ['--target', 'web', '--out-dir', outDir, '--out-name', 'wobblescope_wasm', wasm], { inherit: true })
