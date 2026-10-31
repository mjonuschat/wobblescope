//! Shared helpers for the session suites. Fixtures are the real captures that
//! `wobblescope-core`'s tests already embed, so the same tests run natively and on wasm32.
#![allow(dead_code)]

use wobblescope_core::RawSamples;

/// Runs the wrapped test on both native and wasm32.
macro_rules! parity_test {
    ($(#[$m:meta])* fn $name:ident() $body:block) => {
        $(#[$m])*
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        fn $name() $body
    };
}

/// Logs a note that is informational only.
pub fn log(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{msg}");
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::console_log!("{}", msg);
}

/// Decompresses an embedded zstd file with the pure-Rust decoder.
pub fn unzstd(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut dec = structured_zstd::decoding::StreamingDecoder::new(bytes);
    let mut out = String::new();
    dec.read_to_string(&mut out).unwrap();
    out
}

/// Independent reference parser for Klipper raw CSV text: split on commas, parse with
/// `str::parse`. It deliberately shares no code with the importer it checks.
pub fn parse_csv(text: &str) -> RawSamples {
    let mut s = RawSamples::default();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let v: Vec<f64> = line.split(',').map(|f| f.parse().unwrap()).collect();
        s.t.push(v[0]);
        s.x.push(v[1]);
        s.y.push(v[2]);
        s.z.push(v[3]);
    }
    s
}

/// Text of a real capture stored by `wobblescope-core`'s tests as `<name>.csv.zst`.
pub fn real_text(name: &str) -> String {
    let bytes: &[u8] = match name {
        "ws_x_adxl345" => include_bytes!("../../../core/tests/data/real/ws_x_adxl345.csv.zst"),
        "ws_x_ebb36" => include_bytes!("../../../core/tests/data/real/ws_x_ebb36.csv.zst"),
        _ => panic!("unknown real capture {name}"),
    };
    unzstd(bytes)
}

/// A real capture, parsed with the reference parser.
pub fn real(name: &str) -> RawSamples {
    parse_csv(&real_text(name))
}
