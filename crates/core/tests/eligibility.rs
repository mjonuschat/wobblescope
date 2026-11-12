#[macro_use]
mod common;

use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Decision, Health, select_eligible};
use wobblescope_core::fit::FitParams;
use wobblescope_core::multichip::analyze;

fn input(chip: &str, point: usize, health: Health, coverage: Coverage) -> DatasetInput {
    DatasetInput {
        chip: chip.to_owned(),
        point,
        samples: RawSamples::default(),
        health,
        coverage,
        include_override: false,
        window: None,
        exclusions: Vec::new(),
        load_error: None,
    }
}

fn incomplete(what: &str) -> Coverage {
    Coverage::Incomplete {
        missing: what.to_owned(),
    }
}

parity_test! {
    fn degraded_chip_is_excluded_by_default() {
        let inputs = [
            input("a", 0, Health::Ok, Coverage::Complete),
            input("b", 0, Health::Degraded, Coverage::Complete),
        ];
        let (eligible, e) = select_eligible(&inputs);
        assert_eq!(eligible, vec![0]);
        assert_eq!(e.decisions[1].decision, Decision::Excluded);
        assert!(e.decisions[1].reasons.iter().any(|r| r == "degraded capture health"));
    }
}

parity_test! {
    fn incomplete_coverage_is_excluded_even_when_health_is_ok() {
        let inputs = [input("a", 0, Health::Ok, incomplete("last 0.4 s"))];
        let (eligible, e) = select_eligible(&inputs);
        assert!(eligible.is_empty());
        assert_eq!(e.decisions[0].decision, Decision::Excluded);
    }
}

parity_test! {
    fn override_includes_a_degraded_chip_and_lists_it_as_overridden() {
        let mut d = input("b", 0, Health::Degraded, Coverage::Complete);
        d.include_override = true;
        let (eligible, e) = select_eligible(&[d]);
        assert_eq!(eligible, vec![0]);
        assert_eq!(e.decisions[0].decision, Decision::Overridden);
        assert!(!e.decisions[0].reasons.is_empty());
    }
}

parity_test! {
    fn all_chips_ineligible_gives_no_recommendation_and_no_error() {
        let mut a = input("a", 0, Health::Degraded, Coverage::Complete);
        let mut b = input("b", 0, Health::Ok, incomplete("tail"));
        a.samples = common::load_csv("chip_a");
        b.samples = common::load_csv("chip_b");
        let analysis = analyze(&[a, b], &FitParams::default()).unwrap();
        assert!(analysis.worst_case.is_none());
        assert!(analysis.summed.is_none());
        assert!(analysis.per_chip.iter().all(|c| c.recommendation.is_none()));
        assert_eq!(analysis.per_chip.len(), 2);
        assert!(analysis.per_chip.iter().all(|c| !c.datasets.is_empty()));
    }
}

parity_test! {
    fn decisions_identify_the_point_and_reason_when_one_point_is_incomplete() {
        let inputs = [
            input("a", 0, Health::Ok, Coverage::Complete),
            input("a", 1, Health::Ok, incomplete("tail of point 1")),
        ];
        let (eligible, e) = select_eligible(&inputs);
        assert_eq!(eligible, vec![0]);
        assert_eq!(e.decisions[0].decision, Decision::Included);
        assert_eq!(e.decisions[1].point, 1);
        assert_eq!(e.decisions[1].decision, Decision::Excluded);
        assert_eq!(e.decisions[1].reasons, vec!["incomplete coverage: tail of point 1"]);
    }
}

parity_test! {
    fn chip_health_is_the_worst_of_its_inputs() {
        let mk = |point, health| {
            let mut d = input("a", point, health, Coverage::Complete);
            d.samples = common::load_csv("chip_b");
            d
        };
        let analysis = analyze(
            &[mk(0, Health::Ok), mk(1, Health::Ok), mk(2, Health::Degraded)],
            &FitParams::default(),
        )
        .unwrap();
        assert_eq!(analysis.per_chip[0].health, Health::Degraded);
        assert_eq!(analysis.per_chip[0].datasets.len(), 3);
    }
}

parity_test! {
    fn an_exclusion_reason_excludes_a_healthy_input_and_the_override_includes_it() {
        let mut d = input("lis", 0, Health::Ok, Coverage::Complete);
        d.exclusions.push("undersampled".to_owned());
        let (eligible, e) = select_eligible(std::slice::from_ref(&d));
        assert!(eligible.is_empty());
        assert_eq!(e.decisions[0].decision, Decision::Excluded);
        assert_eq!(e.decisions[0].reasons, vec!["undersampled"]);
        d.include_override = true;
        let (eligible, e) = select_eligible(&[d]);
        assert_eq!(eligible, vec![0]);
        assert_eq!(e.decisions[0].decision, Decision::Overridden);
        assert_eq!(e.decisions[0].reasons, vec!["undersampled"]);
    }
}

#[test]
fn health_is_judged_from_what_was_seen_and_the_worse_of_two_wins() {
    assert_eq!(Health::judged(true), Health::Degraded);
    assert_eq!(Health::judged(false), Health::Ok);

    let all = [Health::Ok, Health::Degraded];
    for a in all {
        assert_eq!(a.worse(a), a);
        assert_eq!(a.worse(Health::Ok), a);
        assert_eq!(a.worse(Health::Degraded), Health::Degraded);
        for b in all {
            assert_eq!(a.worse(b), b.worse(a));
        }
    }
}

mod values {
    use wobblescope_core::RawSamples;
    use wobblescope_core::signal::{GRAVITY, value_problems};

    fn samples(f: impl Fn(usize) -> [f64; 3]) -> RawSamples {
        let mut s = RawSamples::default();
        for k in 0..2000 {
            let [x, y, z] = f(k);
            s.t.push(k as f64 * 0.001);
            s.x.push(x);
            s.y.push(y);
            s.z.push(z);
        }
        s
    }

    fn wobble(k: usize) -> [f64; 3] {
        let p = k as f64 * 0.37;
        [
            200.0 * p.sin(),
            150.0 * (p * 1.3).cos(),
            GRAVITY + 100.0 * (p * 0.7).sin(),
        ]
    }

    #[test]
    fn a_vibrating_sensor_under_gravity_has_no_problems() {
        assert!(value_problems(&samples(wobble)).is_empty());
    }

    #[test]
    fn a_still_sensor_with_a_few_values_is_not_clipped() {
        let still = samples(|k| [(k % 3) as f64, (k % 2) as f64, GRAVITY + (k % 3) as f64]);
        assert!(value_problems(&still).is_empty());
    }

    #[test]
    fn a_constant_axis_is_reported() {
        let notes = value_problems(&samples(|k| [wobble(k)[0], 5.0, wobble(k)[2]]));
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("y axis never changes"), "{notes:?}");
    }

    #[test]
    fn a_flat_topped_axis_is_reported_as_clipped() {
        let notes = value_problems(&samples(|k| {
            let [x, y, z] = wobble(k);
            [(x * 100.0).clamp(-2.0 * GRAVITY, 2.0 * GRAVITY), y, z]
        }));
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("x axis sits at its extreme"), "{notes:?}");
    }

    #[test]
    fn a_quiet_signal_with_a_one_sided_transient_is_not_clipped() {
        let quiet = samples(|k| {
            let [x, y, z] = wobble(k);
            let x = if k < 1500 { 0.0 } else { x.abs() + 50.0 };
            let x = (x / 10.0).round() * 10.0;
            [x, y, z]
        });
        assert!(
            value_problems(&quiet).is_empty(),
            "{:?}",
            value_problems(&quiet)
        );
    }

    #[test]
    fn a_mean_far_from_gravity_is_reported() {
        let notes = value_problems(&samples(|k| {
            let [x, y, z] = wobble(k);
            [x, y, z * 0.5]
        }));
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("not near gravity"), "{notes:?}");
    }

    #[test]
    fn a_short_recording_is_not_judged() {
        let mut s = samples(|_| [0.0; 3]);
        s = s.slice(0..99);
        assert!(value_problems(&s).is_empty());
    }
}
