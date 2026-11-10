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
    fn unknown_health_is_included_with_a_warning_naming_the_chip() {
        let (eligible, e) = select_eligible(&[input("adxl-left", 0, Health::Unknown, Coverage::Complete)]);
        assert_eq!(eligible, vec![0]);
        assert_eq!(e.decisions[0].decision, Decision::Included);
        assert_eq!(e.decisions[0].chip, "adxl-left");
        assert_eq!(e.decisions[0].reasons, vec!["unknown capture health"]);
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
            &[mk(0, Health::Ok), mk(1, Health::Unknown), mk(2, Health::Degraded)],
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
    assert_eq!(Health::judged(true, true), Health::Degraded);
    assert_eq!(Health::judged(true, false), Health::Degraded);
    assert_eq!(Health::judged(false, true), Health::Ok);
    assert_eq!(Health::judged(false, false), Health::Unknown);

    let all = [Health::Ok, Health::Unknown, Health::Degraded];
    for a in all {
        assert_eq!(a.worse(a), a);
        assert_eq!(a.worse(Health::Ok), a);
        assert_eq!(a.worse(Health::Degraded), Health::Degraded);
        for b in all {
            assert_eq!(a.worse(b), b.worse(a));
        }
    }
    assert_eq!(Health::Unknown.worse(Health::Ok), Health::Unknown);
}
