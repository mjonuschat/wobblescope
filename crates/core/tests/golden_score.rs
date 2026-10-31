#[macro_use]
mod common;

use common::{case, close};
use wobblescope_core::CoreError;
use wobblescope_core::score::{
    estimate_shaper, find_max_accel, remaining_vibrations, shaper_smoothing,
};
use wobblescope_core::shaper::{DEFAULT_DAMPING_RATIO, builtin};

parity_test! {
    fn smoothing_and_max_accel_match_klipper_for_every_candidate() {
        let golden = case("chip_a");
        for (name, entry) in &golden.fit {
            let shaper = builtin(name).unwrap();
            for c in &entry.candidates {
                let imp = shaper.impulses(c.freq, DEFAULT_DAMPING_RATIO).unwrap();
                close(&format!("{name}@{} smoothing", c.freq), shaper_smoothing(&imp, 5000.0, 5.0), c.smoothing);
                close(&format!("{name}@{} max_accel", c.freq), find_max_accel(&imp, 5.0), c.max_accel);
            }
        }
    }
}

parity_test! {
    fn remaining_vibrations_errors_on_zero_psd() {
        let imp = builtin("zv").unwrap().impulses(40.0, 0.1).unwrap();
        let bins = [10.0, 20.0, 30.0];
        assert_eq!(
            remaining_vibrations(&imp, 0.1, &bins, &[0.0, 0.0, 0.0]),
            Err(CoreError::FlatSignal)
        );
    }
}

parity_test! {
    fn estimate_shaper_is_one_at_zero_frequency() {
        let imp = builtin("ei").unwrap().impulses(40.0, 0.1).unwrap();
        let v = estimate_shaper(&imp, 0.1, &[0.0]);
        assert!((v[0] - 1.0).abs() < 1e-12);
    }
}
