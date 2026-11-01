import init, { WasmSession, config_snippet } from '../wasm/wobblescope_wasm.js'

import { createHandler } from './handler'
import type { WorkerRequest } from './protocol'

const handle = createHandler(async () => {
  await init()
  return { WasmSession, config_snippet }
})

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const response = await handle(event.data)
  if (response) self.postMessage(response)
}
