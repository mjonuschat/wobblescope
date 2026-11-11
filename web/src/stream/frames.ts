/** The bytes before the samples: u16 index, u16 zero, f64 t0, u32 count. */
const HEADER_BYTES = 16

/** One decoded binary frame. */
export interface DecodedFrame {
  /** The index a `stream` message gave the chip. */
  index: number
  /** The time of the first sample, in seconds. */
  t0: number
  /** Interleaved x, y, z samples. */
  xyz: Float32Array
}

/** Decodes a binary frame, see `crates/daemon/src/stream/protocol.rs` for the layout. */
export function decodeFrame(buffer: ArrayBuffer): DecodedFrame {
  if (buffer.byteLength < HEADER_BYTES) throw new Error('frame shorter than its header')
  const view = new DataView(buffer)
  const count = view.getUint32(12, true)
  if (buffer.byteLength !== HEADER_BYTES + count * 12) throw new Error('frame length does not match its count')
  const xyz = new Float32Array(count * 3)
  for (let i = 0; i < xyz.length; i++) xyz[i] = view.getFloat32(HEADER_BYTES + i * 4, true)
  return { index: view.getUint16(0, true), t0: view.getFloat64(4, true), xyz }
}
