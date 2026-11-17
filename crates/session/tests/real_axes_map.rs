//! A real axes-map capture from the daemon, analysed through the session bridge.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use wobblescope_core::axes_map::{MapState, analyze_axes_map};
use wobblescope_session::bridge::{ChipCapture, axes_map_inputs, axes_map_params};
use wobblescope_session::fs::open_session;

const ID: &str = "test-20261117T011928Z";

fn capture() -> (wobblescope_session::meta::SessionMeta, ChipCapture) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/axes_map");
    let session = open_session(&root, ID).expect("the fixture opens");
    let meta = session.meta().clone();
    let mut captures = axes_map_inputs(&meta, |file| session.load(file)).expect("inputs");
    assert_eq!(captures.len(), 1);
    (meta, captures.remove(0))
}

#[test]
fn the_first_recording_starts_well_before_its_window() {
    // Why the window matters: the travel to the start point is in the first file.
    let (meta, ChipCapture::Ready { input, .. }) = capture() else {
        panic!("expected a ready capture");
    };
    let window = meta.segments[0].window.expect("a window");
    let lead = window.start - input.segments[0].t[0];
    assert!(lead > 2.0, "lead {lead}");
    let lead_of_y = meta.segments[1].window.expect("a window").start - input.segments[1].t[0];
    assert!(lead_of_y < 0.5, "lead {lead_of_y}");
}

#[test]
fn a_real_capture_reads_cleanly_inside_its_windows() {
    let (meta, ChipCapture::Ready { input, .. }) = capture() else {
        panic!("expected a ready capture");
    };
    let params = axes_map_params(&meta).expect("params");
    let got = analyze_axes_map(&input, &params).expect("analysis");
    assert_eq!(got.state, MapState::Complete);
    assert_eq!(got.map.as_deref(), Some("x, -z, y"));
    // Whole files gave 15.3 degrees and 68 % on X, a warning, from the travel move.
    assert!(got.axes[0].tilt_deg < 8.0, "tilt {}", got.axes[0].tilt_deg);
    assert!(got.axes[0].confidence > 0.9);
    assert_eq!(got.quality.messages, ["Detection quality: GOOD"]);
}

#[test]
fn the_same_capture_read_whole_gives_the_false_warning() {
    // What the windows fix: the travel move in the first file read as a 15.3 degree angle error.
    let (meta, ChipCapture::Ready { mut input, .. }) = capture() else {
        panic!("expected a ready capture");
    };
    input.windows = [None, None, None];
    let params = axes_map_params(&meta).expect("params");
    let got = analyze_axes_map(&input, &params).expect("analysis");
    assert!(got.axes[0].tilt_deg > 12.0, "tilt {}", got.axes[0].tilt_deg);
    assert!(
        got.quality
            .messages
            .iter()
            .any(|m| m.contains("High angle error"))
    );
}
