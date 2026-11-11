import init, { WasmLiveSpectrum } from '../wasm/wobblescope_wasm.js'

import { createSpectrumHandler, type SpectrumModule, type SpectrumRequest } from './spectrumHandler'

const handle = createSpectrumHandler(async () => {
  await init()
  return { WasmLiveSpectrum } as unknown as SpectrumModule
}, () => performance.now())

self.onmessage = async (event: MessageEvent<SpectrumRequest>) => {
  const response = await handle(event.data)
  if (response) self.postMessage(response)
}
