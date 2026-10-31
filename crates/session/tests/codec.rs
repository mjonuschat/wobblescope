#[macro_use]
mod common;

use common::{log, parse_csv, real};
use structured_zstd::encoding::{CompressionLevel, FrameCompressor};
use wobblescope_core::RawSamples;
use wobblescope_session::SessionError;
use wobblescope_session::codec::{MAX_SAMPLES, decode_samples, encode_samples};

/// zstd-compresses `payload` as one frame with a content checksum, like the real encoder.
fn frame(payload: &[u8]) -> Vec<u8> {
    let mut fc = FrameCompressor::new(CompressionLevel::Default);
    fc.set_content_checksum(true);
    fc.set_source(payload);
    fc.set_drain(Vec::new());
    fc.compress();
    fc.take_drain().unwrap()
}

/// A payload header: magic, version, claimed sample count.
fn header(magic: &[u8; 8], version: u32, n: u64) -> Vec<u8> {
    let mut p = magic.to_vec();
    p.extend_from_slice(&version.to_le_bytes());
    p.extend_from_slice(&n.to_le_bytes());
    p
}

fn payload_of(s: &RawSamples) -> Vec<u8> {
    let mut p = header(b"WSSAMPLE", 1, s.len() as u64);
    for col in [&s.t, &s.x, &s.y, &s.z] {
        for v in col.iter() {
            p.extend_from_slice(&v.to_le_bytes());
        }
    }
    p
}

fn ramp(n: usize) -> RawSamples {
    let mut s = RawSamples::default();
    for i in 0..n {
        s.t.push(41230.322878 + i as f64 * 0.000313);
        s.x.push((i as f64 * 0.37).sin() * 300.0);
        s.y.push((i as f64 * 0.11).cos() * 250.0);
        s.z.push(-9806.022374 + (i % 7) as f64);
    }
    s
}

parity_test! {
    fn six_decimal_klipper_values_round_trip_bit_exactly() {
        let s = parse_csv(
            "#time,accel_x,accel_y,accel_z\n\
             41230.322878,148.041188,296.082377,-9806.022374\n\
             41230.323190,222.061783,0.000000,-9729.412825\n",
        );
        let back = decode_samples(&encode_samples(&s).unwrap()).unwrap();
        for (a, b) in [(&s.t, &back.t), (&s.x, &back.x), (&s.y, &back.y), (&s.z, &back.z)] {
            assert_eq!(a.len(), b.len());
            for (u, v) in a.iter().zip(b) {
                assert_eq!(u.to_bits(), v.to_bits());
            }
        }
        assert_eq!(back.z[0], -9806.022374);
    }
}

parity_test! {
    fn real_capture_round_trips_equal() {
        let s = real("ws_x_adxl345");
        let bytes = encode_samples(&s).unwrap();
        let raw = 20 + 32 * s.len();
        log(&format!(
            "ws_x_adxl345: {} samples, {raw} payload bytes -> {} compressed ({:.2}x)",
            s.len(),
            bytes.len(),
            raw as f64 / bytes.len() as f64
        ));
        assert_eq!(decode_samples(&bytes).unwrap(), s);
    }
}

parity_test! {
    fn zero_samples_round_trip_to_empty() {
        let back = decode_samples(&encode_samples(&RawSamples::default()).unwrap()).unwrap();
        assert!(back.is_empty());
    }
}

parity_test! {
    fn unequal_column_lengths_fail_to_encode() {
        let mut s = ramp(10);
        s.x.pop();
        assert!(matches!(encode_samples(&s), Err(SessionError::MismatchedColumns)));
    }
}

parity_test! {
    fn wrong_magic_is_rejected() {
        let mut p = payload_of(&ramp(4));
        p[7] = b'X';
        assert!(matches!(decode_samples(&frame(&p)), Err(SessionError::BadMagic)));
    }
}

parity_test! {
    fn newer_sample_version_is_rejected_with_the_version() {
        let mut p = payload_of(&ramp(4));
        p[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert!(matches!(
            decode_samples(&frame(&p)),
            Err(SessionError::UnsupportedSampleVersion { found: 2 })
        ));
    }
}

parity_test! {
    fn absurd_claimed_counts_are_rejected_before_any_column_is_read() {
        for n in [u64::MAX, MAX_SAMPLES as u64 + 1] {
            let p = header(b"WSSAMPLE", 1, n);
            assert!(
                matches!(decode_samples(&frame(&p)), Err(SessionError::TooManySamples { claimed }) if claimed == n),
                "n = {n}"
            );
        }
    }
}

parity_test! {
    fn truncated_files_are_errors_never_ok() {
        let bytes = encode_samples(&ramp(1000)).unwrap();
        for cut in [0, 10, bytes.len() / 2, bytes.len() - 1] {
            assert!(decode_samples(&bytes[..cut]).is_err(), "cut at {cut}");
        }
    }
}

parity_test! {
    fn a_flipped_byte_is_an_error() {
        let mut bytes = encode_samples(&ramp(1000)).unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        assert!(decode_samples(&bytes).is_err());
    }
}

parity_test! {
    fn bytes_after_the_last_column_are_trailing_data() {
        let mut p = payload_of(&ramp(10));
        p.extend_from_slice(&[0u8; 8]);
        assert!(matches!(decode_samples(&frame(&p)), Err(SessionError::TrailingData)));
    }
}

parity_test! {
    fn a_payload_one_sample_short_is_truncated() {
        let mut p = payload_of(&ramp(1000));
        p.truncate(p.len() - 8);
        assert!(matches!(decode_samples(&frame(&p)), Err(SessionError::Truncated)));
    }
}

parity_test! {
    fn csv_text_is_not_a_sample_file() {
        assert!(decode_samples(b"#time,accel_x,accel_y,accel_z\n1.0,2.0,3.0,4.0\n").is_err());
    }
}

parity_test! {
    fn a_frame_after_the_frame_is_trailing_data() {
        let mut bytes = encode_samples(&ramp(10)).unwrap();
        bytes.extend_from_slice(&encode_samples(&ramp(10)).unwrap());
        assert!(decode_samples(&bytes).is_err());
    }
}

parity_test! {
    fn a_frame_declaring_a_huge_window_is_rejected_before_decoding() {
        // magic, descriptor (checksum flag, not single-segment), window descriptor for 128 MiB,
        // one empty last raw block, and a zero checksum.
        let hostile = [
            0x28, 0xB5, 0x2F, 0xFD, 0x04, 0x88, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(matches!(
            decode_samples(&hostile),
            Err(SessionError::WindowTooLarge { requested, .. }) if requested == 128 << 20
        ));
    }
}

parity_test! {
    fn a_damaged_checksum_trailer_is_an_error() {
        let mut bytes = encode_samples(&ramp(1000)).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert!(matches!(decode_samples(&bytes), Err(SessionError::Corrupt(_))));
    }
}

parity_test! {
    fn a_frame_without_a_content_checksum_is_rejected() {
        let mut fc = FrameCompressor::new(CompressionLevel::Default);
        fc.set_content_checksum(false);
        let payload = payload_of(&ramp(100));
        fc.set_source(payload.as_slice());
        fc.set_drain(Vec::new());
        fc.compress();
        let unchecked = fc.take_drain().unwrap();
        assert!(matches!(decode_samples(&unchecked), Err(SessionError::MissingChecksum)));
    }
}

/// Compresses `payload` as one frame, with or without the content checksum.
fn frame_with(payload: &[u8], checksum: bool) -> Vec<u8> {
    let mut fc = FrameCompressor::new(CompressionLevel::Default);
    fc.set_content_checksum(checksum);
    fc.set_source(payload);
    fc.set_drain(Vec::new());
    fc.compress();
    fc.take_drain().unwrap()
}

parity_test! {
    fn a_skippable_frame_prefix_cannot_hide_a_missing_checksum() {
        let mut bytes = vec![0x50, 0x2A, 0x4D, 0x18, 0, 0, 0, 0];
        bytes.extend(frame_with(&payload_of(&ramp(100)), false));
        assert!(decode_samples(&bytes).is_err());
    }
}

parity_test! {
    fn an_appended_empty_frame_is_rejected() {
        let mut bytes = encode_samples(&ramp(10)).unwrap();
        bytes.extend(frame_with(&[], true));
        assert!(decode_samples(&bytes).is_err());
    }
}

parity_test! {
    fn columns_in_a_second_unchecked_frame_are_rejected() {
        let payload = payload_of(&ramp(100));
        let mut bytes = frame_with(&payload[..20], true);
        bytes.extend(frame_with(&payload[20..], false));
        assert!(decode_samples(&bytes).is_err());
    }
}
