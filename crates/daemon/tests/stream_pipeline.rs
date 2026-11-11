use std::time::{Duration, Instant};

use wobblescope::moonraker::bridge::Batch;
use wobblescope::moonraker::chips::{Availability, Chip, ChipType};
use wobblescope::stream::pipeline::{ChipPipeline, Output};
use wobblescope::stream::protocol::{ServerMessage, StreamLimits};
use wobblescope_core::RawSamples;

fn chip(rate: f64) -> Chip {
    Chip {
        section: "adxl345".to_owned(),
        chip_type: ChipType::Adxl345,
        sensor: Some("adxl345".to_owned()),
        dump_endpoint: "adxl345/dump_adxl345".to_owned(),
        nominal_rate_hz: Some(rate),
        mcu: None,
        axes_map: None,
        availability: Availability::Unchecked,
    }
}

fn limits() -> StreamLimits {
    StreamLimits {
        max_updates_per_second: 20.0,
        max_hz: 1600.0,
        max_chips: 4,
    }
}

fn batch(t0: f64, rate: f64, n: usize) -> Batch {
    let t: Vec<f64> = (0..n).map(|i| t0 + i as f64 / rate).collect();
    let x: Vec<f64> = t.iter().map(|t| (t * 50.0).sin()).collect();
    Batch {
        samples: RawSamples {
            y: x.clone(),
            z: x.clone(),
            x,
            t,
        },
        errors: None,
        overflows: None,
    }
}

/// Feeds `seconds` of input in batches of `n`, with `now` following the sample clock.
struct Feed {
    pipe: ChipPipeline,
    base: Instant,
    t: f64,
    clock: f64,
    out: Vec<Output>,
}

impl Feed {
    fn new(pipe: ChipPipeline) -> Self {
        Self {
            pipe,
            base: Instant::now(),
            t: 100.0,
            clock: 0.0,
            out: Vec::new(),
        }
    }

    fn run(&mut self, rate: f64, seconds: f64, n: usize) {
        let batches = (seconds * rate / n as f64).round() as usize;
        for _ in 0..batches {
            let now = self.base + Duration::from_secs_f64(self.clock);
            let out = self.pipe.push(&batch(self.t, rate, n), now);
            self.out.extend(out);
            self.t += n as f64 / rate;
            self.clock += n as f64 / rate;
        }
    }

    fn streams(&self) -> Vec<&ServerMessage> {
        self.out
            .iter()
            .filter_map(|o| match o {
                Output::Message(m @ ServerMessage::Stream { .. }) => Some(m),
                _ => None,
            })
            .collect()
    }

    fn healths(&self) -> Vec<Vec<String>> {
        self.out
            .iter()
            .filter_map(|o| match o {
                Output::Message(ServerMessage::Health { warnings, .. }) => Some(warnings.clone()),
                _ => None,
            })
            .collect()
    }

    /// (t0, sample count) of each frame.
    fn frames(&self) -> Vec<(f64, usize)> {
        self.out
            .iter()
            .filter_map(|o| match o {
                Output::Frame(b) => Some((
                    f64::from_le_bytes(b[4..12].try_into().unwrap()),
                    u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize,
                )),
                _ => None,
            })
            .collect()
    }
}

fn feed(rate: f64, max_hz: f64, updates: f64) -> Feed {
    Feed::new(ChipPipeline::new(
        &chip(rate),
        0,
        max_hz,
        updates,
        &limits(),
    ))
}

#[test]
fn full_range_on_a_3200_hz_chip_passes_samples_through_up_to_1600_hz() {
    let mut f = feed(3200.0, 1600.0, 10.0);
    f.run(3200.0, 1.0, 320);
    let streams = f.streams();
    assert_eq!(streams.len(), 1);
    let ServerMessage::Stream {
        factor, rate_hz, ..
    } = streams[0]
    else {
        unreachable!()
    };
    assert_eq!(*factor, 1);
    assert!((rate_hz - 3200.0).abs() < 1.0);
    let total: usize = f.frames().iter().map(|f| f.1).sum();
    assert!(total >= 3200 - 320, "{total}");
}

#[test]
fn a_3200_hz_chip_gets_a_factor_of_six_and_a_533_hz_stream() {
    let mut f = feed(3200.0, 200.0, 10.0);
    f.run(3200.0, 1.0, 320);
    f.out.extend(f.pipe.tick(f.base + Duration::from_secs(2)));
    let streams = f.streams();
    assert_eq!(streams.len(), 1);
    let ServerMessage::Stream {
        factor, rate_hz, ..
    } = streams[0]
    else {
        unreachable!()
    };
    assert_eq!(*factor, 6);
    assert!((rate_hz - 533.3).abs() < 1.0);
    let total: usize = f.frames().iter().map(|f| f.1).sum();
    assert!((total as f64 - 3200.0 / 6.0).abs() <= 1.0, "{total}");
}

#[test]
fn an_old_firmware_lis2dw_at_395_hz_is_flagged_and_filtered_from_its_real_rate() {
    let mut f = feed(1600.0, 100.0, 10.0);
    f.run(395.0, 2.5, 40);
    let streams = f.streams();
    let ServerMessage::Stream {
        input_rate_hz,
        factor,
        ..
    } = streams[0]
    else {
        unreachable!()
    };
    assert!((input_rate_hz - 395.0).abs() < 2.0);
    assert_eq!(*factor, 1);
    assert!(
        f.healths()
            .iter()
            .flatten()
            .any(|w| w.contains("below the configured"))
    );
}

#[test]
fn updates_per_second_sets_the_frame_cadence() {
    let mut f = feed(3200.0, 200.0, 5.0);
    f.run(3200.0, 2.0, 160);
    let n = f.frames().len();
    assert!((9..=11).contains(&n), "{n}");
}

#[test]
fn a_timestamp_gap_ends_the_frame_and_nothing_panics() {
    let mut f = feed(3200.0, 200.0, 5.0);
    f.run(3200.0, 1.0, 160);
    let before = f.frames().len();
    f.t += 2.0;
    f.run(3200.0, 1.0, 160);
    let frames = f.frames();
    assert!(frames.len() > before);
    // The frame pending at the gap is flushed early. The next one starts 2 s past what it covered.
    let flushed = frames[before];
    let first_after = frames[before + 1];
    let span = flushed.1 as f64 / (3200.0 / 6.0);
    assert!((first_after.0 - flushed.0 - span - 2.0).abs() < 0.01);
}

#[test]
fn the_first_frame_after_a_gap_is_not_shaped_by_samples_from_before_it() {
    let mut f = feed(3200.0, 200.0, 5.0);
    f.run(3200.0, 1.0, 160);
    // A large DC level up to the gap, silence after it.
    let level = |f: &mut Feed, value: f64, seconds: f64| {
        for _ in 0..(seconds * 3200.0 / 160.0) as usize {
            let mut b = batch(f.t, 3200.0, 160);
            for axis in [&mut b.samples.x, &mut b.samples.y, &mut b.samples.z] {
                axis.fill(value);
            }
            let now = f.base + Duration::from_secs_f64(f.clock);
            f.out.extend(f.pipe.push(&b, now));
            f.t += 0.05;
            f.clock += 0.05;
        }
    };
    level(&mut f, 1000.0, 1.0);
    let before = f.frames().len();
    f.t += 2.0;
    level(&mut f, 0.0, 1.0);

    let frames: Vec<&Vec<u8>> = f
        .out
        .iter()
        .filter_map(|o| match o {
            Output::Frame(b) => Some(b),
            _ => None,
        })
        .collect();
    // frames[before] is the one flushed at the gap. The next is the first made after it.
    let after = frames[before + 1];
    let first_x = f32::from_le_bytes(after[16..20].try_into().unwrap());
    assert!(first_x.abs() < 1.0, "pre-gap level leaked: {first_x}");
}

#[test]
fn a_non_finite_or_backwards_batch_is_dropped_with_a_warning() {
    let mut f = feed(3200.0, 200.0, 10.0);
    f.run(3200.0, 0.5, 160);
    let mut bad = batch(f.t, 3200.0, 160);
    bad.samples.x[3] = f64::NAN;
    let now = f.base + Duration::from_millis(600);
    f.out.extend(f.pipe.push(&bad, now));
    let back = batch(f.t - 1.0, 3200.0, 160);
    f.out.extend(f.pipe.push(&back, now));
    f.out.extend(f.pipe.tick(f.base + Duration::from_secs(2)));
    assert!(
        f.healths()
            .iter()
            .flatten()
            .any(|w| w.starts_with("2 batches were ignored"))
    );
}

#[test]
fn overflow_and_error_counters_raise_warnings() {
    let mut f = feed(3200.0, 200.0, 10.0);
    f.run(3200.0, 0.5, 160);
    let mut b = batch(f.t, 3200.0, 160);
    b.overflows = Some(1);
    b.errors = Some(1);
    f.out
        .extend(f.pipe.push(&b, f.base + Duration::from_millis(600)));
    let mut b = batch(f.t + 0.05, 3200.0, 160);
    b.overflows = Some(2);
    b.errors = Some(3);
    f.out
        .extend(f.pipe.push(&b, f.base + Duration::from_millis(700)));
    f.out.extend(f.pipe.tick(f.base + Duration::from_secs(2)));
    let warnings: Vec<String> = f.healths().into_iter().flatten().collect();
    assert!(warnings.iter().any(|w| w.contains("(overflows)")));
    assert!(warnings.iter().any(|w| w == "read errors"));
}

#[test]
fn a_rate_change_rebuilds_the_filter() {
    let mut f = feed(3200.0, 200.0, 10.0);
    f.run(3200.0, 1.5, 160);
    f.run(1600.0, 3.0, 160);
    let streams = f.streams();
    assert!(streams.len() >= 2);
    let ServerMessage::Stream { input_rate_hz, .. } = streams.last().unwrap() else {
        unreachable!()
    };
    assert!((input_rate_hz - 1600.0).abs() < 2.0);
}

#[test]
fn an_unusable_update_rate_is_clamped_instead_of_panicking() {
    for updates in [0.0, -1.0, f64::NAN] {
        let mut f = feed(3200.0, 200.0, updates);
        f.run(3200.0, 4.0, 320);
        let streams = f.streams();
        assert_eq!(streams.len(), 1, "{updates}");
        let ServerMessage::Stream {
            updates_per_second, ..
        } = streams[0]
        else {
            unreachable!()
        };
        assert_eq!(*updates_per_second, 0.5);
        // One frame every two seconds, so no frame is flushed before then.
        assert!(f.frames().len() <= 3, "{updates}: {}", f.frames().len());
    }
}
