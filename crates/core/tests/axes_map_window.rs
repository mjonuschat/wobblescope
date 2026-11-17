//! The axes-map analysis looks only at each segment's recorded window.
//!
//! A real capture's first file starts before the setup and the travel to the start point, so it
//! holds that move as well as the measured one. The travel runs faster than the measured move and
//! on several axes, so left in it spoils the first segment's direction.

use wobblescope_core::RawSamples;
use wobblescope_core::axes_map::{
    AxesMapInput, AxesMapParams, MapState, Mounting, analyze_axes_map,
};

const RATE_HZ: f64 = 1600.0;
const GRAVITY: f64 = 9810.0;
/// Acceleration and duration of each half of a pulse. A pulse up and down makes a triangle of
/// velocity with a peak of `PULSE_ACCEL * PULSE_SECONDS` (80 mm/s).
const PULSE_ACCEL: f64 = 1600.0;
const PULSE_SECONDS: f64 = 0.05;
const WINDOW: std::ops::Range<f64> = 2.0..3.7;

fn recording(start: f64, axis: usize, extra: &[(f64, [f64; 3])]) -> RawSamples {
    let mut s = RawSamples::default();
    let count = ((3.8 - start) * RATE_HZ) as usize;
    for i in 0..count {
        let t = start + i as f64 / RATE_HZ;
        let mut a = [0.0, 0.0, GRAVITY];
        for (k, v) in a.iter_mut().enumerate() {
            *v += 3.0 * (t * 50.0 + k as f64).sin();
        }
        let mut pulse = |at: f64, scale: [f64; 3]| {
            let sign = if (at..at + PULSE_SECONDS).contains(&t) {
                1.0
            } else if (at + PULSE_SECONDS..at + 2.0 * PULSE_SECONDS).contains(&t) {
                -1.0
            } else {
                0.0
            };
            for (v, k) in a.iter_mut().zip(scale) {
                *v += sign * PULSE_ACCEL * k;
            }
        };
        let mut own = [0.0; 3];
        own[axis] = 1.0;
        pulse(2.5, own);
        for (at, scale) in extra {
            pulse(*at, *scale);
        }
        s.t.push(t);
        s.x.push(a[0]);
        s.y.push(a[1]);
        s.z.push(a[2]);
    }
    s
}

/// The three moves. The first recording starts at 0 s, before its window, and holds a faster
/// diagonal travel move on x and y at 0.5 s. The other two start 0.1 s before their windows.
fn input(windows: [Option<std::ops::Range<f64>>; 3]) -> AxesMapInput {
    AxesMapInput {
        chip: "adxl345".to_owned(),
        segments: [
            recording(0.0, 0, &[(0.5, [2.0, 2.0, 0.0])]),
            recording(1.9, 1, &[]),
            recording(1.9, 2, &[]),
        ],
        windows,
        configured_map: None,
        mounting: Mounting::Toolhead,
    }
}

fn params() -> AxesMapParams {
    AxesMapParams {
        speed: 80.0,
        accel: 1600.0,
        max_z_velocity: None,
        max_z_accel: None,
    }
}

#[test]
fn without_a_window_the_travel_move_spoils_the_first_segment() {
    let got = analyze_axes_map(&input([None, None, None]), &params()).unwrap();
    // The travel move is faster than the measured one and diagonal, so x and y peak alike.
    assert!(got.axes[0].tilt_deg > 20.0, "tilt {}", got.axes[0].tilt_deg);
    assert!(got.axes[0].confidence < 0.5);
}

#[test]
fn a_window_leaves_out_the_travel_move() {
    let windows = [Some(WINDOW), Some(WINDOW), Some(WINDOW)];
    let got = analyze_axes_map(&input(windows), &params()).unwrap();
    assert_eq!(got.state, MapState::Complete);
    assert_eq!(got.map.as_deref(), Some("x, y, z"));
    assert!(got.axes[0].tilt_deg < 2.0, "tilt {}", got.axes[0].tilt_deg);
    assert!(got.axes[0].confidence > 0.99);
    let peak = got.axes[0].peak_velocity;
    // The low-pass filter trims a pulse this short. The travel move's 160 mm/s is not counted.
    assert!((55.0..90.0).contains(&peak), "peak {peak}");
    let curve = &got.curves[0].t;
    assert!(curve[curve.len() - 1] <= WINDOW.end - WINDOW.start + 0.01);
}

#[test]
fn a_window_outside_the_recording_is_an_error_not_a_silent_fallback() {
    let windows = [Some(10.0..11.0), None, None];
    assert!(analyze_axes_map(&input(windows), &params()).is_err());
}
