mod common;

use std::time::Duration;

use common::fake_moonraker::{FakeMoonraker, eventually};
use tokio::sync::mpsc;
use wobblescope::capture::availability::OpenChip;
use wobblescope::capture::recorder::{Level, Missing, Recorder};
use wobblescope::moonraker::bridge::Bridge;
use wobblescope::moonraker::chips::{Availability, Chip, ChipType};
use wobblescope_core::eligibility::{Coverage, Health};

const DEADLINE: Duration = Duration::from_secs(10);
const RATE: f64 = 3200.0;

fn chip() -> Chip {
    Chip {
        section: "adxl345".to_owned(),
        chip_type: ChipType::Adxl345,
        sensor: Some("adxl345".to_owned()),
        dump_endpoint: "adxl345/dump_adxl345".to_owned(),
        nominal_rate_hz: Some(RATE),
        mcu: None,
        axes_map: None,
        availability: Availability::Unchecked,
    }
}

async fn recorder_with(
    fake: &FakeMoonraker,
    cap: Option<usize>,
    levels: Option<mpsc::UnboundedSender<Level>>,
) -> Recorder {
    let bridge = Bridge::open(
        &fake.bridge_url("T"),
        "adxl345/dump_adxl345",
        Some("adxl345"),
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    let open = OpenChip {
        chip: chip(),
        bridge,
        observed_rate_hz: RATE,
        first_batches: vec![],
    };
    match cap {
        Some(cap) => Recorder::spawn_capped(open, levels, cap),
        None => Recorder::spawn(open, levels),
    }
}

async fn recorder(fake: &FakeMoonraker) -> Recorder {
    recorder_with(fake, None, None).await
}

/// Pushes samples at `RATE` from `from` to `to` seconds, in batches of 100 ms. `counters` is
/// what each batch reports. Returns once the recorder has seen the last one.
async fn push(
    fake: &FakeMoonraker,
    rec: &Recorder,
    from: f64,
    to: f64,
    counters: Option<(u64, u64)>,
) {
    let step = 1.0 / RATE;
    let per_batch = (0.1 * RATE) as usize;
    let mut n = 0usize;
    let last = loop {
        let rows: Vec<[f64; 4]> = (0..per_batch)
            .map(|k| {
                [
                    from + (n + k) as f64 * step,
                    1.0 + (k % 7) as f64,
                    0.5,
                    9800.0,
                ]
            })
            .collect();
        n += per_batch;
        let last = rows.last().unwrap()[0];
        fake.bridge().push("adxl345", &rows, counters);
        if last >= to {
            break last;
        }
    };
    eventually("the recorder to see the data", DEADLINE, || {
        rec.last_timestamp().is_some_and(|t| t >= last - 1e-9)
    })
    .await;
}

#[tokio::test]
async fn five_seconds_around_a_window_are_complete_ok_and_at_the_right_rate() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    push(&fake, &rec, 10.0, 10.0, Some((0, 0))).await;
    rec.begin_segment();
    push(&fake, &rec, 20.0, 25.0, Some((0, 0))).await;
    rec.wait_past(24.0, Duration::from_secs(1)).await.unwrap();
    let taken = rec.take(21.0, 24.0);
    assert_eq!(taken.report.coverage, Coverage::Complete);
    assert!(!taken.report.degraded, "{:?}", taken.report.notes);
    let rate = taken.report.observed_rate_hz.unwrap();
    assert!((rate - RATE).abs() / RATE < 0.01, "{rate}");
    assert!(taken.samples.len() >= 16_000);
    assert_eq!(rec.summary().health, Health::Ok);
}

#[tokio::test]
async fn clean_timestamps_without_counters_give_unknown_health() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    rec.begin_segment();
    push(&fake, &rec, 20.0, 23.0, None).await;
    let taken = rec.take(20.5, 22.5);
    assert_eq!(taken.report.coverage, Coverage::Complete);
    assert!(!taken.report.degraded);
    assert_eq!(rec.summary().health, Health::Unknown);
}

#[tokio::test]
async fn data_that_stops_short_of_the_window_end_gives_no_data_and_incomplete() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    rec.begin_segment();
    push(&fake, &rec, 20.0, 23.0, Some((0, 0))).await;
    // The window ends 0.01 s after the last sample the chip delivered.
    let last = rec.last_timestamp().unwrap();
    let end = last + 0.01;
    assert_eq!(
        rec.wait_past(end, Duration::from_millis(300)).await,
        Err(Missing::NoData)
    );
    let taken = rec.take(21.0, end);
    let Coverage::Incomplete { missing } = taken.report.coverage else {
        panic!("expected incomplete");
    };
    assert!(missing.contains("after the window end"), "{missing}");
}

#[tokio::test]
async fn a_first_sample_after_the_window_start_is_incomplete_naming_the_start() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    rec.begin_segment();
    push(&fake, &rec, 22.0, 25.0, Some((0, 0))).await;
    let taken = rec.take(21.0, 24.0);
    let Coverage::Incomplete { missing } = taken.report.coverage else {
        panic!("expected incomplete");
    };
    assert!(missing.contains("after the window start"), "{missing}");
}

#[tokio::test]
async fn a_rise_in_the_error_counter_inside_the_segment_is_degraded_naming_it() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    push(&fake, &rec, 10.0, 10.0, Some((0, 0))).await;
    rec.begin_segment();
    push(&fake, &rec, 20.0, 21.0, Some((0, 0))).await;
    push(&fake, &rec, 21.1, 22.0, Some((3, 0))).await;
    let taken = rec.take(20.2, 21.8);
    assert!(taken.report.degraded);
    assert!(
        taken
            .report
            .notes
            .iter()
            .any(|n| n.contains("errors counter rose by 3"))
    );
    let summary = rec.summary();
    assert_eq!(summary.health, Health::Degraded);
    assert!(summary.notes[0].starts_with("segment 1: "));
}

#[tokio::test]
async fn the_counter_baseline_carries_across_the_segment_boundary() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    push(&fake, &rec, 10.0, 10.0, Some((0, 0))).await;
    rec.begin_segment();
    // The segment's own first batch is the one that reports the rise.
    push(&fake, &rec, 20.0, 21.0, Some((1, 0))).await;
    let taken = rec.take(20.2, 20.8);
    assert!(taken.report.degraded);
}

#[tokio::test]
async fn a_counter_that_was_already_high_and_stays_is_ok() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    push(&fake, &rec, 10.0, 10.0, Some((5, 5))).await;
    rec.begin_segment();
    push(&fake, &rec, 20.0, 21.0, Some((5, 5))).await;
    let taken = rec.take(20.2, 20.8);
    assert!(!taken.report.degraded, "{:?}", taken.report.notes);
}

#[tokio::test]
async fn a_ten_millisecond_hole_is_degraded_and_jitter_that_keeps_the_order_is_not() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    rec.begin_segment();
    let step = 1.0 / RATE;
    let mut rows: Vec<[f64; 4]> = (0..3200)
        .map(|k| [20.0 + k as f64 * step, 0.0, 0.0, 9800.0])
        .collect();
    // Jitter well under the limit that keeps the samples in order.
    rows[500][0] += 0.0002;
    for r in &mut rows[1600..] {
        r[0] += 0.010;
    }
    fake.bridge().push("adxl345", &rows, Some((0, 0)));
    let last = rows.last().unwrap()[0];
    eventually("the data", DEADLINE, || {
        rec.last_timestamp().is_some_and(|t| t >= last - 1e-9)
    })
    .await;
    let taken = rec.take(20.0, last);
    assert!(taken.report.degraded);
    let gaps: Vec<&String> = taken
        .report
        .notes
        .iter()
        .filter(|n| n.contains("gap"))
        .collect();
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert!(gaps[0].contains("ms"), "{}", gaps[0]);
}

/// Pushes one second of samples at `RATE` from `from`, with `edit` applied to the rows first,
/// and waits until the recorder has seen the last one.
async fn push_edited(
    fake: &FakeMoonraker,
    rec: &Recorder,
    from: f64,
    edit: impl FnOnce(&mut Vec<[f64; 4]>),
) {
    let step = 1.0 / RATE;
    let mut rows: Vec<[f64; 4]> = (0..3200)
        .map(|k| [from + k as f64 * step, 0.0, 0.0, 9800.0])
        .collect();
    edit(&mut rows);
    fake.bridge().push("adxl345", &rows, Some((0, 0)));
    let last = rows.iter().map(|r| r[0]).fold(f64::NEG_INFINITY, f64::max);
    eventually("the data", DEADLINE, || {
        rec.last_timestamp().is_some_and(|t| t >= last - 1e-9)
    })
    .await;
}

#[tokio::test]
async fn a_repeated_timestamp_inside_the_window_is_degraded_naming_it() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    rec.begin_segment();
    let mut repeated = 0.0;
    push_edited(&fake, &rec, 20.0, |rows| {
        rows[1000][0] = rows[999][0];
        repeated = rows[999][0];
    })
    .await;
    let taken = rec.take(20.1, 20.9);
    assert!(taken.report.degraded, "{:?}", taken.report.notes);
    assert_eq!(taken.report.health, Health::Degraded);
    let order: Vec<&String> = taken
        .report
        .notes
        .iter()
        .filter(|n| n.contains("backwards"))
        .collect();
    assert_eq!(order.len(), 1, "{:?}", taken.report.notes);
    assert!(
        order[0].contains("1 time") && order[0].contains(&format!("{repeated:.3}")),
        "{}",
        order[0]
    );
    assert_eq!(rec.summary().health, Health::Degraded);
}

#[tokio::test]
async fn a_backward_step_before_the_window_still_degrades_the_segment() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    rec.begin_segment();
    push_edited(&fake, &rec, 20.0, |rows| rows.swap(100, 101)).await;
    let taken = rec.take(20.5, 20.9);
    assert!(taken.report.degraded, "{:?}", taken.report.notes);
    assert!(taken.report.notes.iter().any(|n| n.contains("backwards")));
    assert!(
        taken.report.notes.iter().all(|n| !n.contains("gap")),
        "{:?}",
        taken.report.notes
    );
}

#[tokio::test]
async fn a_segment_at_the_sample_cap_stops_accepting_and_says_so() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder_with(&fake, Some(1000), None).await;
    rec.begin_segment();
    push(&fake, &rec, 20.0, 22.0, Some((0, 0))).await;
    let taken = rec.take(20.0, 22.0);
    assert_eq!(taken.samples.len(), 1000);
    assert!(taken.report.capped);
    assert!(
        taken
            .report
            .notes
            .iter()
            .any(|n| n.contains("sample limit"))
    );
}

#[tokio::test]
async fn levels_arrive_at_most_four_times_a_second() {
    let fake = FakeMoonraker::start().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let rec = recorder_with(&fake, None, Some(tx)).await;
    rec.begin_segment();
    push(&fake, &rec, 20.0, 25.0, Some((0, 0))).await;
    let mut n = 0;
    while let Ok(level) = rx.try_recv() {
        assert_eq!(level.chip, "adxl345");
        assert!(level.rms[0] > 0.0 && level.rms[2] < 1e-6);
        n += 1;
    }
    // Five seconds of sample time: four a second, and the first batch opens the count.
    assert!((16..=21).contains(&n), "{n}");
}

#[tokio::test]
async fn a_closed_connection_is_reported_and_wakes_a_waiter() {
    let fake = FakeMoonraker::start().await;
    let rec = recorder(&fake).await;
    fake.bridge().drop_sensor("adxl345");
    let reason = tokio::time::timeout(DEADLINE, rec.lost()).await.unwrap();
    assert!(reason.contains("closed"), "{reason}");
    assert!(matches!(
        rec.wait_past(1e9, Duration::from_secs(1)).await,
        Err(Missing::Closed(_))
    ));
}
