#[macro_use]
mod common;

use common::{assert_close, case, check_recommendation, load_csv, normalized};
use wobblescope_core::eligibility::{Coverage, DatasetInput, Health};
use wobblescope_core::fit::FitParams;
use wobblescope_core::multichip::{analyze, summed_psd};

fn input(chip: &str, health: Health) -> DatasetInput {
    DatasetInput {
        chip: chip.to_owned(),
        point: 0,
        samples: load_csv(chip),
        health,
        coverage: Coverage::Complete,
        include_override: false,
        window: None,
        exclusions: Vec::new(),
        load_error: None,
    }
}

parity_test! {
    fn worst_case_of_chip_a_and_chip_b_matches_klipper_multi_file_run() {
        let a = analyze(&[input("chip_a", Health::Ok), input("chip_b", Health::Ok)], &FitParams::default())
            .unwrap();
        check_recommendation("worst case", a.worst_case.as_ref().unwrap(), &case("chip_a+chip_b"));
    }
}

parity_test! {
    fn worst_case_vibrations_name_the_dataset_that_sets_the_answer() {
        let a = analyze(&[input("chip_a", Health::Ok), input("chip_b", Health::Ok)], &FitParams::default())
            .unwrap();
        let vibrations = &a.worst_case_vibrations;
        let names: Vec<_> = vibrations.iter().map(|v| (v.chip.as_str(), v.point)).collect();
        assert_eq!(names, [("chip_a", 0), ("chip_b", 0)]);
        let largest = vibrations.iter().map(|v| v.vibrs).fold(0.0, f64::max);
        assert_eq!(largest, a.worst_case.as_ref().unwrap().best.vibrs);

        let a = analyze(&[input("chip_a", Health::Ok), input("chip_b", Health::Degraded)], &FitParams::default())
            .unwrap();
        assert_eq!(a.worst_case_vibrations.len(), 1);
        assert_eq!(a.worst_case_vibrations[0].chip, "chip_a");
    }
}

parity_test! {
    fn per_chip_recommendations_match_single_file_runs() {
        let a = analyze(&[input("chip_a", Health::Ok), input("chip_b", Health::Ok)], &FitParams::default())
            .unwrap();
        for (chip, analysis) in ["chip_a", "chip_b"].iter().zip(&a.per_chip) {
            assert_eq!(&analysis.chip, chip);
            check_recommendation(chip, analysis.recommendation.as_ref().unwrap(), &case(chip));
        }
    }
}

parity_test! {
    fn summed_psd_of_two_identical_datasets_is_twice_one() {
        let d = normalized("chip_b");
        let one = summed_psd(std::slice::from_ref(&d), 200.0).unwrap();
        let two = summed_psd(&[d.clone(), d], 200.0).unwrap();
        assert_eq!(one.freq_bins, two.freq_bins);
        let doubled: Vec<f64> = one.psd.iter().map(|v| v * 2.0).collect();
        assert_close("summed", &two.psd, &doubled);
        assert!(summed_psd(&[], 200.0).is_none());
    }
}

parity_test! {
    fn degraded_chip_is_absent_from_worst_case_but_its_psd_is_still_returned() {
        let a = analyze(&[input("chip_a", Health::Ok), input("chip_b", Health::Degraded)], &FitParams::default())
            .unwrap();
        check_recommendation("worst case", a.worst_case.as_ref().unwrap(), &case("chip_a"));
        assert!(a.per_chip[1].recommendation.is_none());
        assert_eq!(a.per_chip[1].datasets.len(), 1);
        assert!(a.per_chip[1].datasets[0].1.is_normalized());
    }
}

fn real_input(chip: &str, axis: &str) -> DatasetInput {
    DatasetInput {
        chip: chip.to_owned(),
        point: 0,
        samples: load_csv(&format!("ws_{axis}_{chip}")),
        health: Health::Unknown,
        coverage: Coverage::Complete,
        include_override: false,
        window: None,
        exclusions: Vec::new(),
        load_error: None,
    }
}

parity_test! {
    fn real_two_chip_worst_case_and_per_chip_match_klipper() {
        for axis in ["x", "y"] {
            let a = analyze(
                &[real_input("adxl345", axis), real_input("ebb36", axis)],
                &FitParams::default(),
            )
            .unwrap();
            check_recommendation(
                &format!("real {axis} worst case"),
                a.worst_case.as_ref().unwrap(),
                &case(&format!("real_{axis}_both")),
            );
            for (chip, c) in ["adxl345", "ebb36"].iter().zip(&a.per_chip) {
                check_recommendation(
                    &format!("real {axis} {chip}"),
                    c.recommendation.as_ref().unwrap(),
                    &case(&format!("real_{axis}_{chip}")),
                );
            }
        }
    }
}
