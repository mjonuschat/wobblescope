#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use wobblescope_core::axes_map::{
    AxisState, ChipAxis, MapState, NoiseVerdict, QualityStatus, analyze_axes_map,
    analyze_axes_map_as_reference,
};
use wobblescope_session::bridge::{ChipCapture, axes_map_inputs, axes_map_params};
use wobblescope_session::fs::open_session;

const ID: &str = "test-20261117T011928Z";
const BEACON_ID: &str = "test-20261117T034234Z";
const HANK_IDS: [&str; 2] = ["axesmap-20261125T034718Z", "axesmap-20261125T034912Z"];

fn capture() -> (wobblescope_session::meta::SessionMeta, ChipCapture) {
    capture_of(ID)
}

fn capture_of(id: &str) -> (wobblescope_session::meta::SessionMeta, ChipCapture) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/axes_map");
    let session = open_session(&root, id).expect("the fixture opens");
    let meta = session.meta().clone();
    let mut captures = axes_map_inputs(&meta, |file| session.load(file)).expect("inputs");
    assert_eq!(captures.len(), 1);
    (meta, captures.remove(0))
}

#[test]
fn the_first_recording_starts_well_before_its_window() {
    // The window matters because the first file holds the travel to the start point.
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
    // Without the windows, the travel move in the first file reads as a 15.3 degree angle error.
    let (meta, ChipCapture::Ready { mut input, .. }) = capture() else {
        panic!("expected a ready capture");
    };
    input.windows = [None, None, None];
    let params = axes_map_params(&meta).expect("params");
    let got = analyze_axes_map_as_reference(&input, &params).expect("analysis");
    assert!(got.axes[0].tilt_deg > 12.0, "tilt {}", got.axes[0].tilt_deg);
    assert!(
        got.quality
            .messages
            .iter()
            .any(|m| m.contains("High angle error"))
    );
}

#[test]
fn a_real_beacon_capture_infers_the_z_axis_it_cannot_feel() {
    // On a printer whose bed moves Z, the toolhead's Beacon sees a 10 mm/s wiggle on chip x during
    // the Z move, at 34 % confidence. It matches the chip axis the X move already has.
    let (meta, ChipCapture::Ready { input, .. }) = capture_of(BEACON_ID) else {
        panic!("expected a ready capture");
    };
    let params = axes_map_params(&meta).expect("params");
    let got = analyze_axes_map(&input, &params).expect("analysis");
    assert_eq!(got.state, MapState::Complete);
    assert_eq!(got.map.as_deref(), Some("-x, -y, z"));
    assert_eq!(got.axes[2].state, AxisState::Virtual);
    assert_ne!(got.quality.status, QualityStatus::Error);
}

#[test]
fn a_step_in_the_chip_reading_does_not_hide_the_y_move_on_hank() {
    // The chip reads 74 to 148 mm/s² differently at rest after the Y move than before it. One
    // baseline per segment turned that into -43 and +49 mm/s of the 80 mm/s on chip z in the two
    // runs, and a confidence of 2 and 15 %.
    for id in HANK_IDS {
        let (meta, ChipCapture::Ready { input, .. }) = capture_of(id) else {
            panic!("expected a ready capture");
        };
        let params = axes_map_params(&meta).expect("params");
        let got = analyze_axes_map(&input, &params).expect("analysis");
        let (x, y) = (&got.axes[0], &got.axes[1]);
        assert_eq!(x.chip_axis, Some(ChipAxis::Y), "{id}");
        assert_eq!(y.chip_axis, Some(ChipAxis::Z), "{id}");
        assert!(!x.negative && !y.negative, "{id}");
        assert!(y.peaks[2] > 70.0, "{id}: peak {}", y.peaks[2]);
        assert!(x.confidence > 0.9 && y.confidence > 0.9, "{id}");
        assert!(x.tilt_deg < 15.0 && y.tilt_deg < 15.0, "{id}");
        let reference = analyze_axes_map_as_reference(&input, &params).expect("analysis");
        assert!(reference.axes[1].confidence < 0.2, "{id}");
    }
}

#[test]
fn gravity_gives_hanks_z_when_its_move_is_too_weak_to_read() {
    // The Z move is 15 mm/s at 500 mm/s², under the baseline error, and read on its own it
    // takes chip z, the axis the Y move has, or chip x at 7 % confidence. Chip x reads +1 g at
    // rest, so it is up.
    for id in HANK_IDS {
        let (meta, ChipCapture::Ready { input, .. }) = capture_of(id) else {
            panic!("expected a ready capture");
        };
        let params = axes_map_params(&meta).expect("params");
        let got = analyze_axes_map(&input, &params).expect("analysis");
        assert_eq!(got.state, MapState::Complete, "{id}");
        assert_eq!(got.map.as_deref(), Some("y, z, x"), "{id}");
        assert_eq!(got.axes[2].state, AxisState::Virtual, "{id}");
        assert_ne!(got.quality.status, QualityStatus::Error, "{id}");
    }
}

#[test]
fn gravity_agrees_with_the_z_axis_of_the_other_real_captures() {
    // buster reads Z from its other two axes and uku's Beacon cannot feel it; gravity gives the
    // same chip axis and sign, so their maps do not change.
    let (meta, ChipCapture::Ready { input, .. }) = capture() else {
        panic!("expected a ready capture");
    };
    let params = axes_map_params(&meta).expect("params");
    let got = analyze_axes_map(&input, &params).expect("analysis");
    let reference = analyze_axes_map_as_reference(&input, &params).expect("analysis");
    assert_eq!(got.map, reference.map);
    let (meta, ChipCapture::Ready { input, .. }) = capture_of(BEACON_ID) else {
        panic!("expected a ready capture");
    };
    let params = axes_map_params(&meta).expect("params");
    let got = analyze_axes_map(&input, &params).expect("analysis");
    let reference = analyze_axes_map_as_reference(&input, &params).expect("analysis");
    assert_eq!(got.map, reference.map);
}

#[test]
fn the_noise_verdict_agrees_with_the_quality_check_on_hank() {
    // Noise of 355 to 389 mm/s² at 1500 mm/s² is under the 450 the quality check warns at.
    for id in HANK_IDS {
        let (meta, ChipCapture::Ready { input, .. }) = capture_of(id) else {
            panic!("expected a ready capture");
        };
        let params = axes_map_params(&meta).expect("params");
        let got = analyze_axes_map(&input, &params).expect("analysis");
        assert_eq!(got.noise_verdict, NoiseVerdict::Fine, "{id}: {}", got.noise);
        assert_eq!(got.quality.status, QualityStatus::Ok, "{id}");
        let reference = analyze_axes_map_as_reference(&input, &params).expect("analysis");
        assert_eq!(reference.noise_verdict, NoiseVerdict::High, "{id}");
    }
}
