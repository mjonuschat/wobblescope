//! Sample files: one zstd frame with a content checksum around a fixed binary payload. A file
//! holds that one frame and nothing else.
//!
//! Payload layout (all integers and floats little-endian): the 8-byte magic `WSSAMPLE`, the
//! sample file version as `u32`, the sample count `n` as `u64`, then four `f64` columns of `n`
//! values each in the order `t`, `x`, `y`, `z`.

use std::io::{self, Read};

use structured_zstd::decoding::errors::FrameDecoderError;
use structured_zstd::decoding::{ContentChecksum, FinishedFrame, FrameDecoder, StreamingDecoder};
use structured_zstd::encoding::{CompressionLevel, FrameCompressor};
use wobblescope_core::RawSamples;

use crate::SessionError;

const MAGIC: &[u8; 8] = b"WSSAMPLE";
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
const CHECKSUM_FLAG: u8 = 0x04;
/// Version written into the payload header.
pub const SAMPLE_FILE_VERSION: u32 = 1;
const HEADER_LEN: usize = 20;
const BYTES_PER_SAMPLE: usize = 32;
const CHUNK_VALUES: usize = 8192;

/// Largest sample count a file may hold: 2^23 samples, 256 MiB decoded.
pub const MAX_SAMPLES: usize = 1 << 23;

/// Largest decoder window, in bytes, the reader accepts. The zstd decoder reserves its window
/// before it delivers the first block, so a tiny hostile frame could otherwise declare a
/// window the host cannot allocate.
pub const MAX_WINDOW: u64 = 8 << 20;

/// Encodes `samples` as one sample file.
///
/// # Errors
/// [`SessionError::MismatchedColumns`] when the columns differ in length, and
/// [`SessionError::TooManySamples`] above [`MAX_SAMPLES`].
pub fn encode_samples(samples: &RawSamples) -> Result<Vec<u8>, SessionError> {
    let n = samples.t.len();
    if [&samples.x, &samples.y, &samples.z]
        .iter()
        .any(|c| c.len() != n)
    {
        return Err(SessionError::MismatchedColumns);
    }
    if n > MAX_SAMPLES {
        return Err(SessionError::TooManySamples { claimed: n as u64 });
    }
    let mut payload = Vec::with_capacity(HEADER_LEN + BYTES_PER_SAMPLE * n);
    payload.extend_from_slice(MAGIC);
    payload.extend_from_slice(&SAMPLE_FILE_VERSION.to_le_bytes());
    payload.extend_from_slice(&(n as u64).to_le_bytes());
    for column in [&samples.t, &samples.x, &samples.y, &samples.z] {
        for v in column {
            payload.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut compressor = FrameCompressor::new(CompressionLevel::Default);
    compressor.set_content_checksum(true);
    compressor.set_source(payload.as_slice());
    compressor.set_drain(Vec::new());
    compressor.compress();
    Ok(compressor
        .take_drain()
        .expect("the drain was set before compressing"))
}

/// Decodes a sample file.
///
/// The input must be exactly one zstd frame with a content checksum, and the frame must end
/// right after the last sample. The header is checked before any sample column is allocated.
///
/// # Errors
/// A [`SessionError`] for wrong magic, a newer version, too many samples, an oversized decoder
/// window, a frame without a content checksum, input that is not exactly one frame,
/// truncation, trailing data or a damaged frame. Never fewer samples than claimed.
pub fn decode_samples(bytes: &[u8]) -> Result<RawSamples, SessionError> {
    // `ContentChecksum::Verify` checks only a checksum that exists, so the one frame must carry
    // the flag (bit 2 of the header descriptor, byte 4 after the 4-byte magic), and it must be
    // the whole input: a skippable-frame prefix or a second frame would otherwise hide a
    // checksumless data frame.
    if bytes.len() >= 4 && bytes[..4] != ZSTD_MAGIC {
        return Err(SessionError::Corrupt("not a single zstd frame".to_owned()));
    }
    if bytes.len() > 4 && bytes[4] & CHECKSUM_FLAG == 0 {
        return Err(SessionError::MissingChecksum);
    }
    let mut decoder = FrameDecoder::new();
    decoder
        .set_max_window_size(MAX_WINDOW)
        .map_err(|e| SessionError::Corrupt(e.to_string()))?;
    decoder.set_content_checksum(ContentChecksum::Verify);
    let mut stream = Frames {
        decoder: StreamingDecoder::new_with_decoder(bytes, decoder),
        finished: Vec::new(),
    };

    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header).map_err(map_io)?;
    if &header[..8] != MAGIC {
        return Err(SessionError::BadMagic);
    }
    let version = u32::from_le_bytes(header[8..12].try_into().expect("4 bytes"));
    if version != SAMPLE_FILE_VERSION {
        return Err(SessionError::UnsupportedSampleVersion { found: version });
    }
    let claimed = u64::from_le_bytes(header[12..20].try_into().expect("8 bytes"));
    if claimed > MAX_SAMPLES as u64 {
        return Err(SessionError::TooManySamples { claimed });
    }
    let n = claimed as usize;

    let mut samples = RawSamples::default();
    for column in [
        &mut samples.t,
        &mut samples.x,
        &mut samples.y,
        &mut samples.z,
    ] {
        read_column(&mut stream, n, column)?;
    }
    let mut extra = [0u8; 1];
    if stream.read(&mut extra).map_err(map_io)? != 0 {
        return Err(SessionError::TrailingData);
    }
    match stream.finished.as_slice() {
        [only] if only.compressed_size == bytes.len() as u64 => Ok(samples),
        _ => Err(SessionError::Corrupt("not a single zstd frame".to_owned())),
    }
}

/// A streaming decoder that remembers every frame it finishes, so the reader can require that
/// the input is exactly one frame.
struct Frames<'a> {
    decoder: StreamingDecoder<&'a [u8], FrameDecoder>,
    finished: Vec<FinishedFrame>,
}

impl Read for Frames<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.decoder
            .read_reporting_frames(buf, |frame| self.finished.push(frame))
    }
}

/// Reads `n` little-endian `f64` values, growing `out` only as data actually arrives.
fn read_column(stream: &mut impl Read, n: usize, out: &mut Vec<f64>) -> Result<(), SessionError> {
    let mut buf = vec![0u8; CHUNK_VALUES * 8];
    let mut remaining = n;
    while remaining > 0 {
        let take = remaining.min(CHUNK_VALUES);
        stream.read_exact(&mut buf[..take * 8]).map_err(map_io)?;
        let (values, _) = buf[..take * 8].as_chunks::<8>();
        out.extend(values.iter().map(|c| f64::from_le_bytes(*c)));
        remaining -= take;
    }
    Ok(())
}

fn map_io(e: io::Error) -> SessionError {
    if let Some(FrameDecoderError::WindowSizeTooBig {
        requested, limit, ..
    }) = e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<FrameDecoderError>())
    {
        return SessionError::WindowTooLarge {
            requested: *requested,
            limit: *limit,
        };
    }
    if e.kind() == io::ErrorKind::UnexpectedEof {
        return SessionError::Truncated;
    }
    SessionError::Corrupt(e.to_string())
}
